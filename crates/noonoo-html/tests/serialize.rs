//! `inner_html` / `outer_html`: serialization is the inverse of `parse`.

use noonoo_html::parse;
use noonoo_html::serialize::{inner_html, outer_html};

fn round_trip(source: &str) -> String {
    let doc = parse(source);
    inner_html(&doc, doc.root())
}

#[test]
fn canonical_html_round_trips_unchanged() {
    let source = r#"<html><head><title>T</title></head><body><p class="a b" id="x">hi <b>there</b></p><!-- note --><ul><li>one</li><li>two</li></ul></body></html>"#;
    assert_eq!(round_trip(source), source);
}

#[test]
fn text_and_attribute_values_are_escaped_for_their_context() {
    let doc =
        parse(r#"<p title="a &quot;q&quot; &amp; &lt;b&gt;">Save &amp; exit &lt;now&gt; "ok"</p>"#);
    let p = doc.query_selector("p").unwrap().unwrap();
    assert_eq!(
        outer_html(&doc, p),
        r#"<p title="a &quot;q&quot; &amp; &lt;b>">Save &amp; exit &lt;now&gt; "ok"</p>"#
    );
}

#[test]
fn void_elements_take_no_end_tag_and_bare_attributes_get_an_empty_value() {
    assert_eq!(
        round_trip(r#"<p>a<br>b<img src="x.png" alt><input disabled></p>"#),
        r#"<p>a<br>b<img src="x.png" alt=""><input disabled=""></p>"#
    );
}

#[test]
fn raw_text_content_is_emitted_verbatim() {
    let source = "<script>if (a < b && c > d) { x = \"&amp;\"; }</script><style>p > a { color: red }</style>";
    assert_eq!(round_trip(source), source);
}

#[test]
fn a_doctype_is_dropped() {
    assert_eq!(round_trip("<!doctype html><p>x</p>"), "<p>x</p>");
}

#[test]
fn inner_html_is_the_children_and_outer_html_adds_the_node() {
    let doc = parse(r#"<div id="d"><span>a</span>b</div>"#);
    let d = doc.get_element_by_id("d").unwrap();
    assert_eq!(inner_html(&doc, d), "<span>a</span>b");
    assert_eq!(outer_html(&doc, d), r#"<div id="d"><span>a</span>b</div>"#);
}

#[test]
fn the_document_node_serializes_as_its_children() {
    let doc = parse("<p>a</p><p>b</p>");
    assert_eq!(outer_html(&doc, doc.root()), "<p>a</p><p>b</p>");
}

#[test]
fn serializing_a_reparse_changes_nothing() {
    // Messy input (case, unquoted values, entities, implied close): the
    // first serialization normalises it, after which it is a fixed point.
    let once = round_trip("<DIV Class=box data-n=1><P>a &amp; b<p>c</DIV>&copy; done");
    assert_eq!(round_trip(&once), once);
}
