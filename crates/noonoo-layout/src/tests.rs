use super::*;
use noonoo_geometry::{Point, Rect, Size};

fn page(html: &str, css: &str, width: f32) -> (Document, LayoutTree) {
    let doc = noonoo_html::parse(html);
    let sheet = noonoo_css::Stylesheet::parse(css).unwrap();
    let styles = noonoo_style::compute(&doc, &[sheet]);
    let tree = layout(&doc, &styles, Size::new(width, 600.0), &MetricShaper);
    (doc, tree)
}

fn id(doc: &Document, name: &str) -> NodeId {
    doc.get_element_by_id(name).unwrap()
}

#[test]
fn blocks_stack_vertically_and_fill_width() {
    let (doc, tree) = page(
        r#"<div id="a"></div><div id="b"></div>"#,
        "div { height: 50px }",
        400.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap();
    let b = tree.get(id(&doc, "b")).unwrap();
    assert_eq!(a.rect.width, 400.0);
    assert_eq!(a.rect.y, 0.0);
    assert_eq!(b.rect.y, 50.0);
}

#[test]
fn padding_and_border_use_box_sizing() {
    let (doc, tree) = page(
        r#"<div id="a"></div>"#,
        "#a { width: 100px; padding: 10px; border: 2px solid black }",
        400.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap();
    // content-box: 100 content + 2*10 padding + 2*2 border
    assert_eq!(a.rect.width, 124.0);
    assert_eq!(a.content_box().width, 100.0);
}

#[test]
fn text_wraps_to_multiple_lines() {
    let (doc, tree) = page(
        r#"<div id="a">one two three four five six seven eight nine ten</div>"#,
        "#a { font-size: 20px }",
        60.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap();
    // More than a single 24px line.
    assert!(a.rect.height > 24.0, "height = {}", a.rect.height);
}

#[test]
fn inline_elements_get_boxes() {
    let (doc, tree) = page(
        r#"<div id="a">hi <span id="s">there</span></div>"#,
        "",
        400.0,
    );
    let span = tree.get(id(&doc, "s")).expect("inline box");
    assert_eq!(span.kind, BoxKind::Inline);
    assert!(span.rect.width > 0.0);
}

#[test]
fn flex_row_grows_children() {
    let (doc, tree) = page(
        r#"<div id="f"><div id="a"></div><div id="b"></div></div>"#,
        "#f { display: flex } #a { flex-grow: 1; height: 20px } #b { flex-grow: 3; height: 20px }",
        400.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap();
    let b = tree.get(id(&doc, "b")).unwrap();
    assert!((b.rect.width - a.rect.width).abs() > 100.0);
    assert!((a.rect.width + b.rect.width - 400.0).abs() < 1.0);
}

#[test]
fn flex_justify_centers() {
    let (doc, tree) = page(
        r#"<div id="f"><div id="a"></div></div>"#,
        "#f { display: flex; justify-content: center } #a { width: 100px; height: 10px }",
        400.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap();
    assert_eq!(a.rect.x, 150.0);
}

#[test]
fn flex_row_wraps() {
    let (doc, tree) = page(
        r#"<div id="f"><div id="a"></div><div id="b"></div><div id="c"></div></div>"#,
        "#f { display: flex; flex-wrap: wrap } #a, #b, #c { width: 80px; height: 20px }",
        100.0,
    );
    // 100px wide, 80px items: one per line.
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.y, 0.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.y, 20.0);
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.y, 40.0);
}

#[test]
fn text_align_centers_the_text() {
    let (doc, tree) = page(
        r#"<div id="a">hi</div>"#,
        "#a { text-align: center; font-size: 10px }",
        100.0,
    );
    let text_box = tree
        .boxes()
        .iter()
        .find(|b| b.kind == BoxKind::Text)
        .expect("a text box");
    // "hi" is 10px wide in a 100px line, centered → starts at 45.
    assert!(
        (text_box.rect.x - 45.0).abs() < 0.001,
        "x = {}",
        text_box.rect.x
    );
    let _ = doc;
}

#[test]
fn relative_offsets_shift_only_the_box() {
    let (doc, tree) = page(
        r#"<div id="a"></div><div id="b"></div><div id="c"></div>"#,
        "div { height: 50px } #b { position: relative; top: 10px; left: 5px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.y, 0.0);
    // The box moved...
    let b = tree.get(id(&doc, "b")).unwrap();
    assert_eq!(b.rect.x, 5.0);
    assert_eq!(b.rect.y, 60.0);
    // ...but the flow did not: `c` still follows at the normal spot.
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.y, 100.0);
}

#[test]
fn absolute_is_out_of_flow_and_placed_by_its_insets() {
    let (doc, tree) = page(
        r#"<div id="cb"><div id="abs"></div><div id="after"></div></div>"#,
        "#cb { position: relative; width: 200px; height: 200px } \
         #abs { position: absolute; top: 20px; left: 30px; width: 50px; height: 10px } \
         #after { height: 40px }",
        400.0,
    );
    let abs = tree.get(id(&doc, "abs")).unwrap();
    assert_eq!(abs.rect, Rect::new(30.0, 20.0, 50.0, 10.0));
    // The in-flow sibling starts at the parent's content origin: the absolute
    // box pushed nothing.
    assert_eq!(tree.get(id(&doc, "after")).unwrap().rect.y, 0.0);
}

#[test]
fn absolute_does_not_extend_its_parent() {
    let (doc, tree) = page(
        r#"<div id="cb"><div id="abs"></div></div>"#,
        "#cb { position: relative; width: 100px } \
         #abs { position: absolute; top: 0; left: 0; width: 50px; height: 500px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "cb")).unwrap().rect.height, 0.0);
}

#[test]
fn absolute_without_a_positioned_ancestor_uses_the_viewport() {
    let (doc, tree) = page(
        r#"<div id="abs"></div>"#,
        "#abs { position: absolute; top: 5px; right: 10px; width: 40px; height: 10px }",
        400.0,
    );
    let abs = tree.get(id(&doc, "abs")).unwrap();
    // right: 10 in a 400-wide viewport, width 40 → x = 400 - 10 - 40.
    assert_eq!(abs.rect.x, 350.0);
    assert_eq!(abs.rect.y, 5.0);
}

#[test]
fn absolute_stretches_between_left_and_right() {
    let (doc, tree) = page(
        r#"<div id="cb"><div id="abs"></div></div>"#,
        "#cb { position: relative; width: 200px; height: 100px } \
         #abs { position: absolute; left: 10px; right: 10px; top: 0; height: 20px }",
        400.0,
    );
    let abs = tree.get(id(&doc, "abs")).unwrap();
    assert_eq!(abs.rect.x, 10.0);
    assert_eq!(abs.rect.width, 180.0);
    assert_eq!(abs.rect.height, 20.0);
}

#[test]
fn absolute_shrinks_to_fit_its_text() {
    let (doc, tree) = page(
        r#"<div id="cb"><span id="tip">hi</span></div>"#,
        "#cb { position: relative; width: 300px; height: 100px } \
         #tip { position: absolute; top: 0; left: 0 }",
        400.0,
    );
    let tip = tree.get(id(&doc, "tip")).unwrap();
    assert!(tip.rect.width > 0.0, "width = {}", tip.rect.width);
    assert!(
        tip.rect.width < 300.0,
        "should shrink, width = {}",
        tip.rect.width
    );
}

#[test]
fn fixed_positions_against_the_viewport_not_the_ancestor() {
    let (doc, tree) = page(
        r#"<div id="cb"><div id="fix"></div></div>"#,
        "#cb { position: relative; margin-left: 50px; width: 100px; height: 100px } \
         #fix { position: fixed; top: 0; left: 0; width: 10px; height: 10px }",
        400.0,
    );
    let fix = tree.get(id(&doc, "fix")).unwrap();
    assert_eq!(fix.rect.x, 0.0);
    assert_eq!(fix.rect.y, 0.0);
}

#[test]
fn absolute_uses_the_nearest_positioned_ancestors_padding_box() {
    let (doc, tree) = page(
        r#"<div id="outer"><div id="inner"><div id="abs"></div></div></div>"#,
        "#outer { position: relative; margin-left: 40px; width: 200px; height: 200px; \
                  border: 5px solid black } \
         #inner { margin-top: 20px; height: 50px } \
         #abs { position: absolute; left: 0; top: 0; width: 10px; height: 10px }",
        400.0,
    );
    let abs = tree.get(id(&doc, "abs")).unwrap();
    // #outer's padding box origin: margin 40 + border 5.
    assert_eq!(abs.rect.x, 45.0);
    assert_eq!(abs.rect.y, 5.0);
}

#[test]
fn a_positioned_child_is_hit_outside_its_parent() {
    let (doc, tree) = page(
        r#"<div id="cb"><div id="badge"></div></div>"#,
        "#cb { position: relative; width: 100px; height: 100px } \
         #badge { position: absolute; left: 90px; top: 90px; width: 30px; height: 30px }",
        400.0,
    );
    // The badge spans (90,90)-(120,120); (110,110) is inside it but outside the
    // 100×100 containing block.
    assert_eq!(
        tree.hit_test(Point::new(110.0, 110.0)),
        Some(id(&doc, "badge"))
    );
}

#[test]
fn adjacent_sibling_margins_collapse_to_the_larger() {
    let (doc, tree) = page(
        r#"<div id="a"></div><div id="b"></div>"#,
        "div { height: 10px } #a { margin-bottom: 20px } #b { margin-top: 30px }",
        400.0,
    );
    // 10 (a) + max(20, 30) — not 10 + 50.
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.y, 40.0);
}

#[test]
fn parent_and_first_child_top_margins_collapse() {
    let (doc, tree) = page(
        r#"<div id="outer"><div id="inner"></div></div>"#,
        "#outer { margin-top: 10px } #inner { margin-top: 20px; height: 30px }",
        400.0,
    );
    let outer = tree.get(id(&doc, "outer")).unwrap();
    let inner = tree.get(id(&doc, "inner")).unwrap();
    // max(10, 20): the child's margin collapses through the parent.
    assert_eq!(outer.rect.y, 20.0);
    assert_eq!(inner.rect.y, 20.0);
    assert_eq!(outer.rect.height, 30.0);
}

#[test]
fn a_top_border_stops_parent_child_collapse() {
    let (doc, tree) = page(
        r#"<div id="outer"><div id="inner"></div></div>"#,
        "#outer { border-top-width: 2px; border-top-style: solid } \
         #inner { margin-top: 20px; height: 30px }",
        400.0,
    );
    let outer = tree.get(id(&doc, "outer")).unwrap();
    let inner = tree.get(id(&doc, "inner")).unwrap();
    assert_eq!(outer.rect.y, 0.0);
    // The border separates them: 2px border + 20px margin.
    assert_eq!(inner.rect.y, 22.0);
    assert_eq!(outer.rect.height, 52.0);
}

#[test]
fn parent_and_last_child_bottom_margins_collapse() {
    let (doc, tree) = page(
        r#"<div id="outer"><div id="inner"></div></div><div id="after"></div>"#,
        "#inner { height: 10px; margin-bottom: 25px } #after { height: 10px }",
        400.0,
    );
    let outer = tree.get(id(&doc, "outer")).unwrap();
    // The child's bottom margin collapses out of the parent's height...
    assert_eq!(outer.rect.height, 10.0);
    // ...and reappears between the parent and the next sibling.
    assert_eq!(tree.get(id(&doc, "after")).unwrap().rect.y, 35.0);
}

#[test]
fn negative_margins_collapse_to_a_sum() {
    let (doc, tree) = page(
        r#"<div id="a"></div><div id="b"></div>"#,
        "div { height: 10px } #a { margin-bottom: 20px } #b { margin-top: -8px }",
        400.0,
    );
    // 20 + (-8) = 12.
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.y, 22.0);
}

#[test]
fn an_empty_block_collapses_through() {
    let (doc, tree) = page(
        r#"<div id="a"></div><div id="empty"></div><div id="b"></div>"#,
        "#a, #b { height: 10px } \
         #a { margin-bottom: 20px } \
         #empty { margin-top: 30px; margin-bottom: 40px } \
         #b { margin-top: 5px }",
        400.0,
    );
    // The empty block contributes only its (collapsed) margin — the largest of
    // 20, 30, 40, 5 — and no space of its own.
    assert_eq!(tree.get(id(&doc, "empty")).unwrap().rect.height, 0.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.y, 50.0);
}

#[test]
fn nested_empty_blocks_collapse_through() {
    let (doc, tree) = page(
        r#"<div id="outer"><div id="empty"></div></div><div id="after"></div>"#,
        "#empty { margin-top: 20px; margin-bottom: 30px } #after { height: 10px }",
        400.0,
    );
    // `outer` has no content but a collapsed-through child, so it collapses
    // through too.
    assert_eq!(tree.get(id(&doc, "outer")).unwrap().rect.height, 0.0);
    assert_eq!(tree.get(id(&doc, "after")).unwrap().rect.y, 30.0);
}

#[test]
fn a_border_makes_an_empty_block_occupy_space() {
    let (doc, tree) = page(
        r#"<div id="a"></div><div id="e"></div><div id="b"></div>"#,
        "#a, #b { height: 10px } \
         #e { margin-top: 20px; margin-bottom: 20px; \
              border-top-width: 1px; border-top-style: solid }",
        400.0,
    );
    let e = tree.get(id(&doc, "e")).unwrap();
    // The border pins the margins apart: the block is 1px tall and its two
    // margins no longer collapse together.
    assert_eq!(e.rect.height, 1.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.y, 51.0);
}

#[test]
fn overflow_hidden_clips_hit_testing() {
    let (doc, tree) = page(
        r#"<div id="clip"><div id="inner"></div></div>"#,
        "#clip { position: relative; width: 100px; height: 50px; overflow: hidden } \
         #inner { position: absolute; left: 200px; top: 0; width: 20px; height: 20px }",
        400.0,
    );
    // Inside the clip box, the clip box itself is hit (the inner sits outside).
    assert_eq!(
        tree.hit_test(Point::new(50.0, 10.0)),
        Some(id(&doc, "clip"))
    );
    // Outside the clip box, the overflowing inner box is clipped away.
    assert_eq!(tree.hit_test(Point::new(210.0, 10.0)), None);
}

#[test]
fn a_clipping_box_does_not_collapse_margins_with_its_child() {
    let (doc, tree) = page(
        r#"<div id="clip"><div id="inner"></div></div>"#,
        "#clip { overflow: hidden } #inner { margin-top: 20px; height: 10px }",
        400.0,
    );
    let clip = tree.get(id(&doc, "clip")).unwrap();
    // The clip box is a formatting context: the child's margin stays inside it.
    assert_eq!(clip.rect.y, 0.0);
    assert_eq!(tree.get(id(&doc, "inner")).unwrap().rect.y, 20.0);
    assert_eq!(clip.rect.height, 30.0);
}

#[test]
fn overflow_auto_computes_a_scroll_range() {
    let (doc, mut tree) = page(
        r#"<div id="list"><div id="tall"></div></div>"#,
        "#list { width: 100px; height: 50px; overflow-y: auto } #tall { height: 200px }",
        400.0,
    );
    let list = id(&doc, "list");
    assert_eq!(tree.scroll_max(list), 150.0);
    assert_eq!(tree.scroll_offset(list), 0.0);
    // Scrolling clamps at the range.
    assert_eq!(tree.scroll_by(list, 100.0), 100.0);
    assert_eq!(tree.scroll_by(list, 100.0), 50.0);
    assert_eq!(tree.scroll_offset(list), 150.0);
}

#[test]
fn scrolling_changes_which_row_is_hit() {
    let (doc, mut tree) = page(
        r#"<div id="list"><div id="a"></div><div id="b"></div></div>"#,
        "#list { width: 100px; height: 50px; overflow-y: auto } #a, #b { height: 50px }",
        400.0,
    );
    let list = id(&doc, "list");
    // #a spans y 0..50, #b y 50..100, in a 50px scrollport.
    assert_eq!(tree.hit_test(Point::new(10.0, 10.0)), Some(id(&doc, "a")));
    // Scroll down 50: #b moves under the point.
    tree.scroll_by(list, 50.0);
    assert_eq!(tree.hit_test(Point::new(10.0, 10.0)), Some(id(&doc, "b")));
}
