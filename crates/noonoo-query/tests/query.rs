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

/// Selectors § 6.4: an attribute selector's name matches an HTML element's
/// attributes ignoring ASCII case, a foreign (SVG) element's exactly.
#[test]
fn attribute_names_match_by_namespace() {
    let doc =
        noonoo_html::parse(r#"<div id=h data-x="1"></div><svg id=s viewBox="0 0 1 1"></svg>"#);
    let h = doc.get_element_by_id("h").unwrap();
    let s = doc.get_element_by_id("s").unwrap();
    assert!(noonoo_query::matches(&doc, h, "[DATA-X]").unwrap());
    assert!(noonoo_query::matches(&doc, h, "[data-x]").unwrap());
    assert!(noonoo_query::matches(&doc, s, "[viewBox]").unwrap());
    assert!(!noonoo_query::matches(&doc, s, "[viewbox]").unwrap());
    assert_eq!(
        noonoo_query::query_selector(&doc, "[viewBox='0 0 1 1']").unwrap(),
        Some(s)
    );
}

#[test]
fn functions_nest_to_a_limit() {
    let doc = page();
    assert_eq!(all(&doc, "li:not(:nth-child(1))"), ["l2"]);
    assert_eq!(all(&doc, "li:not(:is(#l1, :where(p)))"), ["l2"]);
    let nested = |depth: usize| format!("li{}#l1{}", ":not(".repeat(depth), ")".repeat(depth));
    assert_eq!(all(&doc, &nested(32)), ["l1"]);
    for depth in [33, 10_000] {
        assert!(query_selector_all(&doc, &nested(depth)).is_err());
    }
}

#[test]
fn a_selector_that_is_not_evaluated_is_an_error() {
    let doc = page();
    for selector in [
        "li:empty",
        "input:checked",
        "li:first-of-type",
        "li:visited",
        "li:frobnicate",
        "p, li:empty",
        "li:not(:empty)",
        "li:is(a, p:empty) > b",
        "li:has(> a:empty)",
        "li:nth-last-child(1)",
        "li:nth-of-type(1)",
        "li:nth-child(1 of .x)",
        "a:lang(en)",
        "a[href='/LOCAL' i]",
        "svg|a",
        "li:not",
        "li:is(",
        "li:not()",
        "li >",
        "",
    ] {
        assert!(
            query_selector_all(&doc, selector).is_err(),
            "{selector} should be an error"
        );
    }
    // Known, though a query finds none: nothing is hovered, and a
    // pseudo-element isn't an element.
    assert!(all(&doc, "li:hover").is_empty());
    assert!(all(&doc, "li::before").is_empty());
}
