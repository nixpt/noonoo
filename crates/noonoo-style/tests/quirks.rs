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
    // ...but never `-webkit-center` (see a_table_does_not_inherit_webkit_centre).
    assert_eq!(table.text_align, TextAlign::Start);
    assert_eq!(of(&doc, &styles, "f").margin.bottom, Dimension::Px(0.0));
}

fn margins(style: &ComputedStyle) -> (Dimension, Dimension) {
    (style.margin.top.clone(), style.margin.bottom.clone())
}

/// HTML's rendering section: in quirks mode a body, td or th child with no
/// substantial sibling before it loses its default top margin; a td or th
/// child with none after it loses its default bottom margin.
#[test]
fn in_quirks_mode_default_margins_drop_at_body_and_cell_edges() {
    let (doc, styles) = styled(
        "<body>\n<h1 id=h>t</h1><p id=after>x</p>\
         <table><tr><td>\n <p id=a>first</p> <p id=b>last</p>\n</td>\
         <td>x<p id=c>after text</p>y</td></tr></table>",
    );
    let h1 = 0.67 * 32.0;
    assert_eq!(
        margins(of(&doc, &styles, "h")),
        (Dimension::Px(0.0), Dimension::Px(h1))
    );
    // A body child doesn't lose its bottom margin, nor a later one its top.
    assert_eq!(
        margins(of(&doc, &styles, "after")),
        (Dimension::Px(16.0), Dimension::Px(16.0))
    );
    assert_eq!(
        margins(of(&doc, &styles, "a")),
        (Dimension::Px(0.0), Dimension::Px(16.0))
    );
    assert_eq!(
        margins(of(&doc, &styles, "b")),
        (Dimension::Px(16.0), Dimension::Px(0.0))
    );
    // Text is substantial: it keeps both.
    assert_eq!(
        margins(of(&doc, &styles, "c")),
        (Dimension::Px(16.0), Dimension::Px(16.0))
    );
}

#[test]
fn author_margins_and_standards_mode_keep_their_margins() {
    let doc = noonoo_html::parse("<body><h1 id=h>t</h1><p id=p>x</p>");
    let sheet = noonoo_css::Stylesheet::parse("h1 { margin-top: 5px }").unwrap();
    let styles = compute(&doc, &[sheet]);
    assert_eq!(of(&doc, &styles, "h").margin.top, Dimension::Px(5.0));
    let (doc, styles) = styled("<!doctype html><body><h1 id=h>t</h1>");
    assert_eq!(
        of(&doc, &styles, "h").margin.top,
        Dimension::Px(0.67 * 32.0)
    );
}

/// Blink resets the `-webkit-` alignments to `start` on a table, in every
/// mode: `<center>` centres the table, not the text in its cells.
#[test]
fn a_table_does_not_inherit_webkit_centre() {
    let (doc, styles) = styled("<!doctype html><center><table id=t><tr><td id=d>a</table>");
    assert_eq!(of(&doc, &styles, "t").text_align, TextAlign::Start);
    assert_eq!(of(&doc, &styles, "d").text_align, TextAlign::Start);
}

/// Chromium drops a default margin wherever it still meets the top of the
/// body or cell after collapsing: an empty box before it (info.cern.ch's
/// `<HEADER>`, holding only a hidden `title`) doesn't stop it, but a box
/// with a height, padding or border does.
#[test]
fn in_quirks_mode_empty_boxes_before_a_child_dont_keep_its_margin() {
    let top = |html: &str| {
        let (doc, styles) = styled(html);
        of(&doc, &styles, "x").margin.top.clone()
    };
    let zero = Dimension::Px(0.0);
    assert_eq!(
        top("<header><title>t</title></header><h1 id=x>a</h1>"),
        zero
    );
    assert_eq!(
        top("<div><span></span><!-- c --></div>\n<p id=x>a</p>"),
        zero
    );
    assert_eq!(
        top(r#"<div style="position: absolute; height: 9px"></div><p id=x>a</p>"#),
        zero
    );
    for kept in [
        r#"<div style="height: 20px"></div><p id=x>a</p>"#,
        r#"<div style="padding-top: 1px"></div><p id=x>a</p>"#,
        r#"<div style="border-bottom: 1px solid"></div><p id=x>a</p>"#,
        "<div><img></div><p id=x>a</p>",
        "<div>text</div><p id=x>a</p>",
    ] {
        assert_eq!(top(kept), Dimension::Px(16.0), "{kept}");
    }
}
