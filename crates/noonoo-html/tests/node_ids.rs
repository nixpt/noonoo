//! A parse is deterministic down to its [`NodeId`]s.

use noonoo_dom::{Document, NodeId};

/// Every node with its parent and what it is, in document order.
fn listing(doc: &Document) -> Vec<(NodeId, Option<NodeId>, String)> {
    doc.tree()
        .descendants(doc.root())
        .into_iter()
        .map(|id| {
            let what = format!("{:?}", doc.node(id).unwrap());
            (id, doc.tree().parent(id), what)
        })
        .collect()
}

const BROKEN: &[&str] = &[
    "<p><b>1<i>2</b>3</i>4",
    "<a href=1>x<a href=2>y</a>",
    "<b>1<p>2</b>3</p>4",
    "<a><div><div><div><div>1</a>2",
    "<p><b class=x id=y><b><b><b>1<p>2<p>3",
    "<table>oops<div>x</div><tr>a<td>1</table>",
    "<table><input type=hidden><form><input></form><select><option>a",
    "<form><form><button>1<button>2",
    "<select><div><option>a</div><input>",
    "<svg><title><p>a</title><![CDATA[b]]><div>c",
    "<template><tr><td>a</template><noscript><p>",
    "&copy &notit; <p title='&copy=1'>",
    "</p></br><td></html>a<!--b-->",
];

/// A larger document, made of the broken ones.
fn large() -> String {
    let mut source = String::from("<!doctype html><title>large</title>");
    for round in 0..200 {
        for (index, broken) in BROKEN.iter().enumerate() {
            source += &format!("<div id=n{round}-{index}>{broken}</div>\n");
        }
    }
    source
}

#[test]
fn parsing_twice_gives_the_same_ids() {
    let large = large();
    for source in BROKEN.iter().copied().chain([large.as_str()]) {
        let first = listing(&noonoo_html::parse(source));
        let second = listing(&noonoo_html::parse(source));
        assert!(first == second, "{:.60}", source);
    }
    assert!(listing(&noonoo_html::parse(&large)).len() > 10_000);
}

#[test]
fn fragments_get_the_same_ids_too() {
    let build = || {
        let mut doc = noonoo_html::parse("<!doctype html><div id=d>a</div>");
        let div = doc.get_element_by_id("d").unwrap();
        for source in BROKEN {
            noonoo_html::set_inner_html(&mut doc, div, source).unwrap();
        }
        listing(&doc)
    };
    assert_eq!(build(), build());
}

#[test]
fn ids_follow_document_order_where_nothing_is_misnested() {
    let doc = noonoo_html::parse(
        "<!doctype html><!--a--><title>b</title><body><p>c<b>d</b><p>e\
         <table><tr><td>f</table><ul><li>g<li>h</ul>",
    );
    let ids: Vec<_> = listing(&doc).iter().map(|(id, ..)| id.index()).collect();
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]), "{ids:?}");

    // Content fostered out of a table comes before it, with a later id.
    let doc = noonoo_html::parse("<!doctype html><table>a");
    let body = doc.get_elements_by_tag_name("body")[0];
    let children = doc.tree().children(body);
    assert!(children[0].index() > children[1].index());
}

#[test]
fn attribute_changes_keep_every_id() {
    let mut doc = noonoo_html::parse("<!doctype html><p id=a class=b>c<p><b>d");
    let before: Vec<_> = listing(&doc)
        .into_iter()
        .map(|(id, up, _)| (id, up))
        .collect();
    let p = doc.get_element_by_id("a").unwrap();
    doc.mutate().set_attribute(p, "class", "x").unwrap();
    doc.mutate().set_attribute(p, "title", "y").unwrap();
    doc.mutate().remove_attribute(p, "id").unwrap();
    let after: Vec<_> = listing(&doc)
        .into_iter()
        .map(|(id, up, _)| (id, up))
        .collect();
    assert_eq!(before, after);
    // The next node gets the id it would have had anyway.
    assert_eq!(doc.create_text("z").index(), before.len() + 1);
}
