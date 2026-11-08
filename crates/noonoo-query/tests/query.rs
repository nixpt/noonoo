//! Full-selector and element-scoped queries over a parsed document.

use noonoo_dom::{Document, NodeId};
use noonoo_html::parse;
use noonoo_query::{
    matches, query_selector, query_selector_all, query_selector_all_in, query_selector_in,
};

fn ids(doc: &Document, nodes: &[NodeId]) -> Vec<String> {
    nodes
        .iter()
        .map(|&n| {
            doc.element(n)
                .and_then(|e| e.id())
                .unwrap_or("?")
                .to_owned()
        })
        .collect()
}

fn page() -> Document {
    parse(
        r#"<html><body>
        <h1 id="h">t</h1><p id="p1">a</p><p id="p2" class="x">b</p>
        <ul id="u"><li id="l1"><a id="a1" href="https://e.org">e</a></li><li id="l2"><a id="a2" href="/local">l</a></li></ul>
        <table><tbody id="tb"><tr id="r1"><td>1</td></tr><tr id="r2"><td>2</td></tr></tbody></table>
        </body></html>"#,
    )
}

fn all(doc: &Document, selector: &str) -> Vec<String> {
    ids(doc, &query_selector_all(doc, selector).unwrap())
}

#[test]
fn combinators_attributes_and_pseudo_classes_match() {
    let doc = page();
    assert_eq!(all(&doc, "tbody > tr"), ["r1", "r2"]);
    assert_eq!(all(&doc, "ul a"), ["a1", "a2"]);
    assert_eq!(all(&doc, "h1 + p"), ["p1"]);
    assert_eq!(all(&doc, "h1 ~ p"), ["p1", "p2"]);
    assert_eq!(all(&doc, r#"a[href^="https"]"#), ["a1"]);
    assert_eq!(all(&doc, "p:not(.x)"), ["p1"]);
    assert_eq!(all(&doc, "li:nth-child(2)"), ["l2"]);
}

#[test]
fn a_selector_list_answers_in_document_order() {
    let doc = page();
    assert_eq!(all(&doc, "#a2, #h, .x"), ["h", "p2", "a2"]);
    let first = query_selector(&doc, "#a2, #p2").unwrap().unwrap();
    assert_eq!(ids(&doc, &[first]), ["p2"]);
    assert_eq!(query_selector(&doc, "video").unwrap(), None);
}

#[test]
fn a_scoped_query_excludes_its_root_and_stays_inside_it() {
    let doc = parse(
        r#"<div class="c" id="r"><div class="c" id="inner"><span class="c" id="deep"></span></div></div><div class="c" id="outside"></div>"#,
    );
    let r = doc.get_element_by_id("r").unwrap();
    assert_eq!(
        ids(&doc, &query_selector_all_in(&doc, r, ".c").unwrap()),
        ["inner", "deep"]
    );
    let first = query_selector_in(&doc, r, ".c").unwrap().unwrap();
    assert_eq!(ids(&doc, &[first]), ["inner"]);
}

#[test]
fn a_scoped_query_matches_against_the_whole_document() {
    // DOM rule: `el.querySelector("body .c")` finds `.c` under `el` even
    // though `body` is above `el`.
    let doc = parse(r#"<body><main id="m"><p class="c" id="c1"></p></main></body>"#);
    let m = doc.get_element_by_id("m").unwrap();
    assert_eq!(
        ids(&doc, &query_selector_all_in(&doc, m, "body .c").unwrap()),
        ["c1"]
    );
    assert_eq!(
        ids(&doc, &query_selector_all_in(&doc, m, "main > .c").unwrap()),
        ["c1"]
    );
}

#[test]
fn a_detached_subtree_can_be_queried() {
    let mut doc = parse("<p></p>");
    let frag = doc.create_element("div");
    let span = doc.create_element("span");
    doc.mutate().append_child(frag, span).unwrap();
    assert_eq!(
        query_selector_in(&doc, frag, "div > span").unwrap(),
        Some(span)
    );
    assert_eq!(
        query_selector_all(&doc, "span").unwrap(),
        Vec::<NodeId>::new()
    );
}

#[test]
fn matches_tests_one_node() {
    let doc = page();
    let p2 = doc.get_element_by_id("p2").unwrap();
    assert!(matches(&doc, p2, "body > p.x").unwrap());
    assert!(!matches(&doc, p2, "h1 + p").unwrap());
    let text = doc.tree().children(p2)[0];
    assert!(
        !matches(&doc, text, "*").unwrap(),
        "a text node is not an element"
    );
}

#[test]
fn a_malformed_selector_is_an_error_not_a_panic() {
    let doc = page();
    for bad in ["", "div >", "[", "a,,b", ":nth-child("] {
        assert!(query_selector_all(&doc, bad).is_err(), "{bad:?}");
    }
}
