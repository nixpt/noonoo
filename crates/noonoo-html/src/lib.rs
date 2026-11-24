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
//! The tokenizer and the tree builder follow the HTML standard (§13.2): the
//! insertion modes, the adoption agency algorithm for misnested formatting,
//! foster parenting out of tables, templates and foreign content. Any
//! source gives a document, and [`parse`] always gives it its `html` >
//! `head` + `body` skeleton, filling in whichever of those tags the source
//! leaves out; fragments ([`set_inner_html`], [`insert_adjacent_html`]) get
//! none.
//!
//! It parses as with scripting disabled, so a `<noscript>` holds markup. A
//! `<template>`'s contents are its children (the DOM has no separate
//! fragment for them). SVG element names stay lowercase (`clippath`), as
//! [`noonoo_dom::ElementData`] keeps every element name. Not implemented:
//! declarative shadow roots, and processing instructions, which become
//! comments.
//!
//! Past [`MAX_TREE_DEPTH`] nesting becomes siblings, and there are limits
//! on how far misnested markup is searched and copied; see `builder`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod builder;
mod foreign;
mod mode;
mod named;
mod tags;
mod tokenizer;

use builder::Context;
use noonoo_dom::{Document, MutError, Namespace, NodeId};
use tokenizer::{Content, Tokenizer};

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

/// What the content of `node` is: text, for an HTML element like `script`.
fn content_of(doc: &Document, node: NodeId) -> Content {
    match doc.element(node) {
        Some(element) if element.namespace == Namespace::Html => Content::of(&element.name),
        _ => Content::Data,
    }
}

/// Tokenize an HTML source.
///
/// Text is decoded; attribute values keep their character references. As
/// there is no tree here, every tag is taken for an HTML element's.
pub fn tokenize(source: &str) -> Vec<Token> {
    let source = tokenizer::normalize_newlines(source);
    let mut tokenizer = Tokenizer::new(&source);
    let mut tokens = Vec::new();
    while let Some(token) = tokenizer.next(false) {
        if let Token::StartTag { name, .. } = &token {
            tokenizer.switch_to(Content::of(name));
        }
        tokens.push(token);
    }
    tokens
}

/// Parse HTML into a [`Document`].
///
/// The same source always gives the same tree with the same [`NodeId`]s.
pub fn parse(source: &str) -> Document {
    let mut doc = Document::new();
    builder::document(&mut doc, &tokenizer::normalize_newlines(source));
    doc
}

/// A doctype's token from what follows its keyword: the name, and an
/// optional `PUBLIC "id" "id"` or `SYSTEM "id"`.
fn doctype_token(rest: &str) -> Token {
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

/// Parse `html` into `node`, replacing its children: `innerHTML`.
///
/// The fragment is parsed in the context of `node` and built under it, and
/// can't reach outside it. An end tag for `node` or anything above it closes
/// nothing, and tags left open close at the end of the fragment. What the
/// context doesn't allow is dropped as in HTML (a `<td>` straight in a
/// `<div>`, an `<html>` anywhere). On an element whose content is text
/// (`script`, `style`, `textarea`, `title`, ...) the markup is that text.
///
/// On the document node, which is no context, the markup parses as in a
/// `<template>`: rows and cells, say, stay without a table.
///
/// A node that can't hold children is [`MutError::CannotHaveChildren`], and
/// an id not in the document is [`MutError::NoSuchNode`]. Either way the
/// document is unchanged.
pub fn set_inner_html(document: &mut Document, node: NodeId, html: &str) -> Result<(), MutError> {
    let html = tokenizer::normalize_newlines(html);
    match content_of(document, node) {
        Content::Data => {
            // Checks the target, then empties it: replacing, never appending.
            document.mutate().replace_children(node, &[])?;
            builder::fragment(document, node, Context::Node, &html);
            Ok(())
        }
        content => document.mutate().set_text(node, content.text(&html)),
    }
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
/// that context, and a context whose content is text takes the markup as
/// text. Under an `<html>` the markup parses as in `<body>`.
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
    let html = tokenizer::normalize_newlines(html);
    match content_of(document, context) {
        Content::Data => {
            let within = match document.element(context) {
                Some(element) if element.name == "html" => Context::Body,
                _ => Context::Node,
            };
            builder::fragment(document, context, within, &html);
        }
        content => {
            let text = document.create_text(content.text(&html));
            document.mutate().append_child(context, text)?;
        }
    }
    let inserted = document.tree().children(context)[start..].to_vec();
    if let Some(anchor) = anchor {
        for &child in &inserted {
            document.mutate().insert_before(anchor, child)?;
        }
    }
    Ok(inserted)
}

/// The deepest the parser nests nodes, as in Chromium. Past it a node is
/// attached to the current node's parent instead, so deeper nesting becomes
/// siblings: markup can't drive recursive style, layout or paint deeper.
pub const MAX_TREE_DEPTH: usize = 512;

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
