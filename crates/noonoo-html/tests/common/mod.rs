//! Trees as compact text, for the tests to spell out.

use noonoo_dom::{Document, Namespace, NodeData, NodeId};

/// The tree below `node`: text quoted, an element as
/// `name(attr=value ...)[children]`, SVG and MathML ones prefixed.
pub fn tree(doc: &Document, node: NodeId) -> String {
    let children = doc.tree().children(node).iter();
    children
        .filter_map(|&child| match doc.node(child)? {
            NodeData::Text(text) => Some(format!("{text:?}")),
            NodeData::Comment(text) => Some(format!("<!--{text}-->")),
            NodeData::Element(element) => {
                let mut out = match element.namespace {
                    Namespace::Html => element.name.clone(),
                    Namespace::Svg => format!("svg:{}", element.name),
                    Namespace::MathMl => format!("math:{}", element.name),
                };
                if !element.attrs.is_empty() {
                    let attrs: Vec<_> = element
                        .attrs
                        .iter()
                        .map(|attr| format!("{}={}", attr.name, attr.value))
                        .collect();
                    out += &format!("({})", attrs.join(" "));
                }
                let inner = tree(doc, child);
                if !inner.is_empty() {
                    out += &format!("[{inner}]");
                }
                Some(out)
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse each `(body source, tree of the body)` as a no-quirks document and
/// compare.
#[track_caller]
pub fn check(cases: &[(&str, &str)]) {
    for (source, expected) in cases {
        let doc = noonoo_html::parse(&format!("<!doctype html>{source}"));
        let body = doc.get_elements_by_tag_name("body")[0];
        assert_eq!(tree(&doc, body), *expected, "parsing {source:?}");
    }
}
