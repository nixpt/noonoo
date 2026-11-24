//! `:has()`: an element by what is inside or after it.

use noonoo_dom::Document;
use noonoo_html::parse;
use noonoo_query::{matches, query_selector_all};

fn page() -> Document {
    parse(
        r#"<!doctype html><body id="b">
        <section id="s1"><h2 id="h">t</h2><p id="p1"><a id="a1" class="x" href="/">l</a></p></section>
        <section id="s2"><p id="p2">t<em id="e">e</em></p>text</section>
        <section id="s3"></section>
        <form id="f"><label id="l"><input id="i" type="checkbox"></label><div id="d"><p id="p3"><b id="k"></b></p></div></form>
        </body>"#,
    )
}

fn all(selector: &str) -> Vec<String> {
    let doc = page();
    query_selector_all(&doc, selector)
        .unwrap()
        .into_iter()
        .map(|node| doc.element(node).unwrap().id().unwrap_or("?").to_owned())
        .collect()
}

#[test]
fn a_descendant_is_the_default() {
    assert_eq!(all("section:has(a)"), ["s1"]);
    assert_eq!(all("section:has(a.x[href])"), ["s1"]);
    assert_eq!(all("section:has( em )"), ["s2"]);
    assert_eq!(all(":has(b)"), ["?", "b", "f", "d", "p3"]);
    assert_eq!(all("section:has(section)"), Vec::<String>::new());
    assert_eq!(all("section:has(form)"), Vec::<String>::new());
    assert_eq!(all("section:has(*)"), ["s1", "s2"]);
}

#[test]
fn a_child_a_next_sibling_or_any_later_sibling() {
    assert_eq!(all("section:has(> a)"), Vec::<String>::new());
    assert_eq!(all("section:has(> p)"), ["s1", "s2"]);
    assert_eq!(all(":has(>a)"), ["p1"]);
    assert_eq!(all("section:has(+ section)"), ["s1", "s2"]);
    assert_eq!(all("section:has(+ form)"), ["s3"]);
    assert_eq!(all("section:has(~ form)"), ["s1", "s2", "s3"]);
    assert_eq!(all("section:has(~ #s1)"), Vec::<String>::new());
    assert_eq!(all("#s3:has(+ section)"), Vec::<String>::new());
}

#[test]
fn a_longer_relative_selector_starts_from_the_element() {
    assert_eq!(all("section:has(p a)"), ["s1"]);
    // The `p` has to be inside too: `#p1` holds the `a`, but no `p`.
    assert_eq!(all("p:has(p a)"), Vec::<String>::new());
    assert_eq!(all("p:has(section a)"), Vec::<String>::new());
    // What `:is()` looks at isn't bounded by the element.
    assert_eq!(all("p:has(:is(section a))"), ["p1"]);
    assert_eq!(all("section:has(> h2 + p > a)"), ["s1"]);
    assert_eq!(all("section:has(> h2 > a)"), Vec::<String>::new());
    assert_eq!(all("section:has(+ section em)"), ["s1"]);
    assert_eq!(all("section:has(+ section > em)"), Vec::<String>::new());
    assert_eq!(all("section:has(~ section ~ form b)"), ["s1", "s2"]);
    assert_eq!(all("form:has(> label ~ div b)"), ["f"]);
    assert_eq!(all("h2:has(+ p ~ p)"), Vec::<String>::new());
}

#[test]
fn it_combines_with_the_rest_of_a_selector() {
    assert_eq!(all("section:has(h2, em)"), ["s1", "s2"]);
    assert_eq!(all("section:not(:has(p))"), ["s3"]);
    assert_eq!(all("section:has(p):has(h2)"), ["s1"]);
    assert_eq!(all("section:has(h2) a"), ["a1"]);
    assert_eq!(all("section:has(h2) + section em"), ["e"]);
    assert_eq!(all("section:has(p:not(:first-child))"), ["s1"]);
    assert_eq!(all("label:has([type=checkbox])"), ["l"]);
    assert_eq!(all("html:HAS(#k) > body"), ["b"]);
    let doc = page();
    let s1 = doc.get_element_by_id("s1").unwrap();
    assert!(matches(&doc, s1, ":has(> h2)").unwrap());
    assert!(!matches(&doc, s1, ":has(> em)").unwrap());
}

#[test]
fn what_is_not_a_relative_selector_list_is_an_error() {
    let doc = page();
    for selector in [
        ":has()",
        ":has(",
        ":has(a,)",
        ":has(,a)",
        ":has(>)",
        ":has(> > a)",
        ":has(a >)",
        ":has(a::before)",
        ":has(:has(a))",
        ":has(:not(:has(a)))",
        ":has(a:is(b:has(i)))",
        ":has(:is(a::after))",
    ] {
        assert!(
            query_selector_all(&doc, selector).is_err(),
            "{selector} should be an error"
        );
    }
}

#[test]
fn a_deep_tree_and_a_long_selector_stay_cheap() {
    let doc = parse(&format!("{}<i>", "<div class=a><div class=b>".repeat(200)));
    let chain = "div ".repeat(60);
    assert_eq!(
        query_selector_all(&doc, &format!("body:has({chain} u)"))
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        query_selector_all(&doc, &format!("body:has({chain} i)"))
            .unwrap()
            .len(),
        1
    );
}

/// `e:has(> x y)` holds when `> x y`, written after a selector only `e`
/// matches, matches something.
#[test]
fn it_agrees_with_the_selector_it_stands_for() {
    let mut seed = 7u32;
    let mut next = |n: usize| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 16) as usize % n
    };
    let (mut hits, mut tries) = (0, 0);
    for _ in 0..60 {
        let mut html = String::from("<!doctype html>");
        for _ in 0..30 {
            html.push_str(["<div>", "<b>", "<i>", "</div>", "</b>", "</i>", "<u></u>"][next(7)]);
        }
        let mut doc = parse(&html);
        for _ in 0..12 {
            let mut relative = String::new();
            for part in 0..1 + next(4) {
                let combinators = [" ", "> ", "+ ", "~ "];
                relative.push_str(if part == 0 { "" } else { " " });
                relative.push_str(combinators[next(4)]);
                relative.push_str(["div", "b", "i", "u", "*"][next(5)]);
            }
            for element in doc.elements() {
                let has = matches(&doc, element, &format!(":has({relative})")).unwrap();
                doc.mutate().set_attribute(element, "at", "").unwrap();
                let found = query_selector_all(&doc, &format!("[at] {relative}")).unwrap();
                doc.mutate().remove_attribute(element, "at").unwrap();
                assert_eq!(has, !found.is_empty(), "{relative} in {html}");
                hits += usize::from(has);
                tries += 1;
            }
        }
    }
    assert!(hits > 500 && tries > 5 * hits / 4, "{hits} of {tries}");
}
