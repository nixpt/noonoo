//! The tokenizer (HTML §13.2.5): markup in, [`Token`]s out, one at a time.
//!
//! It is driven by the tree builder, because what the next characters mean
//! depends on the tree: a `<title>`'s content is text in HTML but markup in
//! SVG, and `<![CDATA[` opens a section only in foreign content. The builder
//! switches [`Content`] after a start tag and says where CDATA is allowed.
//!
//! Every state scans forward only, so tokenizing is linear in the source.

use std::borrow::Cow;

use crate::{Token, entities};

/// A tag's attributes, as in [`Token::StartTag`].
type Attrs = Vec<(String, Option<String>)>;

/// What the tokenizer reads next.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Content {
    /// Markup.
    Data,
    /// Text with character references, up to the named end tag.
    RcData(&'static str),
    /// Text, up to the named end tag.
    RawText(&'static str),
    /// A script's text, up to its end tag.
    Script,
    /// Text, to the end of the source.
    Plaintext,
}

impl Content {
    /// What the content of an HTML element named `name` is.
    pub(crate) fn of(name: &str) -> Content {
        match name {
            "title" => Content::RcData("title"),
            "textarea" => Content::RcData("textarea"),
            "style" => Content::RawText("style"),
            "xmp" => Content::RawText("xmp"),
            "iframe" => Content::RawText("iframe"),
            "noembed" => Content::RawText("noembed"),
            "noframes" => Content::RawText("noframes"),
            "script" => Content::Script,
            "plaintext" => Content::Plaintext,
            _ => Content::Data,
        }
    }

    /// The text `source` is, read whole as this (non-markup) content.
    pub(crate) fn text(self, source: &str) -> String {
        no_nulls(source).into_owned()
    }
}

/// `source` with every newline as `\n`, as the input stream is normalized.
pub(crate) fn normalize_newlines(source: &str) -> Cow<'_, str> {
    if source.contains('\r') {
        Cow::Owned(source.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(source)
    }
}

/// `source` with U+0000 replaced, as everywhere but in markup's own text.
fn no_nulls(source: &str) -> Cow<'_, str> {
    if source.contains('\0') {
        Cow::Owned(source.replace('\0', "\u{FFFD}"))
    } else {
        Cow::Borrowed(source)
    }
}

/// The tokenizer's place in a source whose newlines are normalized.
pub(crate) struct Tokenizer<'a> {
    source: &'a str,
    pos: usize,
    content: Content,
}

impl<'a> Tokenizer<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        Self {
            source,
            pos: 0,
            content: Content::Data,
        }
    }

    /// Read `content` from here on.
    pub(crate) fn switch_to(&mut self, content: Content) {
        self.content = content;
    }

    /// The next token, or `None` at the end. `cdata` says whether
    /// `<![CDATA[` opens a section here (in foreign content).
    pub(crate) fn next(&mut self, cdata: bool) -> Option<Token> {
        loop {
            if self.pos >= self.source.len() {
                return None;
            }
            match self.content {
                Content::Data => {
                    if let Some(token) = self.data(cdata) {
                        return Some(token);
                    }
                }
                content => {
                    let start = self.pos;
                    let end = match content {
                        Content::RcData(name) | Content::RawText(name) => {
                            self.find_end_tag(start, name)
                        }
                        Content::Script => self.find_script_end(start),
                        Content::Data | Content::Plaintext => None,
                    };
                    // Stop *before* the end tag, which is markup again.
                    self.pos = end.unwrap_or(self.source.len());
                    self.content = Content::Data;
                    if self.pos > start {
                        return Some(Token::Text(content.text(&self.source[start..self.pos])));
                    }
                }
            }
        }
    }

    /// One token of markup; `None` for markup that is no token, after which
    /// the source goes on.
    fn data(&mut self, cdata: bool) -> Option<Token> {
        let source = self.source;
        let bytes = source.as_bytes();
        // Text runs to the next `<` that starts markup.
        let start = self.pos;
        let mut end = start;
        loop {
            match source[end..].find('<') {
                None => end = source.len(),
                Some(offset) => {
                    end += offset;
                    if !starts_markup(&bytes[end..]) {
                        end += 1;
                        continue;
                    }
                }
            }
            break;
        }
        if end > start {
            self.pos = end;
            return Some(Token::Text(entities::decode(&source[start..end])));
        }

        match bytes.get(start + 1) {
            Some(b'!') => Some(self.declaration(cdata)),
            Some(b'?') => self.processing_instruction(start + 1),
            Some(b'/') => match bytes.get(start + 2) {
                Some(b) if b.is_ascii_alphabetic() => {
                    let (name, ..) = self.tag(start + 2)?;
                    Some(Token::EndTag { name })
                }
                Some(b'>') => {
                    self.pos = start + 3;
                    None
                }
                _ => Some(self.bogus_comment(start + 2)),
            },
            _ => {
                let (name, attrs, self_closing) = self.tag(start + 1)?;
                Some(Token::StartTag {
                    name,
                    attrs,
                    self_closing,
                })
            }
        }
    }

    /// `<!...`: a comment, a doctype or a CDATA section.
    fn declaration(&mut self, cdata: bool) -> Token {
        let source = self.source;
        let start = self.pos + 2;
        let rest = &source[start..];
        if rest.starts_with("--") {
            return self.comment(start + 2);
        }
        if let Some(keyword) = rest.get(..7) {
            if keyword.eq_ignore_ascii_case("doctype") {
                let end = rest.find('>').map_or(source.len(), |i| start + i);
                self.pos = (end + 1).min(source.len());
                return crate::doctype_token(source[start + 7..end].trim());
            }
            if cdata && keyword == "[CDATA[" {
                let text = start + 7;
                let end = source[text..].find("]]>").map(|i| text + i);
                self.pos = end.map_or(source.len(), |end| end + 3);
                let end = end.unwrap_or(source.len());
                return Token::Text(source[text..end].to_owned());
            }
        }
        self.bogus_comment(start)
    }

    /// A comment whose text starts at `start`, after `<!--`.
    fn comment(&mut self, start: usize) -> Token {
        let source = self.source;
        let rest = &source[start..];
        // `<!-->` and `<!--->` are empty.
        for abrupt in [">", "->"] {
            if rest.starts_with(abrupt) {
                self.pos = start + abrupt.len();
                return Token::Comment(String::new());
            }
        }
        let mut from = 0;
        while let Some(offset) = rest[from..].find("--") {
            let dashes = from + offset;
            let after = &rest[dashes + 2..];
            let close = if after.starts_with('>') {
                3
            } else if after.starts_with("!>") {
                4
            } else {
                from = dashes + 1;
                continue;
            };
            self.pos = start + dashes + close;
            return Token::Comment(no_nulls(&rest[..dashes]).into_owned());
        }
        // Cut off by the end of the source: what may start the close isn't
        // text.
        self.pos = source.len();
        let text = ["--!", "--", "-"]
            .iter()
            .find_map(|tail| rest.strip_suffix(tail))
            .unwrap_or(rest);
        Token::Comment(no_nulls(text).into_owned())
    }

    /// `<?...`, with the `?` at `start`. The DOM has no processing
    /// instruction nodes, so it becomes a comment; cut off by the end of the
    /// source it is nothing.
    fn processing_instruction(&mut self, start: usize) -> Option<Token> {
        let rest = &self.source[start + 1..];
        if !rest.contains('>') {
            let target_end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_')))
                .unwrap_or(rest.len());
            let target = &rest[..target_end];
            let named = rest.is_empty()
                || target.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_');
            let ended = rest[target_end..]
                .chars()
                .next()
                .is_none_or(|c| matches!(c, '\t' | '\n' | '\x0C' | ' ' | '?'));
            let reserved = target_end < rest.len()
                && (target.eq_ignore_ascii_case("xml")
                    || target.eq_ignore_ascii_case("xml-stylesheet"));
            if named && ended && !reserved {
                self.pos = self.source.len();
                return None;
            }
        }
        Some(self.bogus_comment(start))
    }

    /// Markup that is no known kind: a comment from `start` to the next `>`.
    fn bogus_comment(&mut self, start: usize) -> Token {
        let source = self.source;
        let end = source[start..].find('>').map(|i| start + i);
        self.pos = end.map_or(source.len(), |end| end + 1);
        let end = end.unwrap_or(source.len());
        Token::Comment(no_nulls(&source[start..end]).into_owned())
    }

    /// A start or end tag's name, attributes and whether it ends with `/>`,
    /// from its name at `start`. `None` if the source ends inside it, which
    /// makes it no tag at all.
    fn tag(&mut self, start: usize) -> Option<(String, Attrs, bool)> {
        let source = self.source;
        self.pos = source.len();
        let mut pos = scan(source, start, |c| matches!(c, '>' | '/'));
        let name = no_nulls(&source[start..pos]).to_ascii_lowercase();
        let mut attrs = Vec::new();
        loop {
            pos = skip_whitespace(source, pos);
            match source.as_bytes().get(pos)? {
                b'>' => {
                    self.pos = pos + 1;
                    return Some((name, attrs, false));
                }
                b'/' => {
                    pos += 1;
                    if source.as_bytes().get(pos) == Some(&b'>') {
                        self.pos = pos + 1;
                        return Some((name, attrs, true));
                    }
                    continue;
                }
                _ => {}
            }

            // A name's first character may be `=`.
            let attr_start = pos;
            let first = usize::from(source.as_bytes()[pos] == b'=');
            pos = scan(source, pos + first, |c| matches!(c, '=' | '>' | '/'));
            let attr_name = no_nulls(&source[attr_start..pos]).to_ascii_lowercase();

            pos = skip_whitespace(source, pos);
            if source.as_bytes().get(pos) != Some(&b'=') {
                attrs.push((attr_name, None));
                continue;
            }
            pos = skip_whitespace(source, pos + 1);
            let value = match source.as_bytes().get(pos)? {
                quote @ (b'"' | b'\'') => {
                    let value_start = pos + 1;
                    let end = value_start + source[value_start..].find(*quote as char)?;
                    pos = end + 1;
                    &source[value_start..end]
                }
                _ => {
                    let value_start = pos;
                    pos = scan(source, pos, |c| c == '>');
                    &source[value_start..pos]
                }
            };
            attrs.push((attr_name, Some(no_nulls(value).into_owned())));
        }
    }

    /// Where the `</name` ending a text element starts, from `from` on.
    fn find_end_tag(&self, from: usize, name: &str) -> Option<usize> {
        let mut pos = from;
        loop {
            pos += self.source[pos..].find("</")?;
            if self.is_tag(pos + 2, name) {
                return Some(pos);
            }
            pos += 2;
        }
    }

    /// Whether the tag name at `pos` is `name`, and complete.
    fn is_tag(&self, pos: usize, name: &str) -> bool {
        let bytes = &self.source.as_bytes()[pos..];
        bytes.len() > name.len()
            && bytes[..name.len()].eq_ignore_ascii_case(name.as_bytes())
            && matches!(
                bytes[name.len()],
                b'\t' | b'\n' | b'\x0C' | b' ' | b'/' | b'>'
            )
    }

    /// Where the `</script` ending a script starts. Inside `<!--`, a
    /// `<script>` hides its own `</script>` ("double escaped").
    fn find_script_end(&self, from: usize) -> Option<usize> {
        let bytes = self.source.as_bytes();
        let (mut escaped, mut double) = (false, false);
        let mut pos = from;
        loop {
            pos += self.source[pos..].find(['<', '-'])?;
            let rest = &self.source[pos..];
            if bytes[pos] == b'-' {
                if escaped && rest.starts_with("-->") {
                    (escaped, double) = (false, false);
                    pos += 3;
                } else {
                    pos += 1;
                }
            } else if rest.starts_with("</") && self.is_tag(pos + 2, "script") {
                if !double {
                    return Some(pos);
                }
                double = false;
                pos += 2;
            } else if escaped && !double && self.is_tag(pos + 1, "script") {
                double = true;
                pos += 1;
            } else if !escaped && rest.starts_with("<!--") {
                escaped = true;
                // The same dashes may close it: `<!-->`.
                pos += 2;
            } else {
                pos += 1;
            }
        }
    }
}

/// Whether `bytes`, which start with `<`, start markup rather than text.
fn starts_markup(bytes: &[u8]) -> bool {
    match bytes.get(1) {
        Some(b'!' | b'?') => true,
        Some(b'/') => bytes.len() > 2,
        Some(b) => b.is_ascii_alphabetic(),
        None => false,
    }
}

/// The end of the run from `pos` that has no whitespace and nothing `stop`
/// says ends it.
fn scan(source: &str, pos: usize, stop: impl Fn(char) -> bool) -> usize {
    source[pos..]
        .find(|c: char| c.is_whitespace() || stop(c))
        .map_or(source.len(), |offset| pos + offset)
}

/// The byte index after any whitespace at `pos`, stepping whole characters
/// (some whitespace, like U+2028, is several bytes).
fn skip_whitespace(source: &str, pos: usize) -> usize {
    let rest = &source[pos..];
    pos + (rest.len() - rest.trim_start().len())
}
