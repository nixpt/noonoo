//! `set_inner_html`: parse HTML into an existing node, replacing its children.

use noonoo_dom::MutError;
use noonoo_html::serialize::{inner_html, outer_html};
use noonoo_html::{parse, set_inner_html};

/// The `<body>` every parsed document has.
fn body(doc: &noonoo_dom::Document) -> noonoo_dom::NodeId {
    doc.get_elements_by_tag_name("body")[0]
}

#[test]
fn it_replaces_the_children_rather_than_appending() {
    let mut doc = parse(r#"<div id="t"><p>old</p></div>"#);
    let t = doc.get_element_by_id("t").unwrap();
    set_inner_html(&mut doc, t, r#"<b class="x">new</b> &amp; more"#).unwrap();
    assert_eq!(inner_html(&doc, t), r#"<b class="x">new</b> &amp; more"#);
    set_inner_html(&mut doc, t, "").unwrap();
    assert!(doc.tree().children(t).is_empty());
}

#[test]
fn the_fragment_cannot_escape_its_target() {
    // A stray end tag for the target, or anything above it, closes nothing
    // outside; unclosed tags close at the end of the fragment.
    let mut doc = parse(r#"<main><div id="t"></div><p id="after">after</p></main>"#);
    let t = doc.get_element_by_id("t").unwrap();
    set_inner_html(&mut doc, t, "a</div></main><span>b").unwrap();
    assert_eq!(inner_html(&doc, t), "a<span>b</span>");
    assert_eq!(
        inner_html(&doc, body(&doc)),
        r#"<main><div id="t">a<span>b</span></div><p id="after">after</p></main>"#
    );
}

#[test]
fn table_rows_parse_inside_a_table_section() {
    // The implied-close rules must stop at the target: a `<tr>` fragment in
    // a `<tbody>`, and a `<td>` fragment in a `<tr>`.
    let mut doc = parse(r#"<table><tbody id="b"></tbody></table>"#);
    let b = doc.get_element_by_id("b").unwrap();
    set_inner_html(&mut doc, b, "<tr><td>1<td>2<tr><td>3").unwrap();
    assert_eq!(
        inner_html(&doc, b),
        "<tr><td>1</td><td>2</td></tr><tr><td>3</td></tr>"
    );
    let row = doc.tree().children(b)[0];
    set_inner_html(&mut doc, row, "<td>x<td>y").unwrap();
    assert_eq!(outer_html(&doc, row), "<tr><td>x</td><td>y</td></tr>");
}

#[test]
fn raw_text_targets_take_the_markup_as_text() {
    let mut doc = parse("<script id=s>old()</script>");
    let s = doc.get_element_by_id("s").unwrap();
    set_inner_html(&mut doc, s, "if (a < b) { x('<p>') }").unwrap();
    assert_eq!(doc.tree().children(s).len(), 1);
    assert_eq!(doc.text_content(s), "if (a < b) { x('<p>') }");
}

#[test]
fn a_target_that_cannot_hold_children_is_refused_unchanged() {
    let mut doc = parse("<p>hi</p>");
    let p = doc.query_selector("p").unwrap().unwrap();
    let text = doc.tree().children(p)[0];
    assert_eq!(
        set_inner_html(&mut doc, text, "<b>x</b>"),
        Err(MutError::CannotHaveChildren(text))
    );
    assert_eq!(inner_html(&doc, body(&doc)), "<p>hi</p>");
}

#[test]
fn the_document_root_is_a_valid_target() {
    let mut doc = parse("<p>a</p>");
    let root = doc.root();
    set_inner_html(&mut doc, root, "<!doctype html><div>b</div>").unwrap();
    assert_eq!(inner_html(&doc, root), "<div>b</div>");
}
