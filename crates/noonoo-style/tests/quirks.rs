//! The user-agent rules HTML's rendering section applies only in quirks
//! mode: tables don't inherit text and font settings, and forms keep a
//! bottom margin.

use noonoo_style::{ComputedStyle, Dimension, Styles, TextAlign, compute};

fn styled(html: &str) -> (noonoo_dom::Document, Styles) {
    let doc = noonoo_html::parse(html);
    let styles = compute(&doc, &[]);
    (doc, styles)
}

fn of<'s>(doc: &noonoo_dom::Document, styles: &'s Styles, id: &str) -> &'s ComputedStyle {
    styles.get(doc.get_element_by_id(id).unwrap()).unwrap()
}

const PAGE: &str = r#"<center><div style="font-size: 20px; font-weight: 700; font-style: italic; white-space: nowrap; line-height: 30px"><table id="t"><tr><td id="d">a</td></tr></table><form id="f"></form></div></center>"#;

#[test]
fn in_quirks_mode_tables_reset_text_and_font_inheritance() {
    let (doc, styles) = styled(PAGE);
    let table = of(&doc, &styles, "t");
    assert_eq!(table.font_size, 16.0);
    assert_eq!(table.font_weight, 400);
    assert_eq!(table.font_style, noonoo_style::FontStyle::Normal);
    assert_eq!(table.white_space, noonoo_style::WhiteSpace::Normal);
    assert_eq!(table.line_height, None);
    assert_eq!(table.text_align, TextAlign::Start);
    // The cells inherit the reset.
    assert_eq!(of(&doc, &styles, "d").text_align, TextAlign::Start);
}

#[test]
fn in_quirks_mode_a_form_keeps_a_bottom_margin() {
    let (doc, styles) = styled(PAGE);
    // `1em` of the form's 20px font.
    assert_eq!(of(&doc, &styles, "f").margin.bottom, Dimension::Px(20.0));
}

#[test]
fn in_no_quirks_mode_tables_and_forms_inherit_as_usual() {
    let (doc, styles) = styled(&format!("<!DOCTYPE html>{PAGE}"));
    let table = of(&doc, &styles, "t");
    assert_eq!(table.font_size, 20.0);
    assert_eq!(table.text_align, TextAlign::WebkitCenter);
    assert_eq!(of(&doc, &styles, "f").margin.bottom, Dimension::Px(0.0));
}
