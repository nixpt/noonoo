//! # noonoo-html
//!
//! HTML ingestion: a tokenizer and a tree builder that produce a
//! [`noonoo_dom::Document`]. Downstream layers never see these token types.
//!
//! A correctness note the replaced engine earned the hard way: installing the
//! parser provider was optional there, and its absence was a *silent* no-op — so
//! `set_inner_html` quietly parsed nothing until every consumer remembered to
//! pass it. Here parsing is the crate's whole reason to exist; there is no
//! configuration under which markup silently fails to parse.
//!
//! ## Scope
//!
//! The tokenizer covers the shapes a configuration/UI document uses: doctype,
//! start/end tags with attributes, comments, text, and raw-text elements
//! (`script`, `style`, `textarea`, `title`). The tree builder is a
//! stack-based one — it handles nesting, void elements, and implicit closes by
//! matching end tags, plus the implied end tags and table sections real
//! markup leans on (an unclosed `<p>`, `<li>` or `<option>`, a `<tr>` with no
//! `<tbody>`; see `implied`). [`parse`] always gives a document its
//! `html` > `head` + `body` skeleton, filling in whichever of those tags the
//! source leaves out; fragments ([`set_inner_html`],
//! [`insert_adjacent_html`]) get none. It does **not** implement HTML5's full
//! insertion modes (foster parenting, active formatting elements). That is
//! deliberate scope, not an oversight; see `docs/ROADMAP.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod implied;
mod mode;

use noonoo_dom::{Document, MutError, NodeId};

/// A single HTML token.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Token {
    /// `<!doctype html>`.
    Doctype {
        /// The doctype name (`"html"` for the common case).
        name: String,
        /// The `PUBLIC` identifier, if any.
        public_id: Option<String>,
        /// The system identifier, if any.
        system_id: Option<String>,
    },
    /// A start tag.
    StartTag {
        /// The (lowercased) tag name.
        name: String,
        /// Attributes in source order; `None` value means a bare attribute.
        attrs: Vec<(String, Option<String>)>,
        /// Whether the tag ended with `/>`.
        self_closing: bool,
    },
    /// An end tag.
    EndTag {
        /// The (lowercased) tag name.
        name: String,
    },
    /// A comment.
    Comment(String),
    /// Character data.
    Text(String),
}

/// Elements that never have children.
pub const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

fn is_raw_text(name: &str) -> bool {
    matches!(name, "script" | "style" | "textarea" | "title")
}

fn is_void(name: &str) -> bool {
    VOID_ELEMENTS.contains(&name)
}

/// Pop the open cell/row elements that a new element of `name` implicitly
/// closes, following the spec's table insertion modes:
///
/// - a new cell (`td`/`th`) closes the open cell;
/// - a new row (`tr`) closes the open cell and the open row, and stays in
///   the open row group;
/// - a new row group (`tbody`/`thead`/`tfoot`, also `caption`/`colgroup`)
///   closes the open cell, row, and row group;
/// - a new `table` closes the open row structure — but a nested table grows
///   inside its open cell, so the cell stays.
///
/// An unclosed `<tr>`/`<td>` (the implied tbody/td closing) is therefore
/// structured correctly by the tree builder without the full insertion-mode
/// state machine. End tags already pop to their matching element.
fn implied_table_close(stack: &mut Vec<noonoo_dom::NodeId>, doc: &Document, name: &str) {
    // `stack[0]` is the node being built into: never closed.
    while stack.len() > 1 {
        let Some(&top) = stack.last() else { return };
        let Some(element) = doc.element(top) else {
            return;
        };
        let open = element.name.as_str();
        match (name, open) {
            // A nested `<table>` grows inside its open cell: the cell stays.
            ("table", "td" | "th") => return,
            (
                "td" | "th" | "tr" | "tbody" | "thead" | "tfoot" | "caption" | "colgroup",
                "td" | "th",
            ) => {}
            ("tr", "tr") => {}
            (
                "tbody" | "thead" | "tfoot" | "caption" | "colgroup" | "table",
                "tr" | "tbody" | "thead" | "tfoot",
            ) => {}
            _ => return,
        }
        stack.pop();
    }
}
/// Tokenize an HTML source.
pub fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    let bytes = source.as_bytes();

    while pos < source.len() {
        if bytes[pos] != b'<' {
            let start = pos;
            while pos < source.len() && bytes[pos] != b'<' {
                pos += 1;
            }
            tokens.push(Token::Text(entities::decode(&source[start..pos])));
            continue;
        }

        if source[pos..].starts_with("<!--") {
            let start = pos + 4;
            let end = source[start..]
                .find("-->")
                .map(|i| start + i)
                .unwrap_or(source.len());
            tokens.push(Token::Comment(source[start..end].to_owned()));
            pos = (end + 3).min(source.len());
            continue;
        }

        if source[pos..].starts_with("<!") {
            let start = pos + 2;
            let end = source[start..]
                .find('>')
                .map(|i| start + i)
                .unwrap_or(source.len());
            let inner = source[start..end].trim();
            tokens.push(doctype_token(inner));
            pos = (end + 1).min(source.len());
            continue;
        }

        if source[pos..].starts_with("</") {
            let start = pos + 2;
            let end = source[start..]
                .find('>')
                .map(|i| start + i)
                .unwrap_or(source.len());
            let name = source[start..end].trim().to_ascii_lowercase();
            tokens.push(Token::EndTag { name });
            pos = (end + 1).min(source.len());
            continue;
        }

        if pos + 1 < source.len() && (bytes[pos + 1].is_ascii_alphabetic()) {
            let (token, next) = parse_start_tag(source, pos);
            let raw_name = match &token {
                Token::StartTag { name, .. } => name.clone(),
                _ => unreachable!(),
            };
            let self_closing =
                matches!(&token, Token::StartTag { self_closing, .. } if *self_closing);
            tokens.push(token);
            pos = next;
            if is_raw_text(&raw_name) && !self_closing {
                let (text, end) = read_raw_text(source, pos, &raw_name);
                if !text.is_empty() {
                    tokens.push(Token::Text(text));
                }
                pos = end;
            }
            continue;
        }

        // A stray '<' is text.
        tokens.push(Token::Text("<".to_owned()));
        pos += 1;
    }

    tokens
}

/// The byte index after any whitespace at `pos`, stepping whole characters
/// (some whitespace, like U+2028, is several bytes).
fn skip_whitespace(source: &str, pos: usize) -> usize {
    let rest = &source[pos..];
    pos + (rest.len() - rest.trim_start().len())
}

fn parse_start_tag(source: &str, mut pos: usize) -> (Token, usize) {
    pos += 1; // '<'
    let name_start = pos;
    while pos < source.len()
        && !source[pos..].chars().next().unwrap().is_whitespace()
        && !matches!(source[pos..].chars().next().unwrap(), '>' | '/')
    {
        pos += source[pos..].chars().next().unwrap().len_utf8();
    }
    let name = source[name_start..pos].to_ascii_lowercase();

    let mut attrs = Vec::new();
    let mut self_closing = false;
    loop {
        pos = skip_whitespace(source, pos);
        if pos >= source.len() {
            break;
        }
        if source[pos..].starts_with("/>") {
            self_closing = true;
            pos += 2;
            break;
        }
        if source[pos..].starts_with('>') {
            pos += 1;
            break;
        }
        if source[pos..].starts_with('/') {
            pos += 1;
            continue;
        }

        let attr_start = pos;
        while pos < source.len()
            && !source[pos..].chars().next().unwrap().is_whitespace()
            && !matches!(source[pos..].chars().next().unwrap(), '=' | '>' | '/')
        {
            pos += source[pos..].chars().next().unwrap().len_utf8();
        }
        let attr_name = source[attr_start..pos].to_ascii_lowercase();
        if attr_name.is_empty() {
            pos += 1;
            continue;
        }

        pos = skip_whitespace(source, pos);

        let value = if source[pos..].starts_with('=') {
            pos += 1;
            pos = skip_whitespace(source, pos);
            match source[pos..].chars().next() {
                Some(q @ ('"' | '\'')) => {
                    pos += 1;
                    let val_start = pos;
                    while pos < source.len() && !source[pos..].starts_with(q) {
                        pos += source[pos..].chars().next().unwrap().len_utf8();
                    }
                    let value = source[val_start..pos].to_owned();
                    if pos < source.len() {
                        pos += 1;
                    }
                    Some(value)
                }
                _ => {
                    let val_start = pos;
                    while pos < source.len()
                        && !source[pos..].chars().next().unwrap().is_whitespace()
                        && !source[pos..].starts_with('>')
                    {
                        pos += source[pos..].chars().next().unwrap().len_utf8();
                    }
                    Some(source[val_start..pos].to_owned())
                }
            }
        } else {
            None
        };

        attrs.push((attr_name, value));
    }

    (
        Token::StartTag {
            name,
            attrs,
            self_closing,
        },
        pos,
    )
}

fn read_raw_text(source: &str, pos: usize, name: &str) -> (String, usize) {
    let close = format!("</{name}");
    let lower = source[pos..].to_ascii_lowercase();
    match lower.find(&close) {
        // Stop *before* the closing tag so the main loop tokenizes it normally
        // (the tree builder needs the end tag to pop the element).
        Some(offset) => (source[pos..pos + offset].to_owned(), pos + offset),
        None => (source[pos..].to_owned(), source.len()),
    }
}

/// Parse HTML into a [`Document`] using a simple stack-based tree builder.
pub fn parse(source: &str) -> Document {
    let mut doc = Document::new();
    let root = doc.root();
    // The skeleton may wrap the content in `html` and `body` afterwards:
    // leave room for them under the depth limit.
    let tokens = tokenize(source);
    doc.set_quirks_mode(mode::of(&tokens));
    build(&mut doc, root, tokens, MAX_TREE_DEPTH - 2);
    skeleton(&mut doc);
    doc
}

/// A `<!...>` declaration's token from its inside: `doctype`, the name, and
/// an optional `PUBLIC "id" "id"` or `SYSTEM "id"`.
fn doctype_token(inner: &str) -> Token {
    let rest = match inner.get(..7) {
        Some(keyword) if keyword.eq_ignore_ascii_case("doctype") => &inner[7..],
        _ => inner,
    };
    let rest = rest.trim_start();
    let name_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let name = rest[..name_end].to_owned();
    let mut rest = rest[name_end..].trim_start();
    let keyword_end = rest
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(rest.len());
    let keyword = rest[..keyword_end].to_ascii_lowercase();
    rest = &rest[keyword_end..];
    // One quoted identifier from the start of `rest`, and what follows it.
    let quoted = |rest: &str| -> Option<(String, usize)> {
        let rest_trimmed = rest.trim_start();
        let skipped = rest.len() - rest_trimmed.len();
        let quote = rest_trimmed
            .chars()
            .next()
            .filter(|c| *c == '"' || *c == '\'')?;
        let body = &rest_trimmed[1..];
        let close = body.find(quote).unwrap_or(body.len());
        let consumed = skipped + 1 + close + usize::from(close < body.len());
        Some((body[..close].to_owned(), consumed))
    };
    let (public_id, system_id) = match keyword.as_str() {
        "public" => match quoted(rest) {
            Some((public, used)) => (Some(public), quoted(&rest[used..]).map(|(id, _)| id)),
            None => (None, None),
        },
        "system" => (None, quoted(rest).map(|(id, _)| id)),
        _ => (None, None),
    };
    Token::Doctype {
        name,
        public_id,
        system_id,
    }
}

/// Elements that belong in `<head>` (the spec's "in head" start tags).
fn is_head_content(name: &str) -> bool {
    matches!(
        name,
        "base"
            | "basefont"
            | "bgsound"
            | "link"
            | "meta"
            | "noscript"
            | "script"
            | "style"
            | "template"
            | "title"
    )
}

fn is_blank_text(doc: &Document, node: NodeId) -> bool {
    matches!(doc.node(node), Some(noonoo_dom::NodeData::Text(text)) if text.trim().is_empty())
}

fn is_comment(doc: &Document, node: NodeId) -> bool {
    matches!(doc.node(node), Some(noonoo_dom::NodeData::Comment(_)))
}

/// The first child of `parent` that is a `name` element.
fn child_element(doc: &Document, parent: NodeId, name: &str) -> Option<NodeId> {
    doc.tree()
        .children(parent)
        .iter()
        .copied()
        .find(|&child| doc.element(child).is_some_and(|e| e.name == name))
}

/// Give a parsed document the `html` > `head` + `body` skeleton every
/// document has, whichever tags the source left out (HTML's "before html",
/// "before head", "in head" and "after body" modes, after the fact).
///
/// The doctype and the comments before any content stay at the root; blank
/// text there is dropped. Leading metadata (`meta`, `style`, `title`, ...)
/// goes in a missing `head`; other content, wherever it sits around the
/// `body`, goes in it. Blank text and comments between them stay put.
fn skeleton(doc: &mut Document) {
    let root = doc.root();
    let html = match child_element(doc, root, "html") {
        Some(html) => html,
        None => {
            let html = doc.create_element("html");
            let _ = doc.mutate().append_child(root, html);
            html
        }
    };
    // Root strays: before the `html` element they go to its start, after
    // it to its end; leading comments and the doctype stay.
    let mut leading = true;
    let mut before = Vec::new();
    let mut after_html = false;
    for child in doc.tree().children(root).to_vec() {
        if child == html {
            after_html = true;
            continue;
        }
        match doc.node(child) {
            Some(noonoo_dom::NodeData::Doctype { .. }) => continue,
            Some(noonoo_dom::NodeData::Comment(_)) if leading && !after_html => continue,
            _ if is_blank_text(doc, child) => {
                let _ = doc.mutate().remove(child);
                continue;
            }
            _ => {}
        }
        leading = false;
        if after_html {
            let _ = doc.mutate().append_child(html, child);
        } else {
            before.push(child);
        }
    }
    let first = doc.tree().children(html).first().copied();
    for child in before {
        let _ = match first {
            Some(first) => doc.mutate().insert_before(first, child),
            None => doc.mutate().append_child(html, child),
        };
    }

    let head = match child_element(doc, html, "head") {
        Some(head) => head,
        None => {
            let head = doc.create_element("head");
            let first = doc.tree().children(html).first().copied();
            let _ = match first {
                Some(first) => doc.mutate().insert_before(first, head),
                None => doc.mutate().append_child(html, head),
            };
            // The leading run of metadata (and the blanks and comments in
            // it) moves in; the first piece of content ends it.
            let mut run = Vec::new();
            for &child in &doc.tree().children(html)[1..] {
                let metadata = doc.element(child).is_some_and(|e| is_head_content(&e.name));
                if metadata || is_blank_text(doc, child) || is_comment(doc, child) {
                    run.push(child);
                } else {
                    break;
                }
            }
            for child in run {
                let _ = doc.mutate().append_child(head, child);
            }
            head
        }
    };
    let body = match child_element(doc, html, "body") {
        Some(body) => body,
        None => {
            let body = doc.create_element("body");
            let _ = doc.mutate().append_child(html, body);
            body
        }
    };
    // Content outside `head` and `body` joins the body, in order.
    let mut before_body = Vec::new();
    let mut seen_body = false;
    for child in doc.tree().children(html).to_vec() {
        if child == body {
            seen_body = true;
            continue;
        }
        if child == head || is_blank_text(doc, child) || is_comment(doc, child) {
            continue;
        }
        if seen_body {
            let _ = doc.mutate().append_child(body, child);
        } else {
            before_body.push(child);
        }
    }
    let first = doc.tree().children(body).first().copied();
    for child in before_body {
        let _ = match first {
            Some(first) => doc.mutate().insert_before(first, child),
            None => doc.mutate().append_child(body, child),
        };
    }
}

/// Parse `html` into `node`, replacing its children: `innerHTML`.
///
/// The fragment is built under `node` with the same tree builder as
/// [`parse`], and can't reach outside it. An end tag for `node` or anything
/// above it closes nothing, and tags left open close at the end of the
/// fragment. On a raw-text element (`script`, `style`, `textarea`, `title`)
/// the markup is the element's text, as the HTML fragment parser does.
///
/// A node that can't hold children is [`MutError::CannotHaveChildren`], and
/// an id not in the document is [`MutError::NoSuchNode`]. Either way the
/// document is unchanged.
pub fn set_inner_html(document: &mut Document, node: NodeId, html: &str) -> Result<(), MutError> {
    if document.element(node).is_some_and(|e| is_raw_text(&e.name)) {
        return document.mutate().set_text(node, html);
    }
    // Checks the target, then empties it: replacing, never appending.
    document.mutate().replace_children(node, &[])?;
    build(document, node, tokenize(html), MAX_TREE_DEPTH);
    Ok(())
}

/// Where [`insert_adjacent_html`] puts the fragment, relative to the node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adjacent {
    /// Before the node, as its preceding siblings.
    BeforeBegin,
    /// Inside the node, before its first child.
    AfterBegin,
    /// Inside the node, after its last child.
    BeforeEnd,
    /// After the node, as its following siblings.
    AfterEnd,
}

/// Parse `html` and insert it at `position` relative to `node`:
/// `insertAdjacentHTML`. Returns the inserted top-level nodes, in order.
///
/// The fragment is parsed in its context: `node` itself for
/// [`Adjacent::AfterBegin`]/[`Adjacent::BeforeEnd`], its parent for the
/// other two. As with [`set_inner_html`], it can't close anything outside
/// that context, and a raw-text context takes the markup as text.
///
/// A sibling position on a node with no parent is [`MutError::NoParent`].
/// An inside position on a node that can't hold children is
/// [`MutError::CannotHaveChildren`]. An id not in the document is
/// [`MutError::NoSuchNode`]. In each case the document is unchanged.
pub fn insert_adjacent_html(
    document: &mut Document,
    node: NodeId,
    position: Adjacent,
    html: &str,
) -> Result<Vec<NodeId>, MutError> {
    let data = document.node(node).ok_or(MutError::NoSuchNode(node))?;
    let can_hold = data.can_have_children();
    let tree = document.tree();
    // The context the fragment parses in, and the node it goes in front of
    // (`None`: the context's end, where the builder appends).
    let (context, anchor) = match position {
        Adjacent::AfterBegin | Adjacent::BeforeEnd if !can_hold => {
            return Err(MutError::CannotHaveChildren(node));
        }
        Adjacent::AfterBegin => (node, tree.children(node).first().copied()),
        Adjacent::BeforeEnd => (node, None),
        Adjacent::BeforeBegin | Adjacent::AfterEnd => {
            let parent = tree.parent(node).ok_or(MutError::NoParent(node))?;
            let siblings = tree.children(parent);
            let anchor = match position {
                Adjacent::BeforeBegin => Some(node),
                _ => siblings
                    .iter()
                    .position(|&id| id == node)
                    .and_then(|index| siblings.get(index + 1).copied()),
            };
            (parent, anchor)
        }
    };

    let start = document.tree().children(context).len();
    if document
        .element(context)
        .is_some_and(|e| is_raw_text(&e.name))
    {
        let text = document.create_text(html);
        document.mutate().append_child(context, text)?;
    } else {
        build(document, context, tokenize(html), MAX_TREE_DEPTH);
    }
    let inserted = document.tree().children(context)[start..].to_vec();
    if let Some(anchor) = anchor {
        for &child in &inserted {
            document.mutate().insert_before(anchor, child)?;
        }
    }
    Ok(inserted)
}

/// The deepest the parser nests elements, as in Chromium. Past it a node is
/// attached to the current node's parent instead, so deeper nesting becomes
/// siblings: markup can't drive recursive style, layout or paint deeper.
pub const MAX_TREE_DEPTH: usize = 512;

/// Build `tokens` as children of `base`, the bottom of the open-element
/// stack, which an end tag or an implied close never pops.
fn build(doc: &mut Document, base: NodeId, tokens: Vec<Token>, depth: usize) {
    let mut stack = vec![base];
    // HTML drops a newline straight after these start tags.
    let mut drop_newline = false;

    for token in tokens {
        let after_preformatted = std::mem::take(&mut drop_newline);
        match token {
            Token::Doctype { name, .. } => {
                let node = doc
                    .tree_mut()
                    .insert_raw(noonoo_dom::NodeData::Doctype { name });
                let parent = insertion_parent(doc, &stack, depth);
                let _ = doc.mutate().append_child(parent, node);
            }
            Token::Comment(text) => {
                let node = doc.create_comment(text);
                let parent = insertion_parent(doc, &stack, depth);
                let _ = doc.mutate().append_child(parent, node);
            }
            Token::Text(mut text) => {
                if after_preformatted {
                    if text.starts_with("\r\n") {
                        text.drain(..2);
                    } else if text.starts_with(['\n', '\r']) {
                        text.drain(..1);
                    }
                    if text.is_empty() {
                        continue;
                    }
                }
                if !text.trim().is_empty() {
                    close_head(doc, &mut stack);
                }
                let parent = insertion_parent(doc, &stack, depth);
                let node = doc.create_text(text);
                let _ = doc.mutate().append_child(parent, node);
            }
            Token::StartTag {
                name,
                attrs,
                self_closing,
            } => {
                // Inside a table, the spec's "in cell"/"in row" insertion modes
                // close elements implicitly: a new `td` closes the open cell, a
                // new row or section closes the open cell *and* row (the implied
                // `<tbody>`/`<td>` closing). The stack builder normally relies on
                // explicit end tags; these rules keep a subset of HTML5's
                // table handling without the full insertion-mode state machine.
                if !is_head_content(&name) && name != "head" && name != "html" {
                    close_head(doc, &mut stack);
                }
                implied_table_close(&mut stack, doc, &name);
                implied::Open {
                    stack: &mut stack,
                    doc,
                }
                .before_start_tag(&name);
                // A row or cell outside its section opens the section first.
                while let Some(section) = (implied::Open {
                    stack: &mut stack,
                    doc,
                })
                .implied_section(&name)
                {
                    let parent = insertion_parent(doc, &stack, depth);
                    let node = doc.create_element(section);
                    let _ = doc.mutate().append_child(parent, node);
                    stack.push(node);
                }

                let parent = insertion_parent(doc, &stack, depth);
                let node = doc.create_element(name.clone());
                for (attr, value) in attrs {
                    let _ = doc.mutate().set_attribute(
                        node,
                        &attr,
                        value
                            .map(|value| entities::decode(&value))
                            .unwrap_or_default(),
                    );
                }
                let _ = doc.mutate().append_child(parent, node);
                if !self_closing && !is_void(&name) {
                    stack.push(node);
                    drop_newline = matches!(name.as_str(), "pre" | "listing" | "textarea");
                }
            }
            Token::EndTag { name } if name == "p" => {
                let closed = implied::Open {
                    stack: &mut stack,
                    doc,
                }
                .end_p();
                if !closed {
                    let parent = insertion_parent(doc, &stack, depth);
                    let node = doc.create_element("p");
                    let _ = doc.mutate().append_child(parent, node);
                }
            }
            Token::EndTag { name } => {
                // Pop to the matching open element above `base`, if any.
                if let Some(index) = stack[1..]
                    .iter()
                    .rposition(|&id| doc.element(id).is_some_and(|e| e.name == name))
                {
                    stack.truncate(index + 1);
                }
            }
        }
    }
}

/// Content can't sit in `<head>`: when the head is the current node (not
/// the base), it closes. Inside a `<title>` or `<noscript>` it stays open.
fn close_head(doc: &Document, stack: &mut Vec<NodeId>) {
    let current = stack[stack.len() - 1];
    if stack.len() > 1 && doc.element(current).is_some_and(|e| e.name == "head") {
        stack.pop();
    }
}

/// Where the next node goes: the current node, or past `depth` open
/// elements (at most [`MAX_TREE_DEPTH`]), the current node's parent. `stack[0]` is the base, which
/// is never popped, so the stack is never empty.
fn insertion_parent(doc: &Document, stack: &[NodeId], depth: usize) -> NodeId {
    let current = stack[stack.len() - 1];
    if stack.len() > depth {
        doc.tree().parent(current).unwrap_or(current)
    } else {
        current
    }
}

/// Convert plain text to HTML: one `<pre>` block with escaped content.
///
/// The `.txt` viewer — text has no structure, so the whole file is a single
/// preformatted block.
pub fn plain_text_to_html(source: &str) -> String {
    let mut out = String::from("<pre>");
    for ch in source.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out.push_str("</pre>");
    out
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

mod entities;
pub mod serialize;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_tags_text_and_comments() {
        let tokens = tokenize("<p class=\"a\">hi</p><!-- c -->");
        assert_eq!(
            tokens,
            vec![
                Token::StartTag {
                    name: "p".into(),
                    attrs: vec![("class".into(), Some("a".into()))],
                    self_closing: false,
                },
                Token::Text("hi".into()),
                Token::EndTag { name: "p".into() },
                Token::Comment(" c ".into()),
            ]
        );
    }

    #[test]
    fn raw_text_is_not_tokenized_as_markup() {
        let tokens = tokenize("<script>let a = 1 < 2;</script>");
        assert!(matches!(&tokens[0], Token::StartTag { name, .. } if name == "script"));
        assert!(matches!(&tokens[1], Token::Text(t) if t.contains("a = 1 < 2")));
        assert!(matches!(&tokens[2], Token::EndTag { name } if name == "script"));
    }

    #[test]
    fn builds_a_tree() {
        let doc = parse("<div id=\"box\"><span>hi</span>tail</div>");
        let box_id = doc.get_element_by_id("box").unwrap();
        assert_eq!(doc.element(box_id).unwrap().name, "div");
        let children = doc.tree().children(box_id);
        assert_eq!(children.len(), 2);
        assert_eq!(doc.element(children[0]).unwrap().name, "span");
        assert_eq!(doc.node(children[1]).unwrap().as_text(), Some("tail"));
    }

    #[test]
    fn void_elements_have_no_children() {
        let doc = parse("<div><br><img src=\"x\">after</div>");
        let body = doc.query_selector("div").unwrap().unwrap();
        // br and img are siblings of the text, all under div.
        assert_eq!(doc.tree().children(body).len(), 3);
    }

    #[test]
    fn plain_text_is_escaped_and_preformatted() {
        assert_eq!(plain_text_to_html("a < b\nc"), "<pre>a &lt; b\nc</pre>");
    }

    #[test]
    fn doctype_and_attributes() {
        let doc = parse("<!doctype html><a href=\"a\" download>x</a>");
        let a = doc.query_selector("a").unwrap().unwrap();
        let element = doc.element(a).unwrap();
        assert_eq!(element.attr("href"), Some("a"));
        assert_eq!(element.attr("download"), Some(""));
    }

    #[test]
    fn unclosed_table_cells_and_rows_close_implied() {
        // Finding 3's tree-builder half: `<td>a<td>b</tr>` must produce two
        // (2) sibling cells, not a cell nested in a cell.
        let doc = parse("<table><tr><td>a<td>b</tr><tr><td>c</table>");
        let trs = doc.get_elements_by_tag_name("tr");
        assert_eq!(trs.len(), 2, "two rows: {trs:?}");

        let cells = |tr| -> Vec<_> {
            doc.tree()
                .children(tr)
                .iter()
                .filter_map(|&child| doc.element(child))
                .map(|element| element.name.as_str())
                .collect()
        };
        assert_eq!(cells(trs[0]), ["td", "td"]);
        assert_eq!(cells(trs[1]), ["td"]);

        // No `td` nests inside another.
        let tds = doc.get_elements_by_tag_name("td");
        assert_eq!(tds.len(), 3);
        for td in &tds {
            assert!(
                !doc.tree()
                    .children(*td)
                    .iter()
                    .any(|child| doc.element(*child).is_some_and(|e| e.name == "td")),
                "no nested cell"
            );
        }
        // The cell's attribute rides along.
        let doc = parse("<table><tr><td colspan=2>a</table>");
        let td = doc.get_elements_by_tag_name("td")[0];
        assert_eq!(doc.element(td).unwrap().attr("colspan"), Some("2"));
    }
}
