//! HTML serialization.
//!
//! The read direction the consumer caps need (`dom.inner_html` — see
//! `docs/REVIEW-2026-11-07-consumers.md`): round-trip a parsed document back
//! to HTML. Serialization is the inverse of parsing, so it lives beside the
//! parser: it knows the void and raw-text element lists that shape the
//! output.
//!
//! Escaping follows the rules the data needs, no more:
//!
//! - text nodes escape `&`, `<`, `>`;
//! - text directly in a raw-text element (`script`, `style`, `xmp`, ...) is
//!   emitted verbatim, by `inner_html` too: it is data, and character
//!   references in it are not decoded. Elements in one (`plaintext` may get
//!   some) are written as anywhere;
//! - attribute values are quoted, escaping `&` and `"`; the empty value
//!   serializes as `name=""` (a bare attribute's value is stored empty, and
//!   this form round-trips through the tokenizer);
//! - comments emit `<!--text-->`; a doctype is dropped (its name is data for
//!   presentation, not structure), and a document node serializes as its
//!   children — which is what `inner_html` of the root is.
//!
//! `inner_html` serializes the children of `node`; `outer_html` includes the
//! node itself. Neither sanitizes: a fragment that parses also serializes
//! back with the same parse.

use crate::VOID_ELEMENTS;
use noonoo_dom::{Document, Namespace, NodeData, NodeId};

/// The HTML of `node`'s children.
pub fn inner_html(document: &Document, node: NodeId) -> String {
    let mut out = String::new();
    write_children(document, node, &mut out);
    out
}

/// The HTML of `node` itself, from its start tag to its end tag.
pub fn outer_html(document: &Document, node: NodeId) -> String {
    let mut out = String::new();
    write_outer(document, node, &mut out);
    out
}

fn write_children(document: &Document, node: NodeId, out: &mut String) {
    let raw = document
        .element(node)
        .is_some_and(|element| element.namespace == Namespace::Html && is_raw_text(&element.name));
    for child in document.tree().children(node) {
        match document.node(*child) {
            // A raw-text element's own text is data, verbatim.
            Some(NodeData::Text(text)) if raw => out.push_str(text),
            _ => write_outer(document, *child, out),
        }
    }
}

fn write_outer(document: &Document, node: NodeId, out: &mut String) {
    match document.node(node) {
        Some(NodeData::Element(element)) => {
            out.push('<');
            out.push_str(&element.name);
            for attribute in &element.attrs {
                out.push(' ');
                out.push_str(&attribute.name);
                out.push_str("=\"");
                escape_text(&attribute.value, ESCAPE_ATTRIBUTE, out);
                out.push('"');
            }
            out.push('>');
            let html = element.namespace == Namespace::Html;
            if html && is_void(&element.name) {
                // Void elements carry no content and take no end tag.
                return;
            }
            write_children(document, node, out);
            out.push_str("</");
            out.push_str(&element.name);
            out.push('>');
        }
        Some(NodeData::Text(text)) => escape_text(text, ESCAPE_TEXT, out),
        Some(NodeData::Comment(text)) => {
            out.push_str("<!--");
            out.push_str(text);
            out.push_str("-->");
        }
        // The document node is its children; a doctype is not content.
        Some(NodeData::Document) => write_children(document, node, out),
        Some(NodeData::Doctype { .. }) => {}
        None => {} // The id is not in this document.
    }
}

/// The void elements, and the obsolete ones that serialize like them.
fn is_void(name: &str) -> bool {
    VOID_ELEMENTS.contains(&name) || matches!(name, "basefont" | "bgsound" | "frame" | "keygen")
}

/// The elements whose text is written as it is.
fn is_raw_text(name: &str) -> bool {
    matches!(
        name,
        "iframe" | "noembed" | "noframes" | "plaintext" | "script" | "style" | "xmp"
    )
}

/// Contexts to escape, as bit flags.
const ESCAPE_TEXT: u8 = 0b01;
const ESCAPE_ATTRIBUTE: u8 = 0b10;

/// Escape `text` for the contexts in `what`.
fn escape_text(text: &str, what: u8, out: &mut String) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' if what & ESCAPE_TEXT != 0 => out.push_str("&gt;"),
            '"' if what & ESCAPE_ATTRIBUTE != 0 => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
}
