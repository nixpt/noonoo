//! `Engine::restyle`: styles kept current across DOM edits, stylesheet
//! edits, viewport changes and hover, matching a fresh compute each time.

use noonoo_css::Color;
use noonoo_engine::Engine;
use noonoo_geometry::Size;
use noonoo_style::{Interaction, Styles};

const PAGE: &str = r#"<html><head><style id="sheet">.on { color: red } .w { width: 50vw }</style></head>
<body><ul id="list"><li id="a">a</li><li id="b">b</li></ul><p id="p" class="w">p</p></body></html>"#;

const VIEW: Size = Size {
    width: 800.0,
    height: 600.0,
};

fn assert_fresh(engine: &Engine, styles: &Styles, viewport: Size, interaction: Interaction) {
    let fresh = engine.compute_styles_in(viewport, interaction);
    for element in engine.dom().elements() {
        assert_eq!(styles.get(element), fresh.get(element), "{element:?}");
    }
    assert_eq!(styles.generated(), fresh.generated());
}

fn id(engine: &Engine, name: &str) -> noonoo_dom::NodeId {
    engine.dom().get_element_by_id(name).unwrap()
}

fn ready() -> (Engine, Styles) {
    let mut engine = Engine::from_html(PAGE);
    let mut styles = Styles::default();
    let all = engine.dom().elements().len();
    assert_eq!(
        engine.restyle(&mut styles, VIEW, Interaction::default()),
        all,
        "the first restyle computes everything"
    );
    (engine, styles)
}

#[test]
fn a_dom_edit_restyles_only_what_it_reaches() {
    let (mut engine, mut styles) = ready();
    let a = id(&engine, "a");
    engine
        .dom_mut()
        .mutate()
        .set_attribute(a, "class", "on")
        .unwrap();
    assert_eq!(engine.restyle(&mut styles, VIEW, Interaction::default()), 2);
    assert_eq!(styles.get(a).unwrap().color, Color::rgb(255, 0, 0));
    assert_fresh(&engine, &styles, VIEW, Interaction::default());
    assert_eq!(engine.restyle(&mut styles, VIEW, Interaction::default()), 0);
}

#[test]
fn editing_a_style_element_restyles_with_the_new_sheet() {
    let (mut engine, mut styles) = ready();
    let sheet = id(&engine, "sheet");
    let text = engine.dom().tree().children(sheet)[0];
    engine
        .dom_mut()
        .mutate()
        .set_text(text, "li { color: blue }")
        .unwrap();
    engine.restyle(&mut styles, VIEW, Interaction::default());
    assert_eq!(
        styles.get(id(&engine, "a")).unwrap().color,
        Color::rgb(0, 0, 255)
    );
    assert_fresh(&engine, &styles, VIEW, Interaction::default());
}

#[test]
fn adding_or_removing_a_style_element_changes_the_sheets() {
    let (mut engine, mut styles) = ready();
    let body = engine.dom().get_elements_by_tag_name("body")[0];
    let style = engine.dom_mut().create_element("style");
    let css = engine.dom_mut().create_text("#b { color: lime }");
    engine.dom_mut().mutate().append_child(style, css).unwrap();
    engine.dom_mut().mutate().append_child(body, style).unwrap();
    engine.restyle(&mut styles, VIEW, Interaction::default());
    assert_eq!(
        styles.get(id(&engine, "b")).unwrap().color,
        Color::rgb(0, 255, 0)
    );
    assert_fresh(&engine, &styles, VIEW, Interaction::default());

    engine.dom_mut().mutate().remove(style).unwrap();
    engine.restyle(&mut styles, VIEW, Interaction::default());
    assert_eq!(
        styles.get(id(&engine, "b")).unwrap().color,
        Color::rgb(0, 0, 0)
    );
    assert_fresh(&engine, &styles, VIEW, Interaction::default());
}

#[test]
fn a_new_viewport_restyles_everything() {
    let (mut engine, mut styles) = ready();
    let wide = Size {
        width: 1000.0,
        height: 600.0,
    };
    let all = engine.dom().elements().len();
    assert_eq!(
        engine.restyle(&mut styles, wide, Interaction::default()),
        all
    );
    assert_fresh(&engine, &styles, wide, Interaction::default());
}

#[test]
fn a_hover_move_restyles_its_nodes() {
    let (mut engine, mut styles) = ready();
    let b = id(&engine, "b");
    let hover = Interaction {
        hovered: Some(b),
        ..Interaction::default()
    };
    assert_eq!(engine.restyle(&mut styles, VIEW, hover), 2);
    assert_fresh(&engine, &styles, VIEW, hover);
}

#[test]
fn replacing_a_style_elements_text_or_writing_the_raw_tree_is_seen() {
    let (mut engine, mut styles) = ready();
    let sheet = id(&engine, "sheet");
    engine
        .dom_mut()
        .mutate()
        .set_text(sheet, "li { color: blue }")
        .unwrap();
    engine.restyle(&mut styles, VIEW, Interaction::default());
    assert_eq!(
        styles.get(id(&engine, "a")).unwrap().color,
        Color::rgb(0, 0, 255)
    );

    let text = engine.dom().tree().children(sheet)[0];
    if let Some(node) = engine.dom_mut().tree_mut().get_mut(text) {
        node.data = noonoo_dom::NodeData::Text("li { color: lime }".into());
    }
    engine.restyle(&mut styles, VIEW, Interaction::default());
    assert_eq!(
        styles.get(id(&engine, "a")).unwrap().color,
        Color::rgb(0, 255, 0)
    );
    assert_fresh(&engine, &styles, VIEW, Interaction::default());
}
