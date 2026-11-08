//! A parsed document always has the `html` > `head` + `body` skeleton,
//! whichever tags the source leaves out, as HTML's "before html", "before
//! head", "in head" and "after body" insertion modes give it.

use noonoo_dom::{Document, NodeData, NodeId};

/// The whole tree from the root as compact markup: elements as
/// `name[children]`, non-blank text as `"..."`, `!doctype`, `#comment`.
fn shape(html: &str) -> String {
    let doc = noonoo_html::parse(html);
    children(&doc, doc.root())
}

fn children(doc: &Document, node: NodeId) -> String {
    doc.tree()
        .children(node)
        .iter()
        .filter_map(|&child| match doc.node(child)? {
            NodeData::Element(element) => {
                let inner = children(doc, child);
                Some(if inner.is_empty() {
                    element.name.clone()
                } else {
                    format!("{}[{inner}]", element.name)
                })
            }
            NodeData::Text(text) if !text.trim().is_empty() => Some(format!("{:?}", text.trim())),
            NodeData::Doctype { .. } => Some("!doctype".to_owned()),
            NodeData::Comment(_) => Some("#comment".to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_bare_fragment_gets_html_head_and_body() {
    assert_eq!(shape("<p>x"), r#"html[head body[p["x"]]]"#);
    assert_eq!(shape("just text"), r#"html[head body["just text"]]"#);
    assert_eq!(shape(""), "html[head body]");
}

#[test]
fn leading_metadata_goes_in_the_head() {
    // danluu.com's shape: no html, head or body tags at all.
    assert_eq!(
        shape(
            r#"<style>a{}</style><meta name=viewport><title>t</title><ul><li>a</ul><script>x</script>"#
        ),
        r#"html[head[style["a{}"] meta title["t"]] body[ul[li["a"]] script["x"]]]"#
    );
}

#[test]
fn the_doctype_and_leading_comments_stay_at_the_root() {
    assert_eq!(
        shape("<!doctype html><!-- c --><p>x"),
        r#"!doctype #comment html[head body[p["x"]]]"#
    );
}

#[test]
fn a_complete_document_is_unchanged() {
    assert_eq!(
        shape("<!DOCTYPE html><html><head><title>t</title></head>\n<body><p>x</p></body></html>"),
        r#"!doctype html[head[title["t"]] body[p["x"]]]"#
    );
}

#[test]
fn a_missing_head_or_body_is_filled_in() {
    assert_eq!(shape("<html><body>x"), r#"html[head body["x"]]"#);
    assert_eq!(
        shape("<html><head><title>t</title></head><p>x"),
        r#"html[head[title["t"]] body[p["x"]]]"#
    );
}

#[test]
fn content_closes_an_unclosed_head() {
    assert_eq!(
        shape("<head><meta charset=utf-8><body>x"),
        r#"html[head[meta] body["x"]]"#
    );
    assert_eq!(
        shape("<head><title>t</title><div>x</div>"),
        r#"html[head[title["t"]] body[div["x"]]]"#
    );
}

#[test]
fn content_after_the_body_joins_it() {
    assert_eq!(
        shape("<html><body><p>a</p></body></html><p>b</p>"),
        r#"html[head body[p["a"] p["b"]]]"#
    );
}

#[test]
fn fragments_parsed_into_an_element_get_no_skeleton() {
    let mut doc = noonoo_html::parse("<div id=d></div>");
    let div = doc.get_element_by_id("d").unwrap();
    noonoo_html::set_inner_html(&mut doc, div, "<p>x").unwrap();
    assert_eq!(
        children(&doc, doc.root()),
        r#"html[head body[div[p["x"]]]]"#
    );
}
