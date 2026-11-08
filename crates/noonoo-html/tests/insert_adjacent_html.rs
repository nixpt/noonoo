//! `insert_adjacent_html`: the four `insertAdjacentHTML` positions.

use noonoo_dom::MutError;
use noonoo_html::serialize::inner_html;
use noonoo_html::{Adjacent, insert_adjacent_html, parse};

fn page() -> noonoo_dom::Document {
    parse(r#"<main><div id="a"><p>x</p></div></main>"#)
}

/// The body's content: the page's own markup.
fn html(doc: &noonoo_dom::Document) -> String {
    inner_html(doc, doc.get_elements_by_tag_name("body")[0])
}

#[test]
fn each_position_puts_the_fragment_in_its_place() {
    let cases = [
        (
            Adjacent::BeforeBegin,
            r#"<main><i>1</i>2<div id="a"><p>x</p></div></main>"#,
        ),
        (
            Adjacent::AfterBegin,
            r#"<main><div id="a"><i>1</i>2<p>x</p></div></main>"#,
        ),
        (
            Adjacent::BeforeEnd,
            r#"<main><div id="a"><p>x</p><i>1</i>2</div></main>"#,
        ),
        (
            Adjacent::AfterEnd,
            r#"<main><div id="a"><p>x</p></div><i>1</i>2</main>"#,
        ),
    ];
    for (position, expected) in cases {
        let mut doc = page();
        let a = doc.get_element_by_id("a").unwrap();
        insert_adjacent_html(&mut doc, a, position, "<i>1</i>2").unwrap();
        assert_eq!(html(&doc), expected, "{position:?}");
    }
}

#[test]
fn it_returns_the_inserted_nodes_in_order() {
    let mut doc = page();
    let a = doc.get_element_by_id("a").unwrap();
    let nodes = insert_adjacent_html(&mut doc, a, Adjacent::AfterBegin, "<i>1</i>2").unwrap();
    assert_eq!(nodes.len(), 2);
    assert_eq!(&doc.tree().children(a)[..2], &nodes[..]);
    assert_eq!(doc.element(nodes[0]).unwrap().name, "i");
}

#[test]
fn after_end_lands_before_the_next_sibling_and_after_begin_works_when_empty() {
    let mut doc = parse(r#"<main><div id="a"></div><p id="n">n</p></main>"#);
    let a = doc.get_element_by_id("a").unwrap();
    insert_adjacent_html(&mut doc, a, Adjacent::AfterEnd, "<b>m</b>").unwrap();
    insert_adjacent_html(&mut doc, a, Adjacent::AfterBegin, "in").unwrap();
    assert_eq!(
        html(&doc),
        r#"<main><div id="a">in</div><b>m</b><p id="n">n</p></main>"#
    );
}

#[test]
fn the_fragment_cannot_escape_its_context() {
    let mut doc = page();
    let a = doc.get_element_by_id("a").unwrap();
    insert_adjacent_html(&mut doc, a, Adjacent::BeforeEnd, "y</div></main><b>z").unwrap();
    insert_adjacent_html(&mut doc, a, Adjacent::AfterEnd, "</main><s>w").unwrap();
    assert_eq!(
        html(&doc),
        r#"<main><div id="a"><p>x</p>y<b>z</b></div><s>w</s></main>"#
    );
}

#[test]
fn rows_insert_around_a_row() {
    let mut doc = parse(r#"<table><tbody><tr id="r"><td>1</td></tr></tbody></table>"#);
    let r = doc.get_element_by_id("r").unwrap();
    insert_adjacent_html(&mut doc, r, Adjacent::AfterEnd, "<tr><td>2").unwrap();
    insert_adjacent_html(&mut doc, r, Adjacent::BeforeEnd, "<td>1b").unwrap();
    assert_eq!(
        html(&doc),
        r#"<table><tbody><tr id="r"><td>1</td><td>1b</td></tr><tr><td>2</td></tr></tbody></table>"#
    );
}

#[test]
fn a_raw_text_context_takes_the_markup_as_text() {
    let mut doc = parse("<script id=s>a();</script>");
    let s = doc.get_element_by_id("s").unwrap();
    insert_adjacent_html(&mut doc, s, Adjacent::BeforeEnd, "b('<p>');").unwrap();
    assert_eq!(doc.text_content(s), "a();b('<p>');");
    assert!(
        doc.tree()
            .children(s)
            .iter()
            .all(|&c| doc.element(c).is_none())
    );
}

#[test]
fn impossible_positions_are_refused_unchanged() {
    let mut doc = page();
    let before = html(&doc);
    let loose = doc.create_element("div");
    assert_eq!(
        insert_adjacent_html(&mut doc, loose, Adjacent::BeforeBegin, "<b>x</b>"),
        Err(MutError::NoParent(loose))
    );
    assert_eq!(
        insert_adjacent_html(&mut doc, loose, Adjacent::AfterEnd, "<b>x</b>"),
        Err(MutError::NoParent(loose))
    );
    let p = doc.query_selector("p").unwrap().unwrap();
    let text = doc.tree().children(p)[0];
    assert_eq!(
        insert_adjacent_html(&mut doc, text, Adjacent::AfterBegin, "<b>x</b>"),
        Err(MutError::CannotHaveChildren(text))
    );
    assert_eq!(html(&doc), before);
    assert!(doc.tree().children(loose).is_empty());
}
