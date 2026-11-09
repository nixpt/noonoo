//! `vertical-align`: the keywords, signed lengths and percentages, and the
//! UA sheet's `sub`, `sup` and `small`.

use noonoo_css::Stylesheet;
use noonoo_style::{VerticalAlign, compute};

fn aligns(css: &str, html: &str) -> Vec<VerticalAlign> {
    let doc = noonoo_html::parse(&format!("<!doctype html>{html}"));
    let styles = compute(&doc, &[Stylesheet::parse(css).unwrap()]);
    doc.get_elements_by_tag_name("i")
        .into_iter()
        .map(|node| styles.get(node).unwrap().vertical_align)
        .collect()
}

#[test]
fn keywords_lengths_and_percentages_parse() {
    let css = "i:nth-child(1) { vertical-align: top } i:nth-child(2) { vertical-align: middle }
               i:nth-child(3) { vertical-align: text-bottom } i:nth-child(4) { vertical-align: -2px }
               i:nth-child(5) { vertical-align: 50% } i:nth-child(6) { vertical-align: 0.5em }
               i:nth-child(7) { vertical-align: bogus }";
    let html = "<p><i></i><i></i><i></i><i></i><i></i><i></i><i></i></p>";
    assert_eq!(
        aligns(css, html),
        [
            VerticalAlign::Top,
            VerticalAlign::Middle,
            VerticalAlign::TextBottom,
            VerticalAlign::Length(-2.0),
            VerticalAlign::Percent(50.0),
            VerticalAlign::Length(8.0),
            VerticalAlign::Baseline,
        ]
    );
}

#[test]
fn vertical_align_does_not_inherit() {
    assert_eq!(
        aligns("b { vertical-align: super }", "<b><i></i></b>"),
        [VerticalAlign::Baseline]
    );
}

#[test]
fn sub_and_sup_shift_and_shrink() {
    let doc = noonoo_html::parse("<!doctype html><p><sub>a</sub><sup>b</sup><small>c</small></p>");
    let styles = compute(&doc, &[]);
    let of = |tag: &str| styles.get(doc.get_elements_by_tag_name(tag)[0]).unwrap();
    assert_eq!(of("sub").vertical_align, VerticalAlign::Sub);
    assert_eq!(of("sup").vertical_align, VerticalAlign::Super);
    for tag in ["sub", "sup", "small"] {
        assert!(of(tag).font_size < 16.0, "{tag}: {}", of(tag).font_size);
    }
    // `line-height: normal`, so a parent's fixed line height doesn't
    // stretch them.
    assert_eq!(of("sub").line_height, None);
}
