//! # noonoo-markdown
//!
//! A dependency-free Markdown parser: a practical CommonMark subset to HTML.
//! Because Noonoo already owns an HTML parser and the whole parse→style→layout→
//! paint pipeline, Markdown becomes a **viewer** for free — convert `.md` to
//! HTML and run it through the engine.
//!
//! Supported: ATX headings, paragraphs, emphasis/strong, inline code, fenced
//! code, blockquotes, (nested) ordered/unordered lists, links, images,
//! horizontal rules, hard breaks, and raw HTML passthrough.
//!
//! Not yet: reference links, tables, footnotes, task lists, and full
//! CommonMark emphasis edge cases.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Metadata parsed from a leading `---` front-matter block.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrontMatter {
    /// Key/value pairs, in order.
    pub entries: Vec<(String, String)>,
}

impl FrontMatter {
    /// The value for `key`, if present.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Whether there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate the entries.
    pub fn iter(&self) -> impl Iterator<Item = &(String, String)> {
        self.entries.iter()
    }
}

/// Split optional front matter from the Markdown body.
///
/// Front matter is a `---`-delimited block of `key: value` lines at the very
/// top of the file (values may be quoted). Returns `(metadata, body)`; when
/// there is no valid block the metadata is empty and the body is the whole
/// input.
pub fn split_front_matter(source: &str) -> (FrontMatter, &str) {
    let first_end = source.find('\n').map(|i| i + 1).unwrap_or(source.len());
    if source[..first_end].trim() != "---" {
        return (FrontMatter::default(), source);
    }

    let after = &source[first_end..];
    let mut entries = Vec::new();
    let mut consumed = 0;
    let mut closed = false;
    for line in after.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed == "---" || trimmed == "..." {
            consumed += line.len();
            closed = true;
            break;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            entries.push((
                key.trim().to_owned(),
                value.trim().trim_matches('"').to_owned(),
            ));
        }
        consumed += line.len();
    }

    if !closed {
        return (FrontMatter::default(), source);
    }
    (FrontMatter { entries }, &after[consumed..])
}

/// Convert Markdown to HTML.
pub fn markdown_to_html(source: &str) -> String {
    blocks(&source.lines().collect::<Vec<_>>())
}

/// A small stylesheet for rendered Markdown — pass it to the engine when
/// viewing a `.md` as a page.
pub const DEFAULT_STYLESHEET: &str = r#"
body { margin: 24px; color: #1f2328; background-color: #ffffff }
h1, h2, h3, h4, h5, h6 { color: #1f2328; font-weight: bold; margin: 16px 0 8px 0 }
h1 { font-size: 32px }
h2 { font-size: 24px }
h3 { font-size: 20px }
p { margin: 8px 0 }
a { color: #0969da }
code, pre { font-family: monospace }
pre { background-color: #f6f8fa; padding: 12px; margin: 8px 0 }
code { background-color: #f6f8fa }
blockquote { margin: 8px 0; padding: 0 12px; color: #57606a; border-left: 3px solid #d0d7de }
ul, ol { margin: 8px 0; padding-left: 24px }
li { margin: 2px 0 }
hr { margin: 16px 0 }
img { display: inline-block }
"#;

// --- block level -----------------------------------------------------------

fn blocks(lines: &[&str]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        let trimmed = line.trim_start();

        if is_fence(trimmed) {
            i = fence(lines, i, &mut out);
        } else if let Some((level, text)) = heading(trimmed) {
            out.push_str(&format!("<h{level}>{}</h{level}>", inline(text)));
            i += 1;
        } else if is_hr(trimmed) {
            out.push_str("<hr>");
            i += 1;
        } else if trimmed.starts_with('>') {
            i = blockquote(lines, i, &mut out);
        } else if list_marker(trimmed).is_some() {
            i = list(lines, i, &mut out);
        } else if trimmed.starts_with('<') {
            i = raw_html(lines, i, &mut out);
        } else {
            i = paragraph(lines, i, &mut out);
        }
    }
    out
}

fn fence(lines: &[&str], start: usize, out: &mut String) -> usize {
    let marker = lines[start].trim_start().chars().next().unwrap();
    let len = lines[start]
        .trim_start()
        .chars()
        .take_while(|&c| c == marker)
        .count();
    let info = lines[start].trim_start()[len..].trim().to_owned();
    let mut i = start + 1;
    let mut body = String::new();
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if trimmed.starts_with(marker)
            && trimmed.chars().take_while(|&c| c == marker).count() >= len
            && trimmed.trim().chars().all(|c| c == marker)
        {
            i += 1;
            break;
        }
        body.push_str(lines[i]);
        body.push('\n');
        i += 1;
    }
    let class = if info.is_empty() {
        String::new()
    } else {
        format!(" class=\"language-{}\"", escape(&info))
    };
    out.push_str(&format!("<pre><code{class}>{}</code></pre>", escape(&body)));
    i
}

fn blockquote(lines: &[&str], start: usize, out: &mut String) -> usize {
    let mut i = start;
    let mut inner = Vec::new();
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if let Some(rest) = trimmed.strip_prefix('>') {
            inner.push(rest.strip_prefix(' ').unwrap_or(rest));
            i += 1;
        } else {
            break;
        }
    }
    out.push_str("<blockquote>");
    out.push_str(&blocks(&inner));
    out.push_str("</blockquote>");
    i
}

fn paragraph(lines: &[&str], start: usize, out: &mut String) -> usize {
    let mut i = start;
    let mut text = String::new();
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if lines[i].trim().is_empty()
            || heading(trimmed).is_some()
            || is_hr(trimmed)
            || is_fence(trimmed)
            || trimmed.starts_with('>')
            || list_marker(trimmed).is_some()
        {
            break;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(lines[i].trim());
        i += 1;
    }
    out.push_str(&format!("<p>{}</p>", inline(&text)));
    i
}

fn raw_html(lines: &[&str], start: usize, out: &mut String) -> usize {
    let mut i = start;
    while i < lines.len() && !lines[i].trim().is_empty() {
        out.push_str(lines[i]);
        out.push('\n');
        i += 1;
    }
    i
}

fn list(lines: &[&str], start: usize, out: &mut String) -> usize {
    let base_indent = lines[start].len() - lines[start].trim_start().len();
    let ordered = list_marker(lines[start].trim_start()).is_some_and(|(ordered, _)| ordered);
    let mut items: Vec<String> = Vec::new();
    let mut i = start;

    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            break;
        }
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim_start();
        let Some((_, marker_len)) = list_marker(trimmed) else {
            break;
        };
        if indent != base_indent {
            break;
        }

        let mut item = trimmed[marker_len..].trim_start().to_owned();
        i += 1;

        // Continuation lines (deeper indent, incl. nested lists / blank lines).
        let mut rest: Vec<&str> = Vec::new();
        while i < lines.len() {
            let cont = lines[i];
            if cont.trim().is_empty() {
                rest.push("");
                i += 1;
                continue;
            }
            let cont_indent = cont.len() - cont.trim_start().len();
            if cont_indent > base_indent {
                rest.push(dedent(cont, base_indent + 1));
                i += 1;
            } else {
                break;
            }
        }

        if rest.iter().any(|l| !l.trim().is_empty()) {
            let mut content = std::mem::take(&mut item);
            content.push('\n');
            content.push_str(&rest.join("\n"));
            items.push(blocks(&content.lines().collect::<Vec<_>>()));
        } else {
            items.push(inline(&item));
        }
        while i < lines.len() && lines[i].trim().is_empty() {
            // A blank line between items is allowed within the list.
            if i + 1 < lines.len()
                && list_marker(lines[i + 1].trim_start()).is_some()
                && (lines[i + 1].len() - lines[i + 1].trim_start().len()) == base_indent
            {
                i += 1;
                continue;
            }
            break;
        }
    }

    let tag = if ordered { "ol" } else { "ul" };
    out.push_str(&format!("<{tag}>"));
    for item in items {
        out.push_str("<li>");
        out.push_str(&item);
        out.push_str("</li>");
    }
    out.push_str(&format!("</{tag}>"));
    i
}

/// Recognize a list marker at the start of a trimmed line, returning
/// `(ordered?, marker length in bytes)`.
fn list_marker(trimmed: &str) -> Option<(bool, usize)> {
    let bytes = trimmed.as_bytes();
    if let Some(first) = bytes.first()
        && matches!(first, b'-' | b'*' | b'+')
        && bytes.get(1) == Some(&b' ')
    {
        return Some((false, 1));
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && trimmed[digits..].starts_with(". ") {
        return Some((true, digits + 1));
    }
    None
}

fn dedent(line: &str, n: usize) -> &str {
    let mut count = 0;
    let mut idx = 0;
    for (i, ch) in line.char_indices() {
        if ch == ' ' && count < n {
            count += 1;
            idx = i + 1;
        } else {
            break;
        }
    }
    &line[idx..]
}

fn heading(trimmed: &str) -> Option<(usize, &str)> {
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) {
        let rest = &trimmed[hashes..];
        if rest.is_empty() {
            return Some((hashes, ""));
        }
        if let Some(text) = rest.strip_prefix(' ') {
            return Some((hashes, text.trim_end_matches('#').trim_end()));
        }
    }
    None
}

fn is_fence(trimmed: &str) -> bool {
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

fn is_hr(trimmed: &str) -> bool {
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    compact.len() >= 3
        && (compact.chars().all(|c| c == '-')
            || compact.chars().all(|c| c == '*')
            || compact.chars().all(|c| c == '_'))
}

// --- inline level ----------------------------------------------------------

fn inline(text: &str) -> String {
    let escaped = escape(text);
    let with_code = code_spans(&escaped);
    let with_media = media(&with_code);
    let with_links = links(&with_media);
    let strong = replace_pairs(&with_links, "**", "__", "strong");
    let em = replace_pairs(&strong, "*", "_", "em");
    hard_breaks(&em)
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

fn code_spans(text: &str) -> String {
    let mut out = String::new();
    let mut parts = text.split('`');
    let mut in_code = false;
    for (index, part) in parts.by_ref().enumerate() {
        if index > 0 {
            in_code = !in_code;
        }
        if in_code {
            out.push_str("<code>");
            out.push_str(part);
            out.push_str("</code>");
        } else {
            out.push_str(part);
        }
    }
    out
}

fn media(text: &str) -> String {
    // ![alt](src)
    find_delimited(text, "![", "](", ")").unwrap_or_else(|| text.to_owned())
}

fn links(text: &str) -> String {
    find_delimited(text, "[", "](", ")").unwrap_or_else(|| text.to_owned())
}

/// Replace `open text close` with `<tag>text</tag>` for the first occurrence and
/// wrap everywhere else. Simplistic: no nesting, no escaping subtleties.
fn find_delimited(text: &str, open: &str, mid: &str, close: &str) -> Option<String> {
    if !text.contains(open) {
        return None;
    }
    let mut out = String::new();
    let mut rest = text;
    let mut changed = false;
    while let Some(start) = rest.find(open) {
        out.push_str(&rest[..start]);
        let after = &rest[start + open.len()..];
        if let Some(mid_pos) = after.find(mid) {
            let label = &after[..mid_pos];
            let after_mid = &after[mid_pos + mid.len()..];
            if let Some(end) = after_mid.find(close) {
                let url = &after_mid[..end];
                if open == "![" {
                    out.push_str(&format!("<img src=\"{url}\" alt=\"{label}\">"));
                } else {
                    out.push_str(&format!("<a href=\"{url}\">{label}</a>"));
                }
                changed = true;
                rest = &after_mid[end + close.len()..];
                continue;
            }
        }
        out.push_str(open);
        rest = after;
    }
    out.push_str(rest);
    changed.then_some(out)
}

fn replace_pairs(text: &str, marker: &str, alt: &str, tag: &str) -> String {
    replace_marker(text, marker, tag)
        .unwrap_or_else(|| replace_marker(text, alt, tag).unwrap_or_else(|| text.to_owned()))
}

fn replace_marker(text: &str, marker: &str, tag: &str) -> Option<String> {
    if !text.contains(marker) {
        return None;
    }
    let mut out = String::new();
    let mut rest = text;
    let mut open = false;
    while let Some(pos) = rest.find(marker) {
        out.push_str(&rest[..pos]);
        if open {
            out.push_str(&format!("</{tag}>"));
        } else {
            out.push_str(&format!("<{tag}>"));
        }
        open = !open;
        rest = &rest[pos + marker.len()..];
    }
    out.push_str(rest);
    if open {
        out.push_str(marker); // unbalanced — leave a literal marker
    }
    Some(out)
}

fn hard_breaks(text: &str) -> String {
    let mut out = String::new();
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        if let Some(stripped) = line.strip_suffix("  ") {
            out.push_str(stripped);
            out.push_str("<br>");
        } else if let Some(stripped) = line.strip_suffix('\\') {
            out.push_str(stripped);
            out.push_str("<br>");
        } else {
            out.push_str(line);
        }
    }
    out
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_and_paragraphs() {
        assert_eq!(markdown_to_html("# Hi"), "<h1>Hi</h1>");
        assert_eq!(markdown_to_html("hello"), "<p>hello</p>");
        assert_eq!(markdown_to_html("## Two"), "<h2>Two</h2>");
    }

    #[test]
    fn inline_emphasis_and_code() {
        assert_eq!(markdown_to_html("*em*"), "<p><em>em</em></p>");
        assert_eq!(markdown_to_html("**bold**"), "<p><strong>bold</strong></p>");
        assert_eq!(markdown_to_html("`x`"), "<p><code>x</code></p>");
    }

    #[test]
    fn links_and_images() {
        assert_eq!(
            markdown_to_html("[a](http://x)"),
            "<p><a href=\"http://x\">a</a></p>"
        );
        assert_eq!(
            markdown_to_html("![alt](img.png)"),
            "<p><img src=\"img.png\" alt=\"alt\"></p>"
        );
    }

    #[test]
    fn fences_preserve_content() {
        let html = markdown_to_html("```rs\nfn x() {}\n```");
        assert_eq!(
            html,
            "<pre><code class=\"language-rs\">fn x() {}\n</code></pre>"
        );
    }

    #[test]
    fn lists_are_rendered() {
        assert_eq!(
            markdown_to_html("- a\n- b"),
            "<ul><li>a</li><li>b</li></ul>"
        );
        assert_eq!(
            markdown_to_html("1. one\n2. two"),
            "<ol><li>one</li><li>two</li></ol>"
        );
    }

    #[test]
    fn blockquote_and_hr() {
        assert_eq!(
            markdown_to_html("> quoted"),
            "<blockquote><p>quoted</p></blockquote>"
        );
        assert_eq!(markdown_to_html("---"), "<hr>");
    }

    #[test]
    fn escapes_html() {
        assert_eq!(markdown_to_html("a < b"), "<p>a &lt; b</p>");
    }

    #[test]
    fn splits_front_matter() {
        let source = "---\ntitle: \"Hello\"\ndraft: true\n---\n# Body\n";
        let (meta, body) = split_front_matter(source);
        assert_eq!(meta.get("title"), Some("Hello"));
        assert_eq!(meta.get("draft"), Some("true"));
        assert_eq!(body, "# Body\n");
    }

    #[test]
    fn no_front_matter_leaves_input_whole() {
        let (meta, body) = split_front_matter("# Title\n");
        assert!(meta.is_empty());
        assert_eq!(body, "# Title\n");
    }
}
