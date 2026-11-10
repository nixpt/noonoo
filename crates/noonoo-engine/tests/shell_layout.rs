//! Layout gaps found rendering super-surfer's chrome (`shell.html` with its
//! normalized stylesheet), each reduced to the rules that showed it.

use noonoo_engine::Engine;
use noonoo_geometry::{Rect, Size};
use noonoo_layout::{LayoutTree, MetricShaper};
use noonoo_paint::Command;
use noonoo_style::Interaction;

const VIEWPORT: Size = Size::new(800.0, 600.0);

/// Drops the UA's `body` margin, so a snippet's boxes start at the origin.
const NO_BODY_MARGIN: &str = "<style>body { margin: 0 }</style>";

fn layout(html: &str) -> (Engine, LayoutTree) {
    let engine = Engine::from_html(&format!("{NO_BODY_MARGIN}{html}"));
    let styles = engine.compute_styles_in(VIEWPORT, Interaction::default());
    let layout = noonoo_layout::layout(engine.dom(), &styles, VIEWPORT, &MetricShaper);
    (engine, layout)
}

fn rect(engine: &Engine, layout: &LayoutTree, id: &str) -> Rect {
    let node = engine.dom().get_element_by_id(id).unwrap();
    layout.get(node).unwrap().rect
}

const ROOT: &str = "<style>html, body { width: 100%; height: 100%; margin: 0 }</style>";

#[test]
fn a_percentage_height_on_the_root_resolves_against_the_viewport() {
    let (engine, layout) = layout(&format!(
        r#"{ROOT}<div id="a" style="height: 100%"></div><div style="height: 900px"></div>"#
    ));
    assert_eq!(rect(&engine, &layout, "a").height, 600.0);
}

/// The sidebar: a full-height column whose middle grows, pushing the footer
/// to the bottom.
#[test]
fn a_column_flex_item_grows_to_fill_a_full_height_sidebar() {
    let (engine, layout) = layout(&format!(
        r#"{ROOT}<div style="display: flex; height: 100%">
             <div style="width: 260px; height: 100%; display: flex; flex-direction: column">
               <div style="height: 80px"></div>
               <div id="grow" style="flex: 1 1 0px"></div>
               <div id="footer" style="height: 40px"></div>
             </div>
           </div>"#
    ));
    let footer = rect(&engine, &layout, "footer");
    assert_eq!(footer.y + footer.height, 600.0);
    assert_eq!(rect(&engine, &layout, "grow").height, 480.0);
}

/// The New Tab card: `width: 100%` clamped by `max-width`, centred by its
/// column-flex parent.
#[test]
fn align_items_center_centres_an_item_clamped_by_max_width() {
    let (engine, layout) = layout(
        r#"<div style="display: flex; flex-direction: column; align-items: center; width: 800px">
             <div id="card" style="width: 100%; max-width: 640px; height: 10px"></div>
           </div>"#,
    );
    let card = rect(&engine, &layout, "card");
    assert_eq!(card.width, 640.0);
    assert_eq!(card.x, 80.0);
}

/// The shortcuts box: `align-self: center` on an auto-width item.
#[test]
fn align_self_center_centres_an_auto_width_item() {
    let (engine, layout) = layout(
        r#"<div style="display: flex; flex-direction: column; width: 800px">
             <div id="box" style="align-self: center; max-width: 500px">
               <div style="width: 300px; height: 10px"></div>
             </div>
           </div>"#,
    );
    let item = rect(&engine, &layout, "box");
    assert_eq!(item.width, 300.0);
    assert_eq!(item.x, 250.0);
}

/// The tab strip: a flex container sized by its content must use the tab's
/// clamped width, not its unclamped content.
#[test]
fn a_shrink_to_fit_flex_container_uses_its_items_max_width() {
    let (engine, layout) = layout(
        r#"<div style="display: flex">
             <div id="tabs" style="display: flex; gap: 4px">
               <div style="max-width: 200px; display: flex">
                 <div style="width: 420px; height: 10px; flex-shrink: 0"></div>
               </div>
             </div>
             <div id="plus" style="width: 20px; height: 10px"></div>
           </div>"#,
    );
    assert_eq!(rect(&engine, &layout, "tabs").width, 200.0);
    assert_eq!(rect(&engine, &layout, "plus").x, 200.0);
}

/// "EXOSPHERE": 13px text in a 32px line sits in the middle of the line, with
/// half the extra leading above it.
#[test]
fn text_in_a_tall_line_gets_half_the_leading_above_it() {
    let engine = Engine::from_html(&format!(
        r#"{NO_BODY_MARGIN}<div style="font-size: 13px; line-height: 32px">EXOSPHERE</div>
           <div style="font-size: 10px">normal</div>"#
    ));
    let styles = engine.compute_styles_in(VIEWPORT, Interaction::default());
    let layout = noonoo_layout::layout(engine.dom(), &styles, VIEWPORT, &MetricShaper);
    let paint = noonoo_paint::paint(&styles, &layout);
    let baseline = |word: &str| {
        paint
            .commands()
            .iter()
            .find_map(|command| match command {
                Command::DrawText { text, y, .. } if text == word => Some(*y),
                _ => None,
            })
            .unwrap()
    };
    // (32 - 13 * 1.2) / 2 = 8.2, floored to 8 as Chromium does, above the
    // 13 * 0.8 ascent.
    assert!(
        (baseline("EXOSPHERE") - 18.4).abs() < 0.01,
        "{}",
        baseline("EXOSPHERE")
    );
    // A normal line height adds nothing: 32px down, then the 8px ascent.
    assert!(
        (baseline("normal") - 40.0).abs() < 0.01,
        "{}",
        baseline("normal")
    );
}

#[test]
fn a_definite_column_justifies_its_items() {
    let (engine, layout) = layout(
        r#"<div style="display: flex; flex-direction: column; justify-content: center; height: 100px">
             <div id="a" style="height: 20px"></div>
           </div>"#,
    );
    assert_eq!(rect(&engine, &layout, "a").y, 40.0);
}

#[test]
fn column_items_shrink_into_the_container_but_not_below_their_content() {
    let (engine, layout) = layout(
        r#"<div style="display: flex; flex-direction: column; height: 100px">
             <div id="a" style="height: 80px; flex-shrink: 1"></div>
             <div id="b" style="flex-basis: 80px; overflow: hidden"></div>
             <div id="c"><div style="height: 30px"></div></div>
           </div>"#,
    );
    // 190px of bases in 100px: `a` and `b` shrink by 90px between them
    // (weighted by their bases); `c` has no basis to shrink by, and keeps its
    // 30px content.
    assert_eq!(rect(&engine, &layout, "c").height, 30.0);
    let (a, b) = (rect(&engine, &layout, "a"), rect(&engine, &layout, "b"));
    assert!(a.height < 80.0 && b.height < 80.0, "{a:?} {b:?}");
}

#[test]
fn align_items_end_puts_a_column_item_at_the_far_edge() {
    let (engine, layout) = layout(
        r#"<div style="display: flex; flex-direction: column; align-items: flex-end; width: 400px">
             <div id="a" style="width: 100px; height: 10px"></div>
           </div>"#,
    );
    assert_eq!(rect(&engine, &layout, "a").x, 300.0);
}

#[test]
fn a_stretched_column_item_still_fills_the_width() {
    let (engine, layout) = layout(
        r#"<div style="display: flex; flex-direction: column; width: 400px">
             <div id="a" style="height: 10px"></div>
           </div>"#,
    );
    assert_eq!(rect(&engine, &layout, "a").width, 400.0);
}
