//! Foreign (SVG, MathML) elements keep their attribute names' case; HTML
//! elements fold them to lowercase, as the DOM's `setAttribute` does.

use noonoo_dom::{Document, Namespace};

#[test]
fn an_html_element_folds_attribute_names() {
    let mut doc = Document::new();
    let div = doc.create_element("div");
    assert_eq!(doc.element(div).unwrap().namespace, Namespace::Html);
    doc.mutate().set_attribute(div, "Data-X", "1").unwrap();
    assert_eq!(doc.element(div).unwrap().attr("data-x"), Some("1"));
    doc.mutate().remove_attribute(div, "DATA-X").unwrap();
    assert_eq!(doc.element(div).unwrap().attr("data-x"), None);
}

#[test]
fn a_foreign_element_keeps_attribute_case() {
    let mut doc = Document::new();
    let svg = doc.create_element_ns("svg", Namespace::Svg);
    let element = doc.element(svg).unwrap();
    assert_eq!(
        (element.name.as_str(), element.namespace),
        ("svg", Namespace::Svg)
    );
    doc.mutate()
        .set_attribute(svg, "viewBox", "0 0 1 1")
        .unwrap();
    assert_eq!(doc.element(svg).unwrap().attr("viewBox"), Some("0 0 1 1"));
    assert_eq!(doc.element(svg).unwrap().attr("viewbox"), None);
    // Removal matches the exact name.
    doc.mutate().remove_attribute(svg, "viewbox").unwrap();
    assert!(doc.element(svg).unwrap().attr("viewBox").is_some());
    doc.mutate().remove_attribute(svg, "viewBox").unwrap();
    assert!(doc.element(svg).unwrap().attr("viewBox").is_none());
    // A copy keeps the namespace.
    let copy = doc.clone_node(svg, false).unwrap();
    assert_eq!(doc.element(copy).unwrap().namespace, Namespace::Svg);
}
