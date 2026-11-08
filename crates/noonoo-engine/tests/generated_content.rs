//! arniko's `::before`/`::after` components, through the whole pipeline.

use noonoo_engine::Engine;
use noonoo_geometry::Size;
use noonoo_layout::MetricShaper;
use noonoo_paint::{Color, Command};
use noonoo_style::Interaction;

const VIEWPORT: Size = Size::new(800.0, 600.0);

fn page(body: &str) -> Engine {
    let css = include_str!("fixtures/arniko_styles.css");
    Engine::from_html(&format!("<style>{css}</style>{body}"))
}

fn paint(engine: &Engine, interaction: Interaction) -> Vec<Command> {
    let styles = engine.compute_styles_in(VIEWPORT, interaction);
    let layout = noonoo_layout::layout(engine.dom(), &styles, VIEWPORT, &MetricShaper);
    noonoo_paint::paint(&styles, &layout).commands().to_vec()
}

/// The status badge's dot: a 6px round fill in the badge's colour.
#[test]
fn a_status_badge_paints_its_dot() {
    let engine =
        page(r#"<span class="arniko-status-badge arniko-status-error" id="b">Down</span>"#);
    let b = engine.dom().get_element_by_id("b").unwrap();
    let styles = engine.compute_styles_in(VIEWPORT, Interaction::default());
    let error = styles.get(b).unwrap().color;
    assert_ne!(error, Color::BLACK);
    let commands = paint(&engine, Interaction::default());
    let dot = commands.iter().any(|c| {
        matches!(c, Command::FillRect { rect, color, radius }
            if rect.width == 6.0 && rect.height == 6.0 && *color == error && radius.0 == 3.0)
    });
    assert!(dot, "{commands:#?}");
}

/// The tooltip's text is generated from `data-tooltip`, hidden until hover.
#[test]
fn a_tooltip_shows_its_attribute_on_hover() {
    let engine = page(
        r#"<div style="margin-top: 200px"><span class="arniko-tooltip" id="t"
             data-tooltip="Saved 2 minutes ago" data-tooltip-position="top">?</span></div>"#,
    );
    let t = engine.dom().get_element_by_id("t").unwrap();
    let alpha_of_tip = |interaction| {
        let commands = paint(&engine, interaction);
        let tip = commands
            .iter()
            .position(|c| matches!(c, Command::DrawText { text, .. } if text.contains("Saved")))
            .expect("the tooltip text is painted");
        // The innermost layer opened before the text sets its alpha.
        commands[..tip]
            .iter()
            .rev()
            .find_map(|c| match c {
                Command::PushLayer { alpha, .. } => Some(*alpha),
                _ => None,
            })
            .unwrap_or(1.0)
    };
    assert_eq!(alpha_of_tip(Interaction::default()), 0.0);
    let hovered = Interaction {
        hovered: Some(t),
        ..Interaction::default()
    };
    assert_ne!(alpha_of_tip(hovered), 0.0);
}
