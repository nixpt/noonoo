//! `initial`, `inherit` and `unset` on properties the cascade resolves on
//! their own path, such as `font-size`.

use noonoo_css::Stylesheet;
use noonoo_style::compute;

#[test]
fn font_size_takes_the_css_wide_keywords() {
    let doc = noonoo_html::parse(
        r#"<div style="font-size: 20px"><p id="i">a</p><p id="h">b</p><p id="u">c</p></div>"#,
    );
    let sheet = Stylesheet::parse(
        "p { font-size: 30px } #i { font-size: initial } #h { font-size: inherit } #u { font-size: unset }",
    )
    .unwrap();
    let styles = compute(&doc, &[sheet]);
    let size = |id: &str| {
        styles
            .get(doc.get_element_by_id(id).unwrap())
            .unwrap()
            .font_size
    };
    assert_eq!(size("i"), 16.0);
    assert_eq!(size("h"), 20.0);
    assert_eq!(size("u"), 20.0);
}
