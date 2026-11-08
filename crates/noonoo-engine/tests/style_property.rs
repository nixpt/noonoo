//! A script's `el.style.setProperty` reaches layout on the next pass, and
//! arniko's own stylesheet survives being replayed one property at a time.

use noonoo_css::{Rule, Stylesheet};
use noonoo_engine::Engine;
use noonoo_geometry::Size;
use noonoo_layout::MetricShaper;
use noonoo_style::{remove_style_property, set_style_property};

const VIEWPORT: Size = Size::new(800.0, 600.0);

fn width_of(engine: &Engine, tag: &str) -> f32 {
    let node = engine.dom().get_elements_by_tag_name(tag)[0];
    engine
        .layout(VIEWPORT, &MetricShaper)
        .get(node)
        .unwrap()
        .rect
        .width
}

#[test]
fn a_width_set_from_script_changes_the_layout_box_and_removal_reverts_it() {
    let mut engine = Engine::from_html("<style>div { width: 100px }</style><div>a</div><p>b</p>");
    let div = engine.dom().get_elements_by_tag_name("div")[0];
    assert_eq!(width_of(&engine, "div"), 100.0);

    set_style_property(engine.dom_mut(), div, "width", "250px", false).unwrap();
    assert_eq!(width_of(&engine, "div"), 250.0);

    remove_style_property(engine.dom_mut(), div, "width").unwrap();
    assert_eq!(width_of(&engine, "div"), 100.0);
}

#[test]
fn an_important_stylesheet_rule_beats_a_normal_inline_set() {
    let mut engine = Engine::from_html("<style>div { width: 100px !important }</style><div></div>");
    let div = engine.dom().get_elements_by_tag_name("div")[0];
    set_style_property(engine.dom_mut(), div, "width", "250px", false).unwrap();
    assert_eq!(width_of(&engine, "div"), 100.0);
    set_style_property(engine.dom_mut(), div, "width", "250px", true).unwrap();
    assert_eq!(width_of(&engine, "div"), 250.0);
}

/// The calls arniko's toast makes.
#[test]
fn arnikos_toast_edits_are_accepted() {
    let mut engine = Engine::from_html("<div></div>");
    let div = engine.dom().get_elements_by_tag_name("div")[0];
    for (name, value) in [
        ("opacity", "0"),
        ("transform", "translateX(-50%) translateY(20px)"),
        ("opacity", "1"),
        ("transform", "translateX(-50%) translateY(0)"),
    ] {
        set_style_property(engine.dom_mut(), div, name, value, false).unwrap();
    }
    let style = engine
        .dom()
        .element(div)
        .unwrap()
        .attr("style")
        .unwrap()
        .to_owned();
    assert_eq!(
        style,
        "opacity: 1; transform: translateX(-50%) translateY(0)"
    );
    let _ = engine.paint(VIEWPORT, &MetricShaper);
}

/// Every declaration in arniko's stylesheet, set inline on a styled page:
/// nothing panics, and every one is accepted.
#[test]
fn arnikos_stylesheet_replays_through_set_style_property() {
    let css = include_str!("fixtures/arniko_styles.css");
    let sheet = Stylesheet::parse(css).unwrap();
    let declarations: Vec<_> = sheet
        .rules
        .iter()
        .filter_map(|rule| match rule {
            Rule::Qualified { declarations, .. } => Some(declarations),
            Rule::At { .. } => None,
        })
        .flatten()
        .collect();
    assert!(declarations.len() > 500, "{}", declarations.len());

    let mut engine = Engine::from_html(&format!(
        "<style>{css}</style><div class=\"arniko-card\"><span>text</span></div>"
    ));
    let div = engine.dom().get_elements_by_tag_name("div")[0];
    let mut refused = Vec::new();
    for d in &declarations {
        if set_style_property(engine.dom_mut(), div, &d.name, &d.value, d.important).is_err() {
            refused.push(format!("{}: {}", d.name, d.value));
        }
    }
    let _ = engine.paint(VIEWPORT, &MetricShaper);
    assert!(
        refused.is_empty(),
        "{} of {} refused: {refused:#?}",
        refused.len(),
        declarations.len()
    );
}
