//! Where character references are decoded (HTML §13.2.5.72 on).

mod common;

use common::{check, tree};

#[test]
fn a_reference_without_its_semicolon() {
    check(&[
        ("&copy 1 &copy1", r#""© 1 ©1""#),
        ("I'm &notit; I'm &notin;", r#""I'm ¬it; I'm ∉""#),
        ("&ampamp; &#65B &#x41G", r#""&amp; AB AG""#),
        // Not every name has that form.
        ("&hellip &hellip;", r#""&hellip …""#),
    ]);
}

#[test]
fn an_attribute_value_keeps_what_may_be_a_query() {
    check(&[
        (
            "<a href='?x=1&copy=2&lt3&notit;'>",
            "a(href=?x=1&copy=2&lt3&notit;)",
        ),
        ("<a href=?x&amp;y&copy>", "a(href=?x&y©)"),
        ("<a title='&copy; &copy &copy-'>", "a(title=© © ©-)"),
    ]);
}

#[test]
fn titles_and_text_areas_decode_references_but_hold_no_markup() {
    check(&[
        (
            "<textarea>a &lt;b> <i>x</i> &copy</textarea>c",
            r#"textarea["a <b> <i>x</i> ©"] "c""#,
        ),
        ("<textarea>\n\n&amp;", r#"textarea["\n&"]"#),
        (
            "<textarea>&lt;/textarea></textarea>",
            r#"textarea["</textarea>"]"#,
        ),
        (
            "<select><option value='a&amp;b'>x &lt; y<option>a&copyb</select>",
            r#"select[option(value=a&b)["x < y"] option["a©b"]]"#,
        ),
        ("<p><title>&amp;</title>", r#"p[title["&"]]"#),
    ]);
}

#[test]
fn raw_text_keeps_references() {
    check(&[(
        "<p><script>a &lt; b</script><style>&amp;</style><xmp>&lt;</xmp><iframe>&lt;</iframe>",
        r#"p[script["a &lt; b"] style["&amp;"]] xmp["&lt;"] iframe["&lt;"]"#,
    )]);
}

#[test]
fn a_text_area_round_trips() {
    let source = "<textarea>&lt;/textarea> &amp;amp; a</textarea>";
    let doc = noonoo_html::parse(&format!("<!doctype html>{source}"));
    let area = doc.get_elements_by_tag_name("textarea")[0];
    assert_eq!(doc.text_content(area), "</textarea> &amp; a");
    let out = noonoo_html::serialize::outer_html(&doc, area);
    assert_eq!(out, "<textarea>&lt;/textarea&gt; &amp;amp; a</textarea>");
    let again = noonoo_html::parse(&format!("<!doctype html>{out}"));
    let body = |doc: &noonoo_dom::Document| doc.get_elements_by_tag_name("body")[0];
    assert_eq!(tree(&again, body(&again)), tree(&doc, body(&doc)));
}

#[test]
fn setting_a_text_areas_markup_decodes_it() {
    let mut doc = noonoo_html::parse("<!doctype html><textarea></textarea><script></script>");
    let area = doc.get_elements_by_tag_name("textarea")[0];
    noonoo_html::set_inner_html(&mut doc, area, "a &lt; <b>").unwrap();
    assert_eq!(doc.text_content(area), "a < <b>");
    let script = doc.get_elements_by_tag_name("script")[0];
    noonoo_html::set_inner_html(&mut doc, script, "a &lt; <b>").unwrap();
    assert_eq!(doc.text_content(script), "a &lt; <b>");
}
