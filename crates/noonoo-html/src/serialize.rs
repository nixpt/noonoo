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
//! - raw-text children (`script`, `style`, `textarea`, `title`) emit
//!   verbatim — their content is data;
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

use crate::{is_raw_text, is_void};
use noonoo_dom::{Document, NodeData, NodeId};

/// The HTML of `node`'s children.
pub fn inner_html(document: &Document, node: NodeId) -> String {
    let mut out = String::new();
    for child in document.tree().children(node) {
        write_outer(document, *child, &mut out);
    }
    out
}

/// The HTML of `node` itself, from its start tag to its end tag.
pub fn outer_html(document: &Document, node: NodeId) -> String {
    let mut out = String::new();
    write_outer(document, node, &mut out);
    out
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
            if is_void(&element.name) {
                // Void elements carry no content and take no end tag.
                return;
            }
            if is_raw_text(&element.name) {
                // Its content is data, verbatim.
                out.push_str(&document.text_content(node));
            } else {
                for child in document.tree().children(node) {
                    write_outer(document, *child, out);
                }
            }
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
        Some(NodeData::Document) => {
            for child in document.tree().children(node) {
                write_outer(document, *child, out);
            }
        }
        Some(NodeData::Doctype { .. }) => {}
        None => {} // The id is not in this document.
    }
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
