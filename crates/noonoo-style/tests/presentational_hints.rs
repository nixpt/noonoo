//! HTML's presentational attributes (the rendering section's "presentational
//! hints"): they style like author CSS that every author rule overrides.

use noonoo_css::Stylesheet;
use noonoo_style::{Dimension, compute};

fn width_of(html: &str, css: &str, id: &str) -> Dimension {
    let doc = noonoo_html::parse(html);
    let sheets: Vec<Stylesheet> = if css.is_empty() {
        Vec::new()
    } else {
        vec![Stylesheet::parse(css).unwrap()]
    };
    let styles = compute(&doc, &sheets);
    styles
        .get(doc.get_element_by_id(id).unwrap())
        .unwrap()
        .width
        .clone()
}

#[test]
fn a_width_attribute_sets_the_width_of_tables_and_cells() {
    let html = r#"<table id="t" width="85%"><tr><td id="d" width="40">a</td><th id="h" width=" 12.5 ">b</th></tr></table>"#;
    assert_eq!(width_of(html, "", "t"), Dimension::Percent(85.0));
    assert_eq!(width_of(html, "", "d"), Dimension::Px(40.0));
    assert_eq!(width_of(html, "", "h"), Dimension::Px(12.5));
}

#[test]
fn author_css_overrides_a_width_attribute() {
    let html = r#"<table id="t" width="85%"><tr><td>a</td></tr></table>"#;
    assert_eq!(
        width_of(html, "* { width: 10px }", "t"),
        Dimension::Px(10.0)
    );
}

#[test]
fn an_invalid_or_foreign_width_attribute_is_ignored() {
    let html = r#"<table id="t" width="wide"><tr><td>a</td></tr></table><div id="d" width="40"></div><table id="z" width="0"><tr><td>a</td></tr></table>"#;
    assert_eq!(width_of(html, "", "t"), Dimension::Auto);
    assert_eq!(width_of(html, "", "d"), Dimension::Auto);
    // HTML: a zero width is no hint.
    assert_eq!(width_of(html, "", "z"), Dimension::Auto);
}
