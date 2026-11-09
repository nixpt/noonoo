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

fn padding_of(html: &str, css: &str, id: &str) -> noonoo_style::Length {
    let doc = noonoo_html::parse(html);
    let sheets: Vec<Stylesheet> = if css.is_empty() {
        Vec::new()
    } else {
        vec![Stylesheet::parse(css).unwrap()]
    };
    let styles = compute(&doc, &sheets);
    let style = styles.get(doc.get_element_by_id(id).unwrap()).unwrap();
    assert_eq!(style.padding.top, style.padding.left, "{id}");
    style.padding.right.clone()
}

#[test]
fn cells_have_a_pixel_of_padding_by_default() {
    let html = r#"<table><tr><td id="d">a</td><th id="h">b</th></tr></table>"#;
    assert_eq!(padding_of(html, "", "d"), noonoo_style::Length::Px(1.0));
    assert_eq!(padding_of(html, "", "h"), noonoo_style::Length::Px(1.0));
}

#[test]
fn cellpadding_pads_its_own_tables_cells() {
    let html = r#"<table cellpadding="4"><tbody><tr><td id="d">a<table><tr><td id="inner">b</td></tr></table></td></tr></tbody></table><table cellpadding="0"><tr><th id="z">c</th></tr></table>"#;
    assert_eq!(padding_of(html, "", "d"), noonoo_style::Length::Px(4.0));
    assert_eq!(padding_of(html, "", "inner"), noonoo_style::Length::Px(1.0));
    assert_eq!(padding_of(html, "", "z"), noonoo_style::Length::Px(0.0));
}

#[test]
fn author_css_overrides_cellpadding() {
    let html = r#"<table cellpadding="4"><tr><td id="d">a</td></tr></table>"#;
    assert_eq!(
        padding_of(html, "td { padding: 2px }", "d"),
        noonoo_style::Length::Px(2.0)
    );
}

#[test]
fn tables_space_their_cells_and_cellspacing_sets_it() {
    let doc = noonoo_html::parse(
        "<table id=a><tr><td>x</table><table id=b cellspacing=5><tr><td>x</table>",
    );
    let styles = noonoo_style::compute(&doc, &[]);
    let of = |id: &str| styles.get(doc.get_element_by_id(id).unwrap()).unwrap();
    assert_eq!(of("a").border_spacing, (2.0, 2.0));
    assert_eq!(of("b").border_spacing, (5.0, 5.0));
}

#[test]
fn a_border_attribute_frames_the_table_and_its_cells() {
    use noonoo_style::BorderStyle;
    let doc = noonoo_html::parse(
        "<table id=a border=2><tr><td id=ad>x</table>\
         <table id=b border><tr><th id=bd>x</table>\
         <table id=c border=0><tr><td id=cd>x</table>\
         <table id=d border=3><tr><td id=dd>x</table>",
    );
    let sheet = Stylesheet::parse("#dd { border: none }").unwrap();
    let styles = compute(&doc, &[sheet]);
    let of = |id: &str| styles.get(doc.get_element_by_id(id).unwrap()).unwrap();
    for (table, width) in [("a", 2.0), ("b", 1.0)] {
        let style = of(table);
        assert_eq!(style.border_width.left, width, "{table}");
        assert_eq!(style.border_width.bottom, width, "{table}");
        assert_eq!(style.border_style.top, BorderStyle::Outset, "{table}");
    }
    for cell in ["ad", "bd"] {
        assert_eq!(of(cell).border_width.top, 1.0, "{cell}");
        assert_eq!(of(cell).border_style.right, BorderStyle::Inset, "{cell}");
    }
    // `border=0` is no frame, and no cell borders either.
    assert_eq!(of("c").border_width.top, 0.0);
    assert_eq!(of("cd").border_width.top, 0.0);
    // Author CSS beats the hint.
    assert_eq!(of("dd").border_style.top, BorderStyle::None);
}
