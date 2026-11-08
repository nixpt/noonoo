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
//! matching end tags, but it does **not** yet implement HTML5's full insertion
//! modes (foster parenting, the "in body"/"in table" state machines, active
//! formatting elements). That is deliberate scope, not an oversight; see
//! `docs/ROADMAP.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use noonoo_dom::{Document, MutError, NodeId};

/// A single HTML token.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Token {
    /// `<!doctype html>`.
    Doctype {
        /// The doctype name (`"html"` for the common case).
        name: String,
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
            let name = inner
                .strip_prefix("doctype")
                .or_else(|| inner.strip_prefix("DOCTYPE"))
                .unwrap_or(inner)
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_owned();
            tokens.push(Token::Doctype { name });
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
        while pos < source.len() && source[pos..].chars().next().unwrap().is_whitespace() {
            pos += 1;
        }
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

        while pos < source.len() && source[pos..].chars().next().unwrap().is_whitespace() {
            pos += 1;
        }

        let value = if source[pos..].starts_with('=') {
            pos += 1;
            while pos < source.len() && source[pos..].chars().next().unwrap().is_whitespace() {
                pos += 1;
            }
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
    build(&mut doc, root, tokenize(source));
    doc
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
    build(document, node, tokenize(html));
    Ok(())
}

/// Build `tokens` as children of `base`, the bottom of the open-element
/// stack, which an end tag or an implied close never pops.
fn build(doc: &mut Document, base: NodeId, tokens: Vec<Token>) {
    let mut stack = vec![base];

    for token in tokens {
        match token {
            Token::Doctype { name } => {
                let node = doc
                    .tree_mut()
                    .insert_raw(noonoo_dom::NodeData::Doctype { name });
                let parent = *stack.last().unwrap();
                let _ = doc.mutate().append_child(parent, node);
            }
            Token::Comment(text) => {
                let node = doc.create_comment(text);
                let parent = *stack.last().unwrap();
                let _ = doc.mutate().append_child(parent, node);
            }
            Token::Text(text) => {
                let parent = *stack.last().unwrap();
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
                implied_table_close(&mut stack, doc, &name);

                let parent = *stack.last().unwrap();
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
