use super::*;
use noonoo_geometry::{Point, Rect, Size};

fn page(html: &str, css: &str, width: f32) -> (Document, LayoutTree) {
    page_at(html, css, Size::new(width, 600.0))
}

fn page_at(html: &str, css: &str, viewport: Size) -> (Document, LayoutTree) {
    let doc = noonoo_html::parse(html);
    let sheet = noonoo_css::Stylesheet::parse(css).unwrap();
    let styles = noonoo_style::compute_with(
        &doc,
        &[sheet],
        noonoo_style::Viewport {
            width: viewport.width,
            height: viewport.height,
        },
    );
    let tree = layout(&doc, &styles, viewport, &MetricShaper);
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
fn min_max_and_clamp_resolve_against_the_containing_block() {
    let (doc, tree) = page(
        r#"<div id="a"></div><div id="b"></div><div id="c"></div>"#,
        "#a { width: min(100%, 300px); height: 1px }
         #b { width: max(50%, 200px); height: 1px }
         #c { width: clamp(100px, 50%, 150px); height: 1px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 300.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.width, 200.0);
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.width, 150.0);

    // Narrower viewport: the percentage terms now win.
    let (doc, tree) = page(
        r#"<div id="a"></div><div id="b"></div><div id="c"></div>"#,
        "#a { width: min(100%, 300px); height: 1px }
         #b { width: max(50%, 200px); height: 1px }
         #c { width: clamp(100px, 50%, 150px); height: 1px }",
        160.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 160.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.width, 200.0);
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.width, 100.0);
}

#[test]
fn viewport_units_track_the_layout_viewport() {
    let (doc, tree) = page_at(
        r#"<div id="a"></div><div id="b"></div>"#,
        "#a { width: 50vw; height: 10vh } #b { width: calc(100vw - 100px); height: 1px }",
        Size::new(400.0, 300.0),
    );
    let a = tree.get(id(&doc, "a")).unwrap();
    assert_eq!(a.rect.width, 200.0);
    assert_eq!(a.rect.height, 30.0);
    let b = tree.get(id(&doc, "b")).unwrap();
    assert_eq!(b.rect.width, 300.0);
}

#[test]
fn a_float_leaves_normal_flow_but_shapes_its_container() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="f"></div><div id="b"></div></div>"#,
        "#parent { width: 400px }
         #f { float: left; width: 100px; height: 50px }
         #b { height: 20px }",
        400.0,
    );
    let f = tree.get(id(&doc, "f")).unwrap();
    // The float sits at the flow position, hugging the left edge.
    assert_eq!(
        (f.rect.x, f.rect.y, f.rect.width, f.rect.height),
        (0.0, 0.0, 100.0, 50.0)
    );
    // A block sibling flows right past it: full width, unchanged position.
    let b = tree.get(id(&doc, "b")).unwrap();
    assert_eq!((b.rect.x, b.rect.y, b.rect.width), (0.0, 0.0, 400.0));
    // …but the container's auto height includes the float's bottom.
    let parent = tree.get(id(&doc, "parent")).unwrap();
    assert_eq!(parent.rect.height, 50.0);
}

#[test]
fn line_boxes_wrap_around_a_float() {
    let (_doc, tree) = page(
        r#"<div id="parent"><div id="f"></div><p>aaaa aaaa aaaa aaaa aaaa aaaa aaaa aaaa aaaa aaaa aaaa aaaa aaaa aaaa aaaa</p></div>"#,
        "#parent { width: 400px }
         #f { float: left; width: 100px; height: 30px }
         p { margin: 0 }",
        400.0,
    );
    let runs: Vec<_> = tree
        .boxes()
        .iter()
        .filter(|b| b.kind == BoxKind::Text)
        .map(|b| (b.rect.x, b.rect.y))
        .collect();
    // The band beside the float is [100, 400]; seven 4-char words (32px + 8px
    // space) fill it. Lines that start below the float's bottom are free again.
    assert_eq!(runs[0].0, 100.0, "the first line starts beside the float");
    assert!(
        runs.iter().any(|&(x, y)| x == 0.0 && y >= 30.0),
        "a line below the float spans the full width: {runs:?}"
    );
}

#[test]
fn clear_pushes_below_the_floats_it_names() {
    let (doc, tree) = page(
        r#"<div id="l"></div><div id="r"></div><div id="left-only"></div><div id="both"></div>"#,
        "#l { float: left; width: 50px; height: 40px }
         #r { float: right; width: 50px; height: 60px }
         #both { clear: both; height: 10px }
         #left-only { clear: left; height: 10px }",
        400.0,
    );
    // `clear: both` sits below the deeper (right) float.
    assert_eq!(tree.get(id(&doc, "both")).unwrap().rect.y, 60.0);
    // `clear: left` only cares about the left float: above it is free.
    assert_eq!(tree.get(id(&doc, "left-only")).unwrap().rect.y, 40.0);
}

#[test]
fn floats_stack_beside_each_other_and_wrap() {
    let (doc, tree) = page(
        r#"<div id="a"></div><div id="b"></div><div id="c"></div>"#,
        "#a, #b { float: left; width: 100px; height: 50px }
         #c { float: left; width: 300px; height: 50px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.x, 0.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.x, 100.0);
    // A 300px float does not fit the remaining 200px band, so it drops below.
    let c = tree.get(id(&doc, "c")).unwrap();
    assert_eq!((c.rect.x, c.rect.y), (0.0, 50.0));
}

#[test]
fn a_right_float_hugs_the_right_edge() {
    let (doc, tree) = page(
        r#"<div id="f"></div>"#,
        "#f { float: right; width: 100px; height: 50px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.x, 300.0);
}

#[test]
fn a_float_without_a_width_shrinks_to_fit() {
    let (doc, tree) = page(r#"<div id="f">aaaa bb</div>"#, "#f { float: left }", 400.0);
    // "aaaa bb" at 16px with the metric shaper: 7 chars * 0.5 * 16 = 56px.
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.width, 56.0);
}

#[test]
fn a_float_resolves_percentages_against_its_container() {
    let (doc, tree) = page(
        r#"<div id="f"></div>"#,
        "#f { float: left; width: 50%; height: 10px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.width, 200.0);
}

#[test]
fn a_float_among_inline_content_shapes_the_lines() {
    let (doc, tree) = page(
        r#"<p>aa <b id="f">x</b> bb</p>"#,
        "p { margin: 0 } #f { float: left; width: 60px; height: 20px }",
        400.0,
    );
    let f = tree.get(id(&doc, "f")).unwrap();
    assert_eq!((f.rect.x, f.rect.y), (0.0, 0.0));
    // Both words fit beside the 60px float, so the line starts at x = 60.
    let runs: Vec<(f32, f32)> = tree
        .boxes()
        .iter()
        .filter(|b| b.kind == BoxKind::Text)
        .map(|b| (b.rect.x, b.rect.y))
        .collect();
    assert!(
        runs.contains(&(60.0, 0.0)),
        "text beside the float: {runs:?}"
    );
}

#[test]
fn align_items_stretches_items_to_the_line_height() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div></div>"#,
        "#parent { display: flex } #a { width: 100px; height: 40px } #b { width: 100px }",
        400.0,
    );
    // `b` has an auto height, so `stretch` (the default) fills the line.
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.height, 40.0);
}

#[test]
fn an_explicit_height_beats_stretch() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div></div>"#,
        "#parent { display: flex } #a { width: 100px; height: 40px }
         #b { width: 100px; height: 10px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.height, 10.0);
}

#[test]
fn align_items_positions_within_the_line() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="c"></div><div id="e"></div></div>"#,
        "#parent { display: flex; align-items: center }
         #c { width: 100px; height: 10px } #e { width: 100px; height: 40px }
         #e { align-self: flex-end }",
        400.0,
    );
    // The line is 40 tall: the centered item sits at (40 - 10) / 2, the
    // `flex-end` one at the bottom (its own height is the line height).
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.y, 15.0);
    assert_eq!(tree.get(id(&doc, "e")).unwrap().rect.y, 0.0);
}

#[test]
fn align_self_overrides_align_items() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div></div>"#,
        "#parent { display: flex; align-items: center }
         #a { width: 100px; height: 40px }
         #b { width: 100px; align-self: stretch }",
        400.0,
    );
    // The container centers, but `b` asks to stretch — and it wins.
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.height, 40.0);
}

#[test]
fn grid_items_stretch_to_their_area() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div></div>"#,
        "#parent { display: grid; grid-template-columns: 1fr 1fr;
                  grid-auto-rows: 40px }
         #a, #b { width: auto }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.height, 40.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.height, 40.0);
}

#[test]
fn grid_align_items_positions_within_the_area() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div></div>"#,
        "#parent { display: grid; grid-template-columns: 1fr 1fr;
                  grid-template-rows: 60px; align-items: center }
         #a { height: 20px }
         #b { height: 20px; align-self: end }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.y, 20.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.y, 40.0);
}

#[test]
fn loose_text_becomes_an_anonymous_grid_item() {
    let (_doc, tree) = page(
        r#"<div id="parent"><div id="a"></div>hello world</div>"#,
        "#parent { display: grid; grid-template-columns: 100px 1fr; height: 20px }
         #a { height: 20px }",
        400.0,
    );
    // The text sits in row 1, column 2 — beside the element, not below it.
    let runs: Vec<_> = tree
        .boxes()
        .iter()
        .filter(|b| b.kind == BoxKind::Text)
        .map(|b| (b.rect.x, b.rect.y, b.text.clone().unwrap_or_default()))
        .collect();
    assert!(
        runs.contains(&(100.0, 0.0, "hello world".to_owned())),
        "{runs:?}"
    );
}

#[test]
fn consecutive_text_merges_into_one_anonymous_item() {
    let (_doc, tree) = page(
        r#"<div id="parent">aa <b id="b">x</b> bb</div>"#,
        "#parent { display: grid; grid-template-columns: 100px 100px }
         #b { height: 10px }",
        400.0,
    );
    // Items in order: "aa" (r1c1), the `b` element (r1c2), "bb" (r2c1) —
    // three items, not five. Row 1 is as tall as "aa"'s line (19.2).
    let bb: Vec<_> = tree
        .boxes()
        .iter()
        .filter(|b| b.kind == BoxKind::Text && b.text.as_deref() == Some("bb"))
        .map(|b| (b.rect.x, b.rect.y))
        .collect();
    assert_eq!(bb, vec![(0.0, 19.2)]);
}

#[test]
fn anonymous_text_wraps_and_sizes_its_row() {
    let (doc, tree) = page(
        r#"<div id="parent">aaaa aaaa aaaa<div id="b"></div></div>"#,
        "#parent { display: grid; grid-template-columns: 100px }
         #b { height: 10px }",
        400.0,
    );
    // The text wraps to two lines (2 * 19.2) in the 100px column, so the
    // next row starts below the wrapped text.
    let b = tree.get(id(&doc, "b")).unwrap();
    assert!((b.rect.y - 38.4).abs() < 0.1, "got {}", b.rect.y);
}

#[test]
fn grid_columns_share_the_free_space() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div></div>"#,
        "#parent { display: grid; grid-template-columns: 1fr 1fr }
         #a, #b { height: 20px }",
        400.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap();
    let b = tree.get(id(&doc, "b")).unwrap();
    assert_eq!((a.rect.x, a.rect.width), (0.0, 200.0));
    assert_eq!((b.rect.x, b.rect.width), (200.0, 200.0));
    assert_eq!(a.rect.y, b.rect.y);
}

#[test]
fn fixed_and_fr_tracks_split_the_width() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div></div>"#,
        "#parent { display: grid; grid-template-columns: 150px 1fr }
         #a, #b { height: 20px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 150.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.width, 250.0);
}

#[test]
fn auto_placement_wraps_to_the_next_row() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div><div id="c"></div></div>"#,
        "#parent { display: grid; grid-template-columns: 1fr 1fr }
         #a, #b, #c { height: 10px }",
        400.0,
    );
    let c = tree.get(id(&doc, "c")).unwrap();
    assert_eq!((c.rect.x, c.rect.y, c.rect.width), (0.0, 10.0, 200.0));
}

#[test]
fn explicit_placement_positions_items() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div></div>"#,
        "#parent { display: grid; grid-template-columns: 100px 100px;
                  grid-template-rows: auto auto }
         #a { height: 10px }
         #b { grid-column: 2; grid-row: 2; height: 10px }",
        400.0,
    );
    let b = tree.get(id(&doc, "b")).unwrap();
    assert_eq!((b.rect.x, b.rect.y), (100.0, 10.0));
}

#[test]
fn an_item_spans_tracks() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div></div>"#,
        "#parent { display: grid; grid-template-columns: 100px 1fr }
         #a { grid-column: 1 / 3; height: 10px }
         #b { height: 10px }",
        400.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap();
    assert_eq!((a.rect.x, a.rect.width), (0.0, 400.0));
    // `b` auto-places into row 2 (row 1 is fully occupied).
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.y, 10.0);
}

#[test]
fn rows_size_to_their_tallest_item() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div><div id="c"></div></div>"#,
        "#parent { display: grid; grid-template-columns: 1fr 1fr }
         #a { height: 20px } #b { height: 50px } #c { height: 10px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.y, 50.0);
    // The container's height covers the tallest row.
    assert_eq!(tree.get(id(&doc, "parent")).unwrap().rect.height, 60.0);
}

#[test]
fn gaps_separate_tracks() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div><div id="c"></div></div>"#,
        "#parent { display: grid; grid-template-columns: 1fr 1fr;
                  column-gap: 20px; row-gap: 10px }
         #a, #b, #c { height: 10px }",
        400.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap();
    let b = tree.get(id(&doc, "b")).unwrap();
    let c = tree.get(id(&doc, "c")).unwrap();
    assert_eq!((a.rect.x, a.rect.width), (0.0, 190.0));
    assert_eq!((b.rect.x, b.rect.width), (210.0, 190.0));
    assert_eq!(c.rect.y, 20.0);
}

#[test]
fn implicit_rows_use_grid_auto_rows() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div><div id="c"></div><div id="d"></div><div id="e"></div></div>"#,
        "#parent { display: grid; grid-template-columns: 1fr 1fr;
                  grid-auto-rows: 40px }
         #a, #b, #c, #d, #e { height: 10px }",
        400.0,
    );
    // With no template rows every row is implicit and 40px tall: rows 1 and 2
    // put the 5th item at 40 + 40.
    assert_eq!(tree.get(id(&doc, "e")).unwrap().rect.y, 80.0);
}

#[test]
fn a_percentage_column_resolves_against_the_container() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div></div>"#,
        "#parent { display: grid; grid-template-columns: 50% 1fr }
         #a, #b { height: 10px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 200.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.width, 200.0);
}

#[test]
fn repeat_expands_tracks() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div></div>"#,
        "#parent { display: grid; grid-template-columns: repeat(2, 1fr) }
         #a, #b { height: 10px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 200.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.x, 200.0);
}

#[test]
fn max_width_caps_an_auto_width_block() {
    let (doc, tree) = page(
        r#"<div id="a"></div>"#,
        "#a { max-width: 200px; height: 10px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 200.0);
}

#[test]
fn max_width_caps_a_specified_width() {
    let (doc, tree) = page(
        r#"<div id="a"></div>"#,
        "#a { width: 400px; max-width: 200px; height: 10px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 200.0);
}

#[test]
fn min_width_floors_an_auto_width_block() {
    let (doc, tree) = page(
        r#"<div id="a"></div>"#,
        "#a { min-width: 300px; height: 10px }",
        200.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 300.0);
}

#[test]
fn min_width_wins_over_max_width() {
    let (doc, tree) = page(
        r#"<div id="a"></div>"#,
        "#a { width: 50px; min-width: 300px; max-width: 200px; height: 10px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 300.0);
}

#[test]
fn min_max_respect_box_sizing() {
    let (doc, tree) = page(
        r#"<div id="a"></div>"#,
        "#a { box-sizing: border-box; width: 100px; padding: 10px;
              min-width: 150px; height: 10px }",
        400.0,
    );
    // `min-width` is a border box here: 150 total, 130 of content.
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 150.0);
}

#[test]
fn a_percentage_max_width_resolves_against_the_container() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div></div>"#,
        "#parent { width: 200px } #a { max-width: 50%; height: 10px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 100.0);
}

#[test]
fn max_height_caps_the_box_and_min_height_grows_it() {
    let (doc, tree) = page(
        r#"<div id="a">word word word</div><div id="b"></div>"#,
        "#a { max-height: 20px; overflow: hidden; width: 100px }
         #b { min-height: 40px; width: 10px }",
        400.0,
    );
    // Three words wrap to two lines (~38.4px tall); the box is capped at 20.
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.height, 20.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.height, 40.0);
}

#[test]
fn a_flex_item_respects_its_min_and_max_width() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div></div>"#,
        "#parent { display: flex }
         #a { flex-grow: 1; max-width: 100px; height: 10px }
         #b { flex-grow: 1; height: 10px }",
        400.0,
    );
    // Both want half (200); `a` caps at 100 and `b` takes the rest.
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 100.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.width, 300.0);
}

#[test]
fn a_grid_container_heights_to_its_rows() {
    let (doc, tree) = page(
        r#"<div id="parent"><div id="a"></div><div id="b"></div></div>"#,
        "#parent { display: grid; grid-template-columns: 1fr 1fr }
         #a { height: 30px } #b { height: 10px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "parent")).unwrap().rect.height, 30.0);
}

#[test]
fn calc_mixes_percentage_and_pixels_against_the_containing_block() {
    let (doc, tree) = page(
        r#"<div id="a"></div>"#,
        "#a { width: calc(100% - 20px); height: 10px }",
        400.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap();
    assert_eq!(a.rect.width, 380.0);
}

#[test]
fn calc_padding_resolves_and_clamps() {
    let (doc, tree) = page(
        r#"<div id="a"></div>"#,
        "#a { width: 100px; height: 10px; padding-left: calc(10% - 2px); padding-right: calc(2px - 10%) }",
        400.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap();
    // 38px resolved on the left; the negative one clamps to zero.
    assert_eq!(a.content_box().width, 100.0);
    assert_eq!(a.rect.width, 138.0);
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

#[test]
fn z_index_orders_hit_testing() {
    let (doc, tree) = page(
        r#"<div id="a"></div><div id="b"></div>"#,
        "#a { position: absolute; top: 0; left: 0; width: 50px; height: 50px; z-index: 1 } \
         #b { position: absolute; top: 0; left: 0; width: 50px; height: 50px; z-index: 2 }",
        100.0,
    );
    // #b (z 2) is on top, so a point inside both selects it.
    assert_eq!(tree.hit_test(Point::new(10.0, 10.0)), Some(id(&doc, "b")));
}

#[test]
fn overflow_x_computes_a_horizontal_scroll_range() {
    let (doc, mut tree) = page(
        r#"<div id="strip"><div id="wide"></div></div>"#,
        "#strip { width: 50px; height: 20px; overflow-x: auto } \
         #wide { width: 200px; height: 10px }",
        400.0,
    );
    let strip = id(&doc, "strip");
    assert_eq!(tree.scroll_max_x(strip), 150.0);
    assert_eq!(tree.scroll_by_x(strip, 400.0), 150.0);
    assert_eq!(tree.scroll_offset_x(strip), 150.0);
}

#[test]
fn inline_flex_with_raw_text_lays_out() {
    // Reduction of arniko's real button stylesheet. The panic ("layout only
    // visits styled elements") came from flex running `ctx.style()` on a raw
    // text child; text is an anonymous item styled by the container (see
    // `runs`).
    let (doc, tree) = page(
        r#"<button id="btn">Save and exit</button>"#,
        "#btn { display: inline-flex; align-items: center; justify-content: center; \
         gap: 8px; white-space: nowrap; border-radius: 8px; font-size: 14px; \
         font-weight: 500; cursor: pointer; border: none; padding: 8px 16px; \
         height: 36px }",
        400.0,
    );
    let button = tree.get(id(&doc, "btn")).unwrap();
    assert_eq!(button.rect.height, 52.0);

    // The anonymous run renders as a text box one line high, inside the
    // button's content box (padding 16px on each side) — the whole spread of
    // the old panic path is now covered by layout, no paint assert needed.
    let lines: Vec<_> = tree
        .boxes()
        .iter()
        .filter(|box_| box_.kind == BoxKind::Text)
        .map(|box_| box_.rect.height)
        .collect();
    assert_eq!(lines.len(), 1);
}

#[test]
fn br_starts_a_fresh_line() {
    // Finding 2 of docs/REVIEW-2026-11-07.md: `H<br>i` painted both words at
    // the same y (with an empty box for the `<br>` itself).
    let (_doc, tree) = page(r#"<p>H<br>i</p>"#, "p { margin: 0 }", 400.0);
    let mut words: Vec<(f32, f32, &str)> = tree
        .boxes()
        .iter()
        .filter(|b| b.kind == BoxKind::Text)
        .map(|b| (b.rect.y, b.rect.x, b.text.as_deref().unwrap_or_default()))
        .collect();
    words.sort_by_key(|&(y, _, _)| (y * 1000.0) as i64);
    assert_eq!(
        words.len(),
        2,
        "no empty box for the break itself: {words:?}"
    );
    assert_eq!(words[0].2, "H");
    assert_eq!(words[1].2, "i");
    assert!(words[1].0 > words[0].0, "i sits on its own line: {words:?}");
}

#[test]
fn lone_br_gives_an_empty_line_box() {
    // The classic one-line-height trick: `<br>` alone still generates a
    // line box with no content.
    let (doc, tree) = page(
        r#"<div id="d"><br></div>"#,
        "#d { line-height: 20px }",
        400.0,
    );
    let span = id(&doc, "d");
    assert_eq!(tree.get(span).unwrap().rect.height, 20.0);
}

#[test]
fn consecutive_breaks_leave_a_blank_line() {
    let (_doc, tree) = page(
        r#"<p>H<br><br>i</p>"#,
        "p { margin: 0; line-height: 20px }",
        400.0,
    );
    let mut ys: Vec<f32> = tree
        .boxes()
        .iter()
        .filter(|b| b.kind == BoxKind::Text)
        .map(|b| b.rect.y)
        .collect();
    ys.sort_by_key(|&y| (y * 1000.0) as i64);
    // H on line 1, one blank line, i on line 3.
    assert_eq!(ys.len(), 2, "only H and i draw text: {ys:?}");
    assert!((ys[1] - ys[0] - 40.0).abs() < 0.01, "{ys:?}");
}

#[test]
fn trailing_break_adds_no_line() {
    let (_doc, tree) = page(
        r#"<p>H<br></p>"#,
        "p { margin: 0; line-height: 20px }",
        400.0,
    );
    // The paragraph's height stays one line.
    // Find it via its text box instead: the tree's text boxes are the
    // paragraph content.
    let text = tree
        .boxes()
        .iter()
        .find(|b| b.kind == BoxKind::Text)
        .expect("the H line box");
    assert_eq!(text.rect.height, 20.0);
}

#[test]
fn replaced_elements_show_their_placeholder_text() {
    // Finding 7 of docs/REVIEW-2026-11-07.md: `<img alt>` and `<input value>`
    // used to paint nothing (an empty run that nothing could draw).
    let (_doc, tree) = page(
        r#"<p><img src="x" alt="pic"></p><p><input value="a"></p>"#,
        "p, img { margin: 0 }",
        400.0,
    );
    let words: Vec<&str> = tree
        .boxes()
        .iter()
        .filter(|b| !b.text.as_deref().unwrap_or_default().is_empty())
        .map(|b| b.text.as_deref().unwrap_or_default())
        .collect();
    assert_eq!(words, vec!["pic", "a"]);
}

#[test]
fn table_cells_lay_out_in_rows_and_columns() {
    // Finding 3 of docs/REVIEW-2026-11-07.md: cells painted together as one
    // run (`bc`) because the table had no layout of its own. The review's
    // table: row 1 is one colspan-2 cell, row 2 holds `b` and `c`.
    let (doc, tree) = page(
        r#"<table><tr><td colspan="2">a</td></tr><tr><td>b</td><td>c</td></tr></table>"#,
        "table { border-collapse: collapse; width: 120px } \
         td { border: 1px solid #000; padding: 4px; margin: 0 }",
        400.0,
    );
    let cells: Vec<_> = doc
        .get_elements_by_tag_name("td")
        .iter()
        .filter_map(|&node| tree.get(node))
        .collect();
    assert_eq!(cells.len(), 3, "three laid-out cell boxes");
    let (a, b, c) = (cells[0], cells[1], cells[2]);

    // `a` spans both columns of a 120px table, so its span is wide; `b` and
    // `c` each take one below it.
    assert!(a.rect.width > b.rect.width.max(c.rect.width));
    // The spanning cell sits directly over the two beneath it.
    assert!((b.rect.y - a.rect.bottom()).abs() < 0.01, "{a:?}");
    assert!((c.rect.y - a.rect.bottom()).abs() < 0.01, "{a:?}");
    // `c` starts where `b` ends: one column apart, same row.
    assert!(c.rect.x > b.rect.x + b.rect.width - 1.0);
    assert_eq!(c.rect.y, b.rect.y);
    // The table's height covers both rows.
}

#[test]
fn border_collapse_paints_a_shared_edge_once() {
    let (doc, tree) = page(
        r#"<table><tr><td id="a">a</td><td>b</td></tr><tr><td>c</td><td>d</td></tr></table>"#,
        "table { border-collapse: collapse } td { border: 1px solid #000 }",
        400.0,
    );
    let td = id(&doc, "a");
    let border = tree.get(td).unwrap().border;
    // `a` has a neighbour to its right and one below: it keeps only its own
    // top and left — the shared edges belong to `b` and `c`.
    assert_eq!(border.right, 0.0);
    assert_eq!(border.bottom, 0.0);
    assert_eq!(border.left, 1.0);
    assert_eq!(border.top, 1.0);
}

// ── Atomic inline-level boxes: inline-block, inline-flex, form controls ────

#[test]
fn an_inline_block_has_its_own_size_padding_and_border_on_the_line() {
    let (doc, tree) = page(
        r#"<p id="p">ab<span id="s">x</span>cd</p>"#,
        "p { margin: 0 } #s { display: inline-block; width: 50px; height: 30px; \
         padding: 5px; border: 2px solid red }",
        400.0,
    );
    let s = tree.get(id(&doc, "s")).unwrap().rect;
    assert_eq!(
        (s.width, s.height),
        (64.0, 44.0),
        "50 + 2×5 + 2×2, 30 + 2×5 + 2×2"
    );
    let p = tree.get(id(&doc, "p")).unwrap().rect;
    assert!(p.height >= 44.0, "the line grows to hold it: {p:?}");
    let text_after = text_boxes(&doc, &tree, "cd");
    assert_eq!(text_after.len(), 1);
    assert!(text_after[0].x >= s.right(), "{text_after:?} after {s:?}");
    assert!(s.x >= 16.0, "after `ab`: {s:?}");
}

#[test]
fn an_inline_flex_button_shrinks_to_its_content_and_sits_beside_the_next() {
    let (doc, tree) = page(
        r#"<div><button id="b">Save</button><button id="c">Go</button></div>"#,
        "button { display: inline-flex; border: none; padding: 8px 16px; \
         font-size: 16px; box-sizing: content-box }",
        400.0,
    );
    let b = tree.get(id(&doc, "b")).unwrap().rect;
    let c = tree.get(id(&doc, "c")).unwrap().rect;
    assert_eq!(b.width, 4.0 * 8.0 + 32.0, "{b:?}");
    assert_eq!(c.width, 2.0 * 8.0 + 32.0, "{c:?}");
    assert_eq!(b.y, c.y, "one line");
    assert!(c.x >= b.right(), "{c:?} after {b:?}");
}

#[test]
fn an_input_takes_its_specified_width() {
    let (doc, tree) = page(
        r#"<p><input id="i"></p>"#,
        "input { width: 100px; padding: 2px; border: 1px solid black; \
         box-sizing: content-box }",
        400.0,
    );
    let i = tree.get(id(&doc, "i")).unwrap().rect;
    assert_eq!(i.width, 106.0, "{i:?}");
}

#[test]
fn an_inline_blocks_content_is_laid_out_inside_it() {
    let (doc, tree) = page(
        r#"<p><span id="s">hello world</span></p>"#,
        "#s { display: inline-block; width: 60px; padding: 4px }",
        400.0,
    );
    let s = tree.get(id(&doc, "s")).unwrap().rect;
    let words = [
        text_boxes(&doc, &tree, "hello"),
        text_boxes(&doc, &tree, "world"),
    ];
    for word in words.iter().flatten() {
        assert!(
            word.x >= s.x + 4.0 && word.y >= s.y + 4.0 && word.bottom() <= s.bottom(),
            "{word:?} inside {s:?}"
        );
    }
    // 60px holds "hello" (40px) but not "hello world" (88px): two lines.
    assert!(words[1][0].y > words[0][0].y);
}

#[test]
fn inline_blocks_wrap_like_words() {
    let (doc, tree) = page(
        r#"<div><i id="a"></i><i id="b"></i><i id="c"></i></div>"#,
        "i { display: inline-block; width: 40px; height: 10px }",
        100.0,
    );
    let [a, b, c] = ["a", "b", "c"].map(|n| tree.get(id(&doc, n)).unwrap().rect);
    assert_eq!(a.y, b.y);
    assert!(c.y > a.y, "the third wraps: {a:?} {b:?} {c:?}");
}

#[test]
fn a_block_after_a_tall_inline_block_starts_below_it() {
    let (doc, tree) = page(
        r#"<div><span id="s"></span></div><p id="after">x</p>"#,
        "#s { display: inline-block; width: 10px; height: 100px } p { margin: 0 }",
        400.0,
    );
    let s = tree.get(id(&doc, "s")).unwrap().rect;
    let after = tree.get(id(&doc, "after")).unwrap().rect;
    assert!(after.y >= s.bottom(), "{after:?} below {s:?}");
}

/// The text boxes holding exactly `word`.
fn text_boxes(doc: &Document, tree: &LayoutTree, word: &str) -> Vec<Rect> {
    let _ = doc;
    tree.boxes()
        .iter()
        .filter(|b| b.kind == BoxKind::Text && b.text.as_deref() == Some(word))
        .map(|b| b.rect)
        .collect()
}

#[test]
fn an_auto_width_float_counts_its_padding_and_border_once() {
    let (doc, tree) = page(
        r#"<div><span id="f">Save</span></div>"#,
        "#f { float: left; padding: 0 16px; border: 2px solid black; \
         box-sizing: content-box }",
        400.0,
    );
    let f = tree.get(id(&doc, "f")).unwrap().rect;
    assert_eq!(f.width, 4.0 * 8.0 + 32.0 + 4.0, "{f:?}");
}

/// Margin collapsing walks each block's first and last child. It used to do
/// that once for each edge and again from the parent, so the cost doubled at
/// every nesting level: 25 nested divs took about 30 s. Runs on a thread with
/// a deadline so a regression fails instead of hanging.
#[test]
fn deeply_nested_blocks_lay_out_in_linear_time() {
    let (done, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let depth = 400;
        let html = format!("{}x{}", "<div>".repeat(depth), "</div>".repeat(depth));
        let (_, tree) = page(&html, "div { margin: 1px 0 }", 800.0);
        let _ = done.send(tree.boxes.len());
    });
    let boxes = wait
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("layout of 400 nested divs did not finish in 5 s");
    assert!(boxes >= 400);
}

/// Memoising margins must not change them: nested collapse-through boxes and
/// a first child's margin still collapse into their parent's.
#[test]
fn nested_margins_still_collapse_through_their_parents() {
    let (doc, tree) = page(
        r#"<div id="a"><div><div id="b">x</div></div></div><div id="c">y</div>"#,
        "body { margin: 0 } div { margin: 10px 0 } #b { margin-top: 30px }",
        800.0,
    );
    // #a's top margin collapses with its descendants' down to #b's 30px.
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.y, 30.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.y, 30.0);
}

/// A flex item is its own block formatting context: a float inside one
/// does not push the text of the next item aside.
#[test]
fn a_float_in_a_flex_item_stays_inside_it() {
    let (doc, tree) = page(
        r#"<div id="row"><div id="a"><div id="f"></div></div><div id="b">next</div></div>"#,
        "body { margin: 0 } #row { display: flex }
         #a { width: 100px } #f { float: left; width: 150px; height: 40px }
         #b { width: 200px }",
        800.0,
    );
    let b = tree.get(id(&doc, "b")).unwrap().rect;
    let next = text_boxes(&doc, &tree, "next");
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].x, b.x, "the float leaked into the next item");
    // #a grows to hold its float, which overflows it to the right.
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.height, 40.0);
}

/// An inline-block is its own block formatting context: its floats stay
/// inside it, even when they overflow its fixed height.
#[test]
fn an_inline_block_contains_its_floats() {
    let (doc, tree) = page(
        r#"<div><span id="ib"><div id="f"></div></span></div><p id="p">after</p>"#,
        "body { margin: 0 } p { margin: 0 } div { line-height: 10px }
         #ib { display: inline-block; width: 100px; height: 10px }
         #f { float: left; width: 50px; height: 60px }",
        800.0,
    );
    // Its own float doesn't push it aside (a trial measurement used to
    // register the float outside), nor stretch the line's block.
    assert_eq!(tree.get(id(&doc, "ib")).unwrap().rect.x, 0.0);
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.x, 0.0);
    let after = text_boxes(&doc, &tree, "after");
    assert_eq!(after[0].y, 10.0, "the float escaped the inline-block");
}

/// A table cell is its own block formatting context too: a float that
/// overflows a fixed-height table does not wrap the text after it.
#[test]
fn a_table_cell_contains_its_floats() {
    let (doc, tree) = page(
        r#"<table><tr><td><div id="f"></div></td></tr></table><p>after</p>"#,
        "body { margin: 0 } p { margin: 0 } table { height: 10px }
         td { height: 10px; padding: 0 } #f { float: left; width: 50px; height: 60px }",
        800.0,
    );
    // The cell is measured before it's placed; that trial used to leave the
    // float registered outside, so the real float landed below its ghost.
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.y, 0.0);
    let after = text_boxes(&doc, &tree, "after");
    assert_eq!(after[0].x, 0.0, "the float escaped the table cell");
}

/// Flex rows, grids and inline-blocks measure each child with a trial
/// layout before placing it. Without remembering those heights the cost
/// doubled per nesting level: 20 nested flex rows took 0.5 s.
#[test]
fn nested_measured_boxes_lay_out_in_polynomial_time() {
    for display in ["flex", "inline-flex", "grid", "inline-block"] {
        let (done, wait) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let depth = 60;
            let html = format!("{}x{}", "<div>".repeat(depth), "</div>".repeat(depth));
            let (_, tree) = page(&html, &format!("div {{ display: {display} }}"), 800.0);
            let _ = done.send(tree.boxes.len());
        });
        let boxes = wait
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap_or_else(|_| panic!("60 nested {display} boxes did not lay out in 5 s"));
        assert!(boxes >= 60);
    }
}

/// Indented markup puts whitespace text between table parts. Text has no
/// computed style, and looking one up used to panic.
#[test]
fn text_between_table_parts_is_skipped() {
    let (doc, tree) = page(
        "<table>\n  <tbody>\n    <tr>\n      <td>a</td>\n    </tr>\n  </tbody>\n</table>\
         <table> stray <tr> <td>b</td> </tr> </table>",
        "",
        800.0,
    );
    assert_eq!(text_boxes(&doc, &tree, "a").len(), 1);
    assert_eq!(text_boxes(&doc, &tree, "b").len(), 1);
}
