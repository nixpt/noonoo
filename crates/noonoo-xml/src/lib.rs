//! # noonoo-xml
//!
//! A dependency-free XML parser producing a [`noonoo_dom::Document`] — the same
//! tree `noonoo-html` builds, so XML flows through the whole render pipeline
//! (`docs/FORMATS.md`: renderable formats parse to the DOM).
//!
//! Supported: the XML prolog, doctype, elements with attributes (single or
//! double quoted), self-closing tags, text with the standard entities and
//! numeric character references, comments, and CDATA sections. Unlike HTML,
//! XML nesting is **strict** — a mismatched or unclosed tag is an error.
//!
//! Namespace *resolution* is not implemented, and the shared DOM normalizes
//! element names, and attribute names outside `<svg>` and `<math>`, to ASCII
//! lowercase (an HTML-shaped choice), so those XML names are case-folded on
//! ingest. An `<svg>` or `<math>` and what it holds are SVG or MathML
//! elements, whose attribute names keep their case. Also not yet: DTD entities and
//! processing-instruction dispatch.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use noonoo_dom::{Document, Namespace, NodeId};
use noonoo_parse::{ParseError, ParseResult};

/// Parse an XML document into a [`Document`].
pub fn parse(source: &str) -> ParseResult<Document> {
    let mut document = Document::new();
    let mut stack: Vec<NodeId> = vec![document.root()];
    let mut pos = 0;

    while pos < source.len() {
        let rest = &source[pos..];

        if let Some(stripped) = rest.strip_prefix("<!--") {
            let end = stripped
                .find("-->")
                .ok_or_else(|| ParseError::new(pos, "unterminated comment"))?;
            let node = document.create_comment(stripped[..end].to_owned());
            append(&mut document, *stack.last().unwrap(), node);
            pos += 4 + end + 3;
        } else if let Some(stripped) = rest.strip_prefix("<![CDATA[") {
            let end = stripped
                .find("]]>")
                .ok_or_else(|| ParseError::new(pos, "unterminated CDATA"))?;
            let node = document.create_text(stripped[..end].to_owned());
            append(&mut document, *stack.last().unwrap(), node);
            pos += 9 + end + 3;
        } else if rest.starts_with("<?") {
            let end = rest
                .find("?>")
                .ok_or_else(|| ParseError::new(pos, "unterminated processing instruction"))?;
            pos += end + 2;
        } else if rest.starts_with("<!") {
            let end = rest
                .find('>')
                .ok_or_else(|| ParseError::new(pos, "unterminated declaration"))?;
            pos += end + 1;
        } else if let Some(stripped) = rest.strip_prefix("</") {
            let end = stripped
                .find('>')
                .ok_or_else(|| ParseError::new(pos, "unterminated end tag"))?;
            let name = stripped[..end].trim();
            let parent = stack
                .pop()
                .ok_or_else(|| ParseError::new(pos, "stray end tag"))?;
            let matches = document
                .element(parent)
                .is_some_and(|element| element.name == name);
            if !matches {
                return Err(ParseError::new(
                    pos,
                    format!("unexpected closing tag `</{name}>`"),
                ));
            }
            if stack.is_empty() {
                return Err(ParseError::new(pos, "end tag with no open element"));
            }
            pos += 3 + end; // "</" + name + ">"
        } else if rest.starts_with('<') {
            let parent = *stack.last().unwrap();
            let (element, length, self_closing) =
                parse_start_tag(&mut document, parent, rest, pos)?;
            append(&mut document, parent, element);
            if !self_closing {
                stack.push(element);
            }
            pos += length;
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            let text = decode_entities(&rest[..end]);
            if !text.is_empty() {
                let node = document.create_text(text);
                append(&mut document, *stack.last().unwrap(), node);
            }
            pos += end;
        }
    }

    if stack.len() != 1 {
        return Err(ParseError::new(
            source.len(),
            "unclosed element at end of input",
        ));
    }
    Ok(document)
}

fn append(document: &mut Document, parent: NodeId, node: NodeId) {
    let _ = document.mutate().append_child(parent, node);
}

/// Parse `<name attr="v" …>` or `<name …/>`; returns `(node, bytes consumed, self_closing)`.
fn parse_start_tag(
    document: &mut Document,
    parent: NodeId,
    rest: &str,
    offset: usize,
) -> ParseResult<(NodeId, usize, bool)> {
    let bytes = rest.as_bytes();
    let mut i = 1; // skip '<'

    let name_start = i;
    while i < rest.len() && !is_name_end(bytes[i]) {
        i += 1;
    }
    let name = &rest[name_start..i];
    if name.is_empty() {
        return Err(ParseError::new(offset, "expected a tag name"));
    }
    // `<svg>` and `<math>` start their namespace and what they hold
    // inherits it, so their attribute names keep their case.
    let namespace = if name.eq_ignore_ascii_case("svg") {
        Namespace::Svg
    } else if name.eq_ignore_ascii_case("math") {
        Namespace::MathMl
    } else {
        document
            .element(parent)
            .map_or(Namespace::Html, |parent| parent.namespace)
    };
    let element = document.create_element_ns(name, namespace);
    let mut self_closing = false;

    loop {
        while i < rest.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= rest.len() {
            return Err(ParseError::new(offset, "unterminated start tag"));
        }
        if bytes[i] == b'/' {
            self_closing = true;
            i += 1;
            while i < rest.len() && bytes[i] != b'>' {
                i += 1;
            }
            if i >= rest.len() {
                return Err(ParseError::new(offset, "unterminated start tag"));
            }
            i += 1; // consume '>'
            break;
        }
        if bytes[i] == b'>' {
            i += 1;
            break;
        }

        // attribute name
        let attr_start = i;
        while i < rest.len()
            && bytes[i] != b'='
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'>'
            && bytes[i] != b'/'
        {
            i += 1;
        }
        let attr_name = rest[attr_start..i].to_owned();
        while i < rest.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < rest.len() && bytes[i] == b'=' {
            i += 1;
            while i < rest.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let quote = bytes.get(i).copied();
            if quote != Some(b'"') && quote != Some(b'\'') {
                return Err(ParseError::new(offset, "attribute value must be quoted"));
            }
            let q = quote.unwrap();
            i += 1;
            let value_start = i;
            while i < rest.len() && bytes[i] != q {
                i += 1;
            }
            let value = decode_entities(&rest[value_start..i]);
            i += 1; // closing quote
            let _ = document.mutate().set_attribute(element, &attr_name, value);
        } else if !attr_name.is_empty() {
            let _ = document.mutate().set_attribute(element, &attr_name, "");
        }
    }

    Ok((element, i, self_closing))
}

fn is_name_end(byte: u8) -> bool {
    byte.is_ascii_whitespace() || byte == b'>' || byte == b'/'
}

/// Decode the five predefined entities plus numeric character references.
fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        if let Some(semi) = after.find(';').filter(|&s| s <= 8) {
            let entity = &after[..semi];
            if let Some(ch) = decode_entity(entity) {
                out.push(ch);
                rest = &after[semi + 1..];
                continue;
            }
        }
        out.push('&');
        rest = after;
    }
    out.push_str(rest);
    out
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let rest = entity.strip_prefix('#')?;
            let code = match rest.strip_prefix('x').or_else(|| rest.strip_prefix('X')) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => rest.parse::<u32>().ok()?,
            };
            char::from_u32(code)
        }
    }
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    /// Without namespace resolution, an `<svg>` and what it holds are SVG:
    /// their attribute names keep their case, as XML's always would.
    #[test]
    fn svg_keeps_attribute_case() {
        let doc =
            parse(r#"<doc Lang="x"><svg viewBox="0 0 1 1"><g gradientTransform="t"/></svg></doc>"#)
                .unwrap();
        let element = |name: &str| {
            doc.element(doc.query_selector(name).unwrap().unwrap())
                .unwrap()
        };
        assert_eq!(element("doc").attr("lang"), Some("x"));
        assert_eq!(element("svg").namespace, noonoo_dom::Namespace::Svg);
        assert_eq!(element("svg").attr("viewBox"), Some("0 0 1 1"));
        assert_eq!(element("g").attr("gradientTransform"), Some("t"));
    }

    #[test]
    fn parses_elements_and_attributes() {
        let doc = parse(r#"<a href="x.txt"><b>hi</b></a>"#).unwrap();
        let root = doc.query_selector("a").unwrap().unwrap();
        assert_eq!(doc.element(root).unwrap().attr("href"), Some("x.txt"));
        let b = doc.query_selector("b").unwrap().unwrap();
        assert_eq!(doc.text_content(b), "hi");
    }

    #[test]
    fn self_closing_and_entities() {
        let doc = parse(r#"<r><br/><t>a &amp; b &#65; &#x42;</t></r>"#).unwrap();
        let t = doc.query_selector("t").unwrap().unwrap();
        assert_eq!(doc.text_content(t), "a & b A B");
    }

    #[test]
    fn cdata_and_comments() {
        let doc = parse("<r><!-- c --><d><![CDATA[a<b]]></d></r>").unwrap();
        let d = doc.query_selector("d").unwrap().unwrap();
        assert_eq!(doc.text_content(d), "a<b");
        assert!(doc.query_selector("d").unwrap().is_some());
    }

    #[test]
    fn prolog_and_doctype_are_skipped() {
        let doc = parse(r#"<?xml version="1.0"?><!DOCTYPE r><r>x</r>"#).unwrap();
        let r = doc.query_selector("r").unwrap().unwrap();
        assert_eq!(doc.text_content(r), "x");
    }

    #[test]
    fn mismatched_tags_error() {
        assert!(parse("<a><b></a></b>").is_err());
        assert!(parse("<a>").is_err());
    }
}
