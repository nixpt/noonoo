//! `set_style_property` / `remove_style_property`: a script's
//! `el.style.setProperty(...)` reaches computed style on the next compute.

use noonoo_css::{Color, InlineStyleError, Stylesheet};
use noonoo_dom::{Document, MutError};
use noonoo_style::{StyleEditError, compute, remove_style_property, set_style_property};

fn page() -> (Document, Vec<Stylesheet>) {
    let doc = noonoo_html::parse(r#"<div id="p" style="width: 1px"><span>x</span></div>"#);
    (
        doc,
        vec![Stylesheet::parse("div { color: green }").unwrap()],
    )
}

fn ids(doc: &Document) -> (noonoo_dom::NodeId, noonoo_dom::NodeId) {
    (
        doc.get_elements_by_tag_name("div")[0],
        doc.get_elements_by_tag_name("span")[0],
    )
}

#[test]
fn a_set_property_reaches_inherited_computed_style() {
    let (mut doc, sheets) = page();
    let (div, span) = ids(&doc);
    set_style_property(&mut doc, div, "color", "red", false).unwrap();
    let styles = compute(&doc, &sheets);
    assert_eq!(styles.get(div).unwrap().color, Color::rgb(255, 0, 0));
    assert_eq!(styles.get(span).unwrap().color, Color::rgb(255, 0, 0));
    assert_eq!(
        doc.element(div).unwrap().attr("style"),
        Some("width: 1px; color: red")
    );
}

#[test]
fn removing_reverts_to_the_stylesheet() {
    let (mut doc, sheets) = page();
    let (div, span) = ids(&doc);
    set_style_property(&mut doc, div, "color", "red", false).unwrap();
    remove_style_property(&mut doc, div, "color").unwrap();
    let styles = compute(&doc, &sheets);
    assert_eq!(styles.get(span).unwrap().color, Color::rgb(0, 128, 0));
    assert_eq!(doc.element(div).unwrap().attr("style"), Some("width: 1px"));
}

#[test]
fn removing_the_last_property_drops_the_attribute() {
    let (mut doc, _) = page();
    let (div, _) = ids(&doc);
    remove_style_property(&mut doc, div, "width").unwrap();
    assert_eq!(doc.element(div).unwrap().attr("style"), None);
}

#[test]
fn an_invalid_value_is_an_error_and_changes_nothing() {
    let (mut doc, _) = page();
    let (div, _) = ids(&doc);
    assert_eq!(
        set_style_property(&mut doc, div, "color", "red; display: none", false),
        Err(StyleEditError::Style(InlineStyleError::InvalidValue))
    );
    assert_eq!(doc.element(div).unwrap().attr("style"), Some("width: 1px"));
}

#[test]
fn a_non_element_is_a_mutation_error() {
    let (mut doc, _) = page();
    let root = doc.root();
    assert_eq!(
        set_style_property(&mut doc, root, "color", "red", false),
        Err(StyleEditError::Mutation(MutError::NotAnElement(root)))
    );
    assert_eq!(
        remove_style_property(&mut doc, root, "color"),
        Err(MutError::NotAnElement(root))
    );
}
