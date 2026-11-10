use super::*;
use noonoo_geometry::{Point, Rect, Size};

fn page(html: &str, css: &str, width: f32) -> (Document, LayoutTree) {
    page_at(html, css, Size::new(width, 600.0))
}

fn page_at(html: &str, css: &str, viewport: Size) -> (Document, LayoutTree) {
    let doc = noonoo_html::parse(html);
    // The parser gives every snippet a `<body>`: drop its UA margin, so a
    // test's boxes start at the origin. A test's own `body` rule wins.
    let sheet = noonoo_css::Stylesheet::parse(&format!("body {{ margin: 0 }} {css}")).unwrap();
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

/// [`page`] without the test reset: the UA sheet's `body` margin applies.
fn page_with_ua_body(html: &str, width: f32) -> (Document, LayoutTree) {
    let doc = noonoo_html::parse(html);
    let viewport = Size::new(width, 600.0);
    let styles = noonoo_style::compute_with(
        &doc,
        &[],
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
    // …and the container, no formatting context root, is as tall as its
    // in-flow content alone (CSS 2 §10.6.3).
    let parent = tree.get(id(&doc, "parent")).unwrap();
    assert_eq!(parent.rect.height, 20.0);
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
    // Outside the clip box, the overflowing inner box is clipped away: the
    // body beneath it is hit.
    assert_eq!(
        tree.hit_test(Point::new(210.0, 10.0)),
        Some(doc.get_elements_by_tag_name("body")[0])
    );
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
        r#"<table id="t"><tr><td id="a">a</td><td>b</td></tr><tr><td>c</td><td>d</td></tr></table>"#,
        "table { border-collapse: collapse } td { border: 1px solid #000 }",
        400.0,
    );
    let td = id(&doc, "a");
    // Each cell's box takes half of every line around it ...
    let border = tree.get(td).unwrap().border;
    assert_eq!(
        (border.top, border.right, border.bottom, border.left),
        (0.5, 0.5, 0.5, 0.5)
    );
    // ... and paints none of its own: the table paints each edge once,
    // 3 column lines x 2 rows and 3 row lines x 2 columns.
    assert_eq!(tree.collapsed_borders(td), Some(&[][..]));
    let segments = tree.collapsed_borders(id(&doc, "t")).unwrap();
    assert_eq!(segments.len(), 12);
    let table = tree.get(id(&doc, "t")).unwrap().rect;
    // The middle line sits on the cells' boundary, 1px wide across it.
    let a = tree.get(td).unwrap().rect;
    let middle = segments
        .iter()
        .find(|segment| segment.rect.height > 1.0 && segment.rect.x > 1.0)
        .unwrap();
    assert!(
        near(table.x + middle.rect.x + 0.5, a.right()),
        "{middle:?} {a:?}"
    );
    assert_eq!(middle.rect.width, 1.0);
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
        // Standards mode: quirks mode drops the strut from a line with no
        // text.
        r#"<!doctype html><div><span id="ib"><div id="f"></div></span></div><p id="p">after</p>"#,
        "body { margin: 0 } p { margin: 0 } div { line-height: 10px }
         #ib { display: inline-block; width: 100px; height: 10px }
         #f { float: left; width: 50px; height: 60px }",
        800.0,
    );
    // Its own float doesn't push it aside (a trial measurement used to
    // register the float outside), nor stretch the line's block.
    assert_eq!(tree.get(id(&doc, "ib")).unwrap().rect.x, 0.0);
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.x, 0.0);
    // The line: the inline-block's 10px above the baseline, plus the 10px
    // strut's 1.8px of descent below it (MetricShaper, 16px).
    let after = text_boxes(&doc, &tree, "after");
    assert!(
        (after[0].y - 11.8).abs() < 0.01,
        "the float escaped the inline-block: {}",
        after[0].y
    );
}

/// A table cell is its own block formatting context too: a float that
/// overflows a fixed-height table does not wrap the text after it.
#[test]
fn a_table_cell_contains_its_floats() {
    let (doc, tree) = page(
        r#"<table><tr><td><div id="f"></div></td></tr></table><p>after</p>"#,
        "table { border-spacing: 0 } body { margin: 0 } p { margin: 0 } table { height: 10px }
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

/// An empty text input is still one line tall, as in a browser; a
/// checkbox isn't a text field and gets no line.
#[test]
fn an_empty_text_input_is_one_line_tall() {
    let (doc, tree) = page(
        r#"<p><input id="i"><input id="e" type="email" value=""><input id="c" type="checkbox"></p>"#,
        "input { padding: 8px 12px; border: 1px solid black; line-height: 20px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "i")).unwrap().rect.height, 38.0);
    assert_eq!(tree.get(id(&doc, "e")).unwrap().rect.height, 38.0);
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.height, 18.0);
    assert!(
        tree.boxes()
            .iter()
            .all(|b| b.kind != BoxKind::Text || b.text.as_deref() != Some("")),
        "no empty text box"
    );
}

/// Each item's row position used to be a fresh sum over every row before
/// it, so many items spanning many rows was quadratic in the row count.
#[test]
fn a_grid_with_many_spanning_items_lays_out_quickly() {
    let (done, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let html = format!("<div>{}</div>", "<p>x</p>".repeat(1500));
        let (_, tree) = page(
            &html,
            "div { display: grid } p { margin: 0; grid-row: span 1000 }",
            800.0,
        );
        let _ = done.send(tree.boxes.len());
    });
    let boxes = wait
        .recv_timeout(std::time::Duration::from_secs(3))
        .expect("1,500 items spanning 1,000 rows each did not lay out in 3 s");
    assert!(boxes >= 1500);
}

/// Rows still stack in order with their gaps.
#[test]
fn grid_rows_start_after_the_rows_and_gaps_before_them() {
    let (doc, tree) = page(
        r#"<div><p id="a">x</p><p id="b">y</p><p id="c">z</p></div>"#,
        "body { margin: 0 } div { display: grid; row-gap: 5px; grid-auto-rows: 20px }
         p { margin: 0 } #b { grid-row: span 2 }",
        800.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.y, 0.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.y, 25.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.height, 45.0);
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.y, 75.0);
}

/// The grid is clamped to `grid::MAX_TRACKS` per axis, as Firefox does:
/// an item past the limit is placed in the last tracks (the spec lets an
/// implementation clamp the grid's size).
#[test]
fn the_implicit_grid_is_clamped() {
    let (doc, tree) = page(
        r#"<div id="g"><p id="a">x</p><p id="b">y</p></div>"#,
        "body { margin: 0 } div { display: grid; grid-auto-rows: 1px }
         p { margin: 0; grid-row: span 9000 }",
        800.0,
    );
    let g = tree.get(id(&doc, "g")).unwrap().rect;
    assert_eq!(g.height, grid::MAX_TRACKS as f32);
    // #b would start at row 9,000 and end past the limit: it moves back so
    // its span ends at the last line.
    let b = tree.get(id(&doc, "b")).unwrap().rect;
    assert_eq!(b.y, (grid::MAX_TRACKS - 9000) as f32);
    assert_eq!(b.height, 9000.0);
}

/// MetricShaper: 0.5em per character, so 8px per character at 16px.
fn text_box_width(html: &str, css: &str) -> f32 {
    let (_, tree) = page(html, css, 800.0);
    tree.boxes()
        .iter()
        .find(|b| b.kind == BoxKind::Text)
        .unwrap()
        .rect
        .width
}

#[test]
fn letter_spacing_widens_each_character() {
    assert_eq!(text_box_width("<div>abc</div>", ""), 24.0);
    assert_eq!(
        text_box_width("<div>abc</div>", "div { letter-spacing: 2px }"),
        30.0
    );
    // A space is a character too: a float shrink-wraps "ab cd" to 5 of them.
    let (doc, tree) = page(
        r#"<div id="f">ab cd</div>"#,
        "#f { float: left; letter-spacing: 2px }",
        800.0,
    );
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.width, 50.0);
}

#[test]
fn letter_spacing_reaches_flex_and_grid_text() {
    for display in ["flex", "grid"] {
        let css = format!("div {{ display: {display}; letter-spacing: 2px }}");
        assert_eq!(text_box_width("<div>abc</div>", &css), 30.0, "{display}");
    }
}

#[test]
fn text_transform_measures_the_transformed_text() {
    // "ß" uppercases to "SS": two characters wide.
    assert_eq!(text_box_width("<div>ß</div>", ""), 8.0);
    assert_eq!(
        text_box_width("<div>ß</div>", "div { text-transform: uppercase }"),
        16.0
    );
}

fn text_box<'t>(tree: &'t LayoutTree, text: &str) -> &'t LayoutBox {
    tree.boxes()
        .iter()
        .find(|b| b.text.as_deref() == Some(text))
        .unwrap_or_else(|| panic!("no text box {text:?}"))
}

#[test]
fn before_and_after_lay_out_around_the_content() {
    let (_, tree) = page(
        r#"<p id="p">mid</p>"#,
        r#"p::before { content: "<<" } p::after { content: ">>"; display: block }"#,
        800.0,
    );
    let before = text_box(&tree, "<<");
    let mid = text_box(&tree, "mid");
    let after = text_box(&tree, ">>");
    assert_eq!(before.rect.y, mid.rect.y);
    assert!(before.rect.x < mid.rect.x, "{before:?} {mid:?}");
    // A block ::after starts its own line.
    assert!(after.rect.y > mid.rect.y);
    assert_eq!(after.rect.x, 0.0);
}

/// arniko's status dot: an empty inline-block with a size, as the first
/// item of an inline-flex badge.
#[test]
fn an_empty_content_box_takes_its_size() {
    let (doc, tree) = page(
        r#"<div><span id="s">ok</span></div>"#,
        r#"#s { display: inline-flex; gap: 6px }
           #s::before { content: ""; display: inline-block; width: 6px; height: 6px }"#,
        800.0,
    );
    let s = tree.get(id(&doc, "s")).unwrap();
    let dot = tree
        .boxes()
        .iter()
        .find(|b| b.rect.width == 6.0 && b.rect.height == 6.0)
        .unwrap_or_else(|| panic!("{:#?}", tree.boxes()));
    assert_eq!(dot.rect.x, s.rect.x);
    // The dot, the gap, then the text.
    assert_eq!(text_box(&tree, "ok").rect.x, s.rect.x + 12.0);
}

/// arniko's tooltip: an absolutely positioned ::after showing an attribute.
#[test]
fn a_positioned_after_shows_its_attribute() {
    let (doc, tree) = page(
        r#"<div id="t" data-tip="hello" style="position: relative; margin-top: 100px">x</div>"#,
        r#"#t::after { content: attr(data-tip); position: absolute; left: 0; bottom: 100% }"#,
        800.0,
    );
    let tip = text_box(&tree, "hello");
    let t = tree.get(id(&doc, "t")).unwrap();
    assert!(tip.rect.y < t.rect.y, "{tip:?} above {t:?}");
}

/// Clicking generated content clicks the element it belongs to.
#[test]
fn hit_testing_generated_content_finds_its_element() {
    let (doc, tree) = page(
        r#"<div id="d"></div>"#,
        r#"#d::before { content: "x"; display: block; height: 20px }"#,
        800.0,
    );
    let hit = tree.hit_test(Point::new(2.0, 5.0)).unwrap();
    let d = id(&doc, "d");
    assert!(hit == d || doc.tree().parent(hit) == Some(d), "{hit:?}");
    assert!(doc.node(hit).is_some(), "a hit is always a real DOM node");
}

/// `bottom`/`right` with an auto height/width place the box by its
/// measured size.
#[test]
fn bottom_and_right_insets_place_an_auto_sized_absolute_box() {
    let (doc, tree) = page(
        r#"<div id="c" style="position: relative; margin-top: 100px; height: 50px; width: 400px">
             <div id="a" style="position: absolute; left: 0; bottom: 100%">hello</div>
             <div id="b" style="position: absolute; right: 0; bottom: 0"><span>abcd</span></div>
           </div>"#,
        "",
        800.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap().rect;
    let b = tree.get(id(&doc, "b")).unwrap().rect;
    // "hello": 40 x 19.2, its bottom at the container's top (y = 100).
    assert!((a.bottom() - 100.0).abs() < 1e-3, "{a:?}");
    assert!((b.bottom() - 150.0).abs() < 1e-3, "{b:?}");
    assert!((b.right() - 400.0).abs() < 1e-3, "{b:?}");
    assert_eq!(b.width, 32.0);
    // The content moves with its box.
    let hello = text_box(&tree, "hello").rect;
    assert!((hello.y - a.y).abs() < 1e-3, "{hello:?} {a:?}");
    let abcd = text_box(&tree, "abcd").rect;
    assert!((abcd.x - b.x).abs() < 1e-3, "{abcd:?} {b:?}");
}

/// A row flex container shrink-wraps to its items side by side plus the
/// gaps between them; a column one to its widest item.
#[test]
fn a_shrink_wrapped_flex_container_counts_items_and_gaps() {
    let (doc, tree) = page(
        r#"<div><span id="r"><i></i><i style="width: 20px"></i>ab</span></div>
           <div><span id="c"><i></i><i style="width: 20px"></i></span></div>"#,
        r#"#r { display: inline-flex; gap: 6px }
           #c { display: inline-flex; flex-direction: column; gap: 6px }
           i { display: block; width: 10px; height: 4px }"#,
        800.0,
    );
    // 10 + 6 + 20 + 6 + 16 ("ab").
    assert_eq!(tree.get(id(&doc, "r")).unwrap().rect.width, 58.0);
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.width, 20.0);
    assert_eq!(text_box(&tree, "ab").rect.width, 16.0);
}

/// An inline element shows only its in-flow text; a positioned descendant
/// is laid out as its own box.
#[test]
fn an_inline_elements_positioned_descendant_leaves_its_text() {
    let (doc, tree) = page(
        r#"<p>a <span id="s" style="position: relative">b<i id="i" style="position: absolute; top: 20px; left: 0">c</i><em style="display: none">d</em></span></p>"#,
        "",
        800.0,
    );
    let s = tree.get(id(&doc, "s")).unwrap();
    // The span's own text is "b": "c" is the positioned box's, "d" hidden.
    let shown: Vec<String> = drawn_text(&tree).into_iter().map(|(t, _)| t).collect();
    assert_eq!(shown, ["a", "b", "c"]);
    assert!(text_box(&tree, "b").rect.x >= s.rect.x);
    let i = tree.get(id(&doc, "i")).unwrap();
    assert_eq!(i.rect.y, s.rect.y + 20.0);
    assert_eq!(i.rect.x, s.rect.x);
}

/// Shrink-to-fit never makes an absolute box narrower than content that
/// can't wrap: a `nowrap` tooltip in a tiny containing block is as wide as
/// its text.
#[test]
fn an_absolute_box_is_as_wide_as_its_unwrappable_content() {
    let (doc, tree) = page(
        r#"<div style="position: relative; width: 8px; height: 8px">
             <div id="t" style="position: absolute; left: 0; top: 0; white-space: nowrap; padding: 0 4px">hello world</div>
           </div>"#,
        "",
        800.0,
    );
    // "hello world" is 11 characters (88px) plus 8px of padding.
    assert_eq!(tree.get(id(&doc, "t")).unwrap().rect.width, 96.0);
}

/// A shaper that kerns across spaces: a joined string is 3px narrower than
/// its words and spaces measured apart, as real fonts can be.
struct KerningShaper;

impl Shaper for KerningShaper {
    fn advance(&self, text: &str, _request: noonoo_text::FontRequest, _font_size: f32) -> f32 {
        let width = text.chars().count() as f32 * 10.0;
        if text.trim().contains(' ') {
            width - 3.0
        } else {
            width
        }
    }

    fn line_height(&self, _request: noonoo_text::FontRequest, _font_size: f32) -> f32 {
        20.0
    }
}

/// super-surfer's shortcuts: text sized to its own max-content must not
/// wrap, so max-content measures the way line breaking does.
#[test]
fn shrink_wrapped_text_fits_on_one_line_with_kerning() {
    let doc = noonoo_html::parse(r#"<div id="f" style="float: left">Close Tab</div>"#);
    let styles = noonoo_style::compute_with(
        &doc,
        &[],
        noonoo_style::Viewport {
            width: 800.0,
            height: 600.0,
        },
    );
    let tree = layout(&doc, &styles, Size::new(800.0, 600.0), &KerningShaper);
    // One line, at the shaper's normal line height.
    let height = tree.get(id(&doc, "f")).unwrap().rect.height;
    assert_eq!(height, 20.0);
}

/// super-surfer's devtools dock: `top: 0; bottom: 0` with an auto height
/// fills its containing block's height, and its children see that height.
#[test]
fn absolute_stretches_between_top_and_bottom() {
    let (doc, tree) = page(
        r#"<div id="cb"><div id="abs"><div id="half"></div></div></div>"#,
        "#cb { position: relative; width: 200px; height: 100px; margin-top: 5px } \
         #abs { position: absolute; top: 10px; bottom: 20px; right: 0; width: 50px } \
         #half { height: 50% }",
        400.0,
    );
    let abs = tree.get(id(&doc, "abs")).unwrap();
    assert_eq!(abs.rect, Rect::new(150.0, 15.0, 50.0, 70.0));
    assert_eq!(tree.get(id(&doc, "half")).unwrap().rect.height, 35.0);
}

/// A single-line row with a definite height is one line of that height:
/// stretched items fill it, and `center` centres within it.
#[test]
fn a_row_line_takes_the_definite_height_of_its_container() {
    let (doc, tree) = page(
        r#"<div id="row"><div id="tall"><div class="h10"></div></div><div id="mid"><div class="h10"></div></div></div>"#,
        "#row { display: flex; height: 300px } \
         #tall { width: 50px } \
         #mid { width: 50px; align-self: center } \
         .h10 { height: 10px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "tall")).unwrap().rect.height, 300.0);
    let mid = tree.get(id(&doc, "mid")).unwrap();
    assert_eq!((mid.rect.y, mid.rect.height), (145.0, 10.0));
}

/// super-surfer's main panel: a column stretched by a full-height row grows
/// its `flex: 1` content area into the rest of the height.
#[test]
fn a_column_stretched_by_a_row_grows_its_items_into_the_height() {
    let (doc, tree) = page(
        r#"<div id="shell"><div id="side"></div><div id="main"><div id="top"></div><div id="content"></div></div></div>"#,
        "#shell { display: flex; height: 600px } \
         #side { width: 100px } \
         #main { flex: 1 1 0px; display: flex; flex-direction: column } \
         #top { height: 97px } \
         #content { flex: 1 1 0px }",
        800.0,
    );
    assert_eq!(tree.get(id(&doc, "main")).unwrap().rect.height, 600.0);
    let content = tree.get(id(&doc, "content")).unwrap();
    assert_eq!((content.rect.y, content.rect.height), (97.0, 503.0));
}

/// Shrinking stops at an item's `min-width`; the rest of the overflow comes
/// out of the others.
#[test]
fn a_shrinking_row_item_stops_at_its_min_width() {
    let (doc, tree) = page(
        r#"<div id="row"><div id="a"></div><div id="b"></div></div>"#,
        "#row { display: flex; width: 300px } \
         #a { width: 200px; min-width: 180px } \
         #b { width: 200px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 180.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.width, 120.0);
}

/// super-surfer's devtools tabs: with `min-width: auto`, a flex item never
/// shrinks below its longest word, so labels overflow the row instead of
/// overlapping.
#[test]
fn a_shrinking_row_item_keeps_its_min_content_width() {
    // MetricShaper: 8px per character at 16px.
    let (doc, tree) = page(
        r#"<div id="row"><a id="a">Elements</a><a id="b">aa bbbb</a></div>"#,
        "#row { display: flex; width: 50px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 64.0);
    // Two words: it may wrap, so its floor is the longer word.
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.width, 32.0);
}

/// A clipping item has no automatic minimum: it shrinks below its content.
#[test]
fn a_clipping_row_item_shrinks_below_its_content() {
    let (doc, tree) = page(
        r#"<div id="row"><a id="a">Elements</a><a id="b">Elements</a></div>"#,
        "#row { display: flex; width: 64px } a { overflow: hidden }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 32.0);
}

/// super-surfer's devtools tabs: a flex item's width holds its transformed,
/// letter-spaced label.
#[test]
fn a_flex_item_is_as_wide_as_its_transformed_label() {
    let (doc, tree) = page(
        r#"<div id="tabs"><a id="tab">Elements</a></div>"#,
        "#tabs { display: flex } \
         #tab { padding: 7px 14px; font-size: 11px; text-transform: uppercase; \
                letter-spacing: .04em }",
        400.0,
    );
    let tab = tree.get(id(&doc, "tab")).unwrap();
    let label = text_box(&tree, "Elements");
    assert_eq!(tab.rect.width, label.rect.width + 28.0);
    assert_eq!(label.rect.x, tab.rect.x + 14.0);
}

/// Elements laid out (for real or to measure) by one `layout` call.
fn layout_work(html: &str, css: &str) -> usize {
    LAYOUTS.with(|count| count.set(0));
    page(html, css, 1024.0);
    LAYOUTS.with(|count| count.get())
}

#[test]
fn nested_measuring_containers_cost_linear_work() {
    // Each level measures its child before placing it. A measured box's
    // height is reused, not laid out again, so doubling the depth roughly
    // doubles the work instead of squaring it.
    for css in [
        "div { display: flex }",
        "div { display: grid } p { margin: 0 }",
        "div { display: flex; flex-direction: column }",
        "div { display: inline-block }",
    ] {
        let nested = |depth: usize| {
            layout_work(
                &format!(
                    "{}<p>x</p>{}",
                    "<div>".repeat(depth),
                    "</div>".repeat(depth)
                ),
                css,
            )
        };
        let (shallow, deep) = (nested(32), nested(64));
        assert!(
            deep <= 3 * shallow,
            "{css}: depth 32 took {shallow} layouts, depth 64 took {deep}"
        );
    }
}

#[test]
fn reusing_measured_heights_changes_no_box() {
    let shapes = [
        "div { display: grid } p { margin: 0; grid-row: span 3 }",
        "div { display: flex }",
        "div { display: flex; padding: 3px } div:nth-child(odd) { width: 90% }",
        "div { display: flex; flex-direction: column }",
        "div { display: flex; flex-direction: column; height: 50% }",
        "div { display: inline-block; padding: 1px }",
        "div { float: left; width: 95% }",
        "div { display: table-cell }",
        "div { display: flex; align-items: center } p { width: 30px }",
        "div { display: grid; grid-template-columns: 1fr 2fr } div > div { width: 40px }",
        "div { overflow: hidden; display: flex; flex-wrap: wrap } p { flex: 1 0 20px }",
    ];
    let nested = format!(
        "{}<p>x y z</p><span>tail words</span>{}",
        "<div><b>lead</b>".repeat(9),
        "<p>after</p></div>".repeat(9)
    );
    let mut pages: Vec<(String, &str)> = shapes.iter().map(|css| (nested.clone(), *css)).collect();
    // Inside a measured flex item: the same box measured at one width and
    // laid out at another (shrinking, wrapping items), without and then with
    // a definite height (a stretched grid row), and without and then with
    // a column flex container's decided height.
    let words = "many words that wrap ".repeat(6);
    pages.extend([
        (
            format!("<div class=o><div class=r><p>{words}</p><p>{words}</p></div></div>"),
            ".o, .r { display: flex } p { margin: 0 }",
        ),
        (
            "<div class=o><div class=g><div><p>x</p></div></div></div>".to_owned(),
            ".o { display: flex } .g { display: grid; grid-template-rows: 100px } p { margin: 0 }",
        ),
        (
            "<div class=o><div class=c><div class=i><p>x</p></div></div></div>".to_owned(),
            ".o { display: flex } .c { display: flex; flex-direction: column; height: 200px } \
             .i { flex: 1 } p { margin: 0 }",
        ),
    ]);
    for (html, css) in &pages {
        let (html, css) = (html.as_str(), *css);
        let boxes = |reuse: bool| {
            REUSE_MEASURED.with(|cell| cell.set(reuse));
            let (_, tree) = page(html, css, 300.0);
            REUSE_MEASURED.with(|cell| cell.set(true));
            tree.boxes()
                .iter()
                .map(|b| (b.node, b.rect, b.children.clone()))
                .collect::<Vec<_>>()
        };
        // A height is a difference of positions, so the same box placed
        // elsewhere can differ in the last bit; anything more is a bug.
        let (reused, reference) = (boxes(true), boxes(false));
        assert_eq!(reused.len(), reference.len(), "{css}");
        for (a, b) in reused.iter().zip(&reference) {
            let close = |p: f32, q: f32| (p - q).abs() < 0.01;
            assert!(
                a.0 == b.0
                    && a.2 == b.2
                    && close(a.1.x, b.1.x)
                    && close(a.1.y, b.1.y)
                    && close(a.1.width, b.1.width)
                    && close(a.1.height, b.1.height),
                "{css}: {a:?} vs {b:?}"
            );
        }
    }
}

#[test]
fn table_cells_in_a_block_sit_side_by_side_in_an_anonymous_table() {
    let (doc, tree) = page(
        "<div id=p><p id=before>x</p><div id=a class=c>aa</div>\n  <div id=b class=c>bbbb</div>\n<p id=after>x</p></div>",
        ".c { display: table-cell; padding: 2px } p { margin: 0 } #before { margin-bottom: 10px }",
        400.0,
    );
    let (a, b) = (
        tree.get(id(&doc, "a")).unwrap().rect,
        tree.get(id(&doc, "b")).unwrap().rect,
    );
    let before = tree.get(id(&doc, "before")).unwrap().rect;
    assert_eq!(a.y, before.y + before.height + 10.0, "below the margin");
    assert_eq!(a.y, b.y, "one row");
    assert_eq!(b.x, a.x + a.width, "b follows a");
    // The anonymous table shrinks to its cells' contents.
    assert!(b.x + b.width < 400.0, "{b:?}");
    let after = tree.get(id(&doc, "after")).unwrap().rect;
    assert_eq!(after.y, a.y + a.height, "the next block goes below the row");
}

#[test]
fn a_table_row_in_a_block_lays_its_cells_out_side_by_side() {
    let (doc, tree) = page(
        "<div><div class=r><div id=a class=c>a</div><div id=b class=c>b</div></div>\n\
         <div class=r><div class=c>wider</div><div id=d class=c>d</div></div></div>",
        ".r { display: table-row } .c { display: table-cell }",
        400.0,
    );
    let (a, b) = (
        tree.get(id(&doc, "a")).unwrap().rect,
        tree.get(id(&doc, "b")).unwrap().rect,
    );
    assert_eq!(a.y, b.y);
    // The rows share columns: the first is as wide as its widest cell.
    let d = tree.get(id(&doc, "d")).unwrap().rect;
    assert_eq!(b.x, d.x, "{b:?} lines up with {d:?}");
    assert!(d.y > b.y);
}

#[test]
fn a_shrink_to_fit_box_holding_table_cells_is_as_wide_as_the_row() {
    let (doc, tree) = page(
        "<div id=f><div id=a class=c>aaaa</div>\n<div id=b class=c>bbbb</div><p>x</p><div class=c>cc</div></div>",
        "#f { float: left } .c { display: table-cell }",
        400.0,
    );
    let (a, b) = (
        tree.get(id(&doc, "a")).unwrap().rect,
        tree.get(id(&doc, "b")).unwrap().rect,
    );
    let float = tree.get(id(&doc, "f")).unwrap().rect;
    assert_eq!(a.y, b.y);
    // Not squeezed: as wide as the same cell with room to spare.
    let (alone_doc, alone) = page(
        "<div><div id=a class=c>aaaa</div></div>",
        ".c { display: table-cell }",
        400.0,
    );
    assert_eq!(a.width, alone.get(id(&alone_doc, "a")).unwrap().rect.width);
    assert!(
        (float.width - (a.width + b.width)).abs() < 0.01,
        "{float:?} holds {a:?} and {b:?}"
    );
}

#[test]
fn a_bounding_client_rect_follows_scrolling_ancestors_and_spans_fragments() {
    let (doc, mut tree) = page(
        r#"<div id=s><div id=pad></div><p id=p>one two three four five six</p></div>"#,
        "#s { height: 50px; overflow: scroll } #pad { height: 100px } p { margin: 0; width: 40px }",
        400.0,
    );
    let p = id(&doc, "p");
    let before = tree.bounding_client_rect(p).unwrap();
    assert_eq!(
        before,
        tree.get(p).unwrap().rect,
        "unscrolled: the border box"
    );
    let s = id(&doc, "s");
    let scrolled = tree.scroll_by(s, 30.0);
    let after = tree.bounding_client_rect(p).unwrap();
    assert_eq!(after.y, before.y - scrolled);
    assert_eq!((after.x, after.width), (before.x, before.width));
    // A wrapped text node's fragments: one rect around them all.
    let text = doc.tree().children(p)[0];
    let text_rect = tree.bounding_client_rect(text).unwrap();
    assert!(
        text_rect.height > 19.2 * 2.0,
        "{text_rect:?} spans the lines"
    );
    assert_eq!(tree.bounding_client_rect(doc.tree().upcoming_id(0)), None);
}

const STICKY_CSS: &str = "#s { height: 100px; overflow: scroll } #pad { height: 50px } \
     #sec { height: 150px } #h { position: sticky; top: 10px; height: 20px } \
     #rest { height: 300px } p { margin: 0 }";
const STICKY_HTML: &str = "<div id=s><div id=pad></div><div id=sec><div id=h><p id=t>head</p></div></div><div id=rest></div></div>";

#[test]
fn a_sticky_box_stays_in_flow_until_scrolling_would_carry_it_past_its_inset() {
    let (doc, mut tree) = page(STICKY_HTML, STICKY_CSS, 400.0);
    let (h, s, t) = (id(&doc, "h"), id(&doc, "s"), id(&doc, "t"));
    // At rest it sits where flow put it: unlike `relative`, `top` alone
    // doesn't move it.
    assert_eq!(tree.get(h).unwrap().rect.y, 50.0);
    // Scrolled 30px it's still below the inset; at 80px it sticks 10px
    // below the scrollport's top.
    tree.scroll_by(s, 30.0);
    assert_eq!(tree.bounding_client_rect(h).unwrap().y, 20.0);
    tree.scroll_by(s, 50.0);
    assert_eq!(tree.bounding_client_rect(h).unwrap().y, 10.0);
    assert_eq!(tree.get(h).unwrap().rect.y, 90.0, "in content space");
    // Its contents move with it.
    assert_eq!(tree.get(t).unwrap().rect.y, 90.0);
    // It never leaves its containing block (#sec ends at 200px).
    tree.scroll_by(s, 100.0);
    assert_eq!(tree.get(h).unwrap().rect.y, 180.0);
    // Scrolling back returns it to flow.
    tree.scroll_by(s, -1000.0);
    assert_eq!(tree.get(h).unwrap().rect.y, 50.0);
    assert_eq!(tree.get(t).unwrap().rect.y, 50.0);
}

#[test]
fn a_bottom_sticky_box_waits_at_the_bottom_of_the_scrollport() {
    let (doc, mut tree) = page(
        "<div id=s><div id=pad></div><div id=f></div></div>",
        "#s { height: 100px; overflow: scroll } #pad { height: 300px } \
         #f { position: sticky; bottom: 0; height: 20px }",
        400.0,
    );
    let (f, s) = (id(&doc, "f"), id(&doc, "s"));
    assert_eq!(tree.get(f).unwrap().rect.y, 80.0, "pulled up into view");
    tree.scroll_by(s, 100.0);
    assert_eq!(tree.get(f).unwrap().rect.y, 180.0);
    tree.scroll_by(s, 1000.0);
    assert_eq!(
        tree.get(f).unwrap().rect.y,
        300.0,
        "back in flow at the end"
    );
}

#[test]
fn without_a_scrolling_ancestor_a_sticky_box_sticks_to_the_page() {
    let (doc, mut tree) = page(
        "<div id=pad></div><div id=h></div><div id=rest></div>",
        "#pad { height: 50px } #h { position: sticky; top: 0; height: 20px } #rest { height: 2000px }",
        400.0,
    );
    let h = id(&doc, "h");
    assert_eq!(tree.get(h).unwrap().rect.y, 50.0);
    tree.set_page_scroll(Point::new(0.0, 500.0));
    assert_eq!(tree.get(h).unwrap().rect.y, 500.0);
    tree.set_page_scroll(Point::ZERO);
    assert_eq!(tree.get(h).unwrap().rect.y, 50.0);
}

#[test]
fn a_sticky_box_never_moves_against_its_containing_block() {
    // Taller than what's left of its block: pushing it would pull it up.
    let (doc, mut tree) = page(
        "<div id=s><div id=sec><div id=h></div></div><div id=rest></div></div>",
        "#s { height: 100px; overflow: scroll } #sec { height: 10px } \
         #h { position: sticky; top: 0; height: 20px } #rest { height: 300px }",
        400.0,
    );
    tree.scroll_by(id(&doc, "s"), 50.0);
    assert_eq!(tree.get(id(&doc, "h")).unwrap().rect.y, 0.0);
    // Above its block's start (a negative margin), pulling it up toward the
    // scrollport's bottom would push it down.
    let (doc, tree) = page(
        "<div id=s><div id=pad></div><div id=b><div id=f></div></div></div>",
        "#s { height: 100px; overflow: scroll } #pad { height: 200px } \
         #b { height: 50px; padding-top: 1px } \
         #f { position: sticky; bottom: 0; height: 20px; margin-top: -10px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.y, 191.0);
}

#[test]
fn sticky_boxes_measured_by_a_flex_container_are_moved_once() {
    let (doc, mut tree) = page(
        // The sticky item second: its measured boxes' indices are the
        // first item's in the real layout.
        "<div class=f><div id=o><p id=ot>o</p></div><div id=h><p id=ht>h</p></div></div>",
        ".f { display: flex; height: 2000px; align-items: flex-start } \
         #h { position: sticky; top: 0 } p { margin: 0 }",
        400.0,
    );
    let before: Vec<_> = tree.boxes().iter().map(|b| (b.node, b.rect)).collect();
    tree.set_page_scroll(Point::new(0.0, 500.0));
    for (b, (node, rect)) in tree.boxes().iter().zip(&before) {
        let h = id(&doc, "h");
        let moved = node.is_some_and(|node| doc.tree().is_ancestor_of(h, node));
        let expected = if moved { rect.y + 500.0 } else { rect.y };
        assert_eq!(b.rect.y, expected, "{node:?}");
    }
}

/// An auto width fills the containing block less the margins (bliss-diff:
/// `body { margin: 8px }` was as wide as the viewport and overflowed it).
#[test]
fn an_auto_width_leaves_room_for_the_margins() {
    let (doc, tree) = page(
        r#"<div id="a"></div><div id="b"><div id="c"></div></div>"#,
        "#a { margin: 10px; height: 5px } #b { margin: 0 30px } #c { height: 5px }",
        400.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap().rect;
    assert_eq!((a.x, a.width), (10.0, 380.0));
    let c = tree.get(id(&doc, "c")).unwrap().rect;
    assert_eq!((c.x, c.width), (30.0, 340.0));
}

#[test]
fn the_body_margin_keeps_the_page_inside_the_viewport() {
    let (doc, tree) = page_with_ua_body(r#"<body id="body"><p id="p">x</p></body>"#, 400.0);
    let body = tree.get(id(&doc, "body")).unwrap().rect;
    assert_eq!((body.x, body.width), (8.0, 384.0));
}

#[test]
fn a_percentage_width_still_resolves_against_the_container() {
    let (doc, tree) = page(
        r#"<div id="a"></div>"#,
        "#a { width: 50%; margin: 0 20px; height: 5px }",
        400.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap().rect;
    assert_eq!((a.x, a.width), (20.0, 200.0));
}

#[test]
fn a_grid_item_with_margins_fills_its_area_less_them() {
    let (doc, tree) = page(
        r#"<div id="g"><div id="a"></div></div>"#,
        "#g { display: grid; grid-template-columns: 200px } #a { margin: 0 10px; height: 5px }",
        400.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap().rect;
    assert_eq!((a.x, a.width), (10.0, 180.0));
}

/// bliss-diff: `display: ` with no value is invalid and dropped, so a
/// `<div>` stays a block (super-surfer's frame emits it from an empty
/// template value).
#[test]
fn an_empty_declaration_is_dropped() {
    let (doc, tree) = page(
        r#"<div id="a" style="display: ">some words</div>"#,
        "",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().kind, BoxKind::Block);
}

/// bliss-diff (example.com): auto side margins centre a block of known
/// width.
#[test]
fn auto_side_margins_centre_a_block() {
    let (doc, tree) = page(
        r#"<div id="a"></div><div id="b"></div>"#,
        "#a { width: 100px; height: 5px; margin: 0 auto }
         #b { width: 100px; height: 5px; margin-left: auto }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.x, 150.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.x, 300.0);
}

#[test]
fn auto_margins_centre_a_block_narrowed_by_max_width() {
    let (doc, tree) = page(
        r#"<div id="a"></div><div id="b"></div>"#,
        "#a { max-width: 100px; height: 5px; margin: 0 auto }
         #b { height: 5px; margin: 0 auto }",
        400.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap().rect;
    assert_eq!((a.x, a.width), (150.0, 100.0));
    // An auto width fills the container: nothing is left to share.
    let b = tree.get(id(&doc, "b")).unwrap().rect;
    assert_eq!((b.x, b.width), (0.0, 400.0));
}

/// bliss-diff (Wikipedia's navboxes): measuring a `width: 100%` cell's
/// preferred width resolved the percentage against infinity, so a table
/// nested in such a cell came out infinitely wide.
#[test]
fn nested_percentage_tables_stay_finite() {
    let (_doc, tree) = page(
        r#"<table><tr><td colspan="2" style="width: 100%"><table><tr><td colspan="2" style="width: 100%">x</td></tr></table></td></tr></table>"#,
        "",
        400.0,
    );
    for b in tree.boxes() {
        assert!(
            b.rect.width.is_finite() && b.rect.x.is_finite(),
            "{:?} {:?}",
            b.node,
            b.rect
        );
    }
}

/// bliss-diff (the surfer shell's find bar): an empty text field laid out
/// 0 px wide. A text field is `size` characters wide (20 by default),
/// whatever its value; a CSS width still wins.
#[test]
fn a_text_field_is_as_wide_as_its_size() {
    let (doc, tree) = page(
        r#"<p><input id="a"> <input id="b" size="5" value="a much longer value than five"> <input id="c" style="width: 50px"> <input id="d" type="checkbox"></p>"#,
        // Controls default to a 13.33px font: set 16px for round numbers.
        "input { padding: 0; border: 0; font-size: 16px }",
        400.0,
    );
    let width = |name: &str| tree.get(id(&doc, name)).unwrap().content_box().width;
    // MetricShaper: a 16px "0" is 8px wide.
    assert_eq!(width("a"), 160.0);
    assert_eq!(width("b"), 40.0);
    assert_eq!(width("c"), 50.0);
    // A checkbox takes no `size`: it is its own 13px.
    assert_eq!(width("d"), 13.0, "only text fields take a size");
}

/// The text boxes' (text, y) in reading order.
fn text_lines(tree: &LayoutTree) -> Vec<(String, f32)> {
    tree.boxes()
        .iter()
        .filter(|b| b.kind == BoxKind::Text)
        .map(|b| (b.text.clone().unwrap_or_default(), b.rect.y))
        .collect()
}

/// bliss-diff (the devtools console's error row): a preserved newline
/// didn't break the line, so a two-line `pre-wrap` message ran on as one.
#[test]
fn a_preserved_newline_breaks_the_line() {
    for white_space in ["pre", "pre-wrap", "pre-line"] {
        let (_doc, tree) = page(
            "<div>ab cd\nef</div>",
            &format!("div {{ white-space: {white_space}; line-height: 20px }}"),
            400.0,
        );
        let lines = text_lines(&tree);
        let ys: Vec<f32> = lines.iter().map(|(_, y)| *y).collect();
        let words: Vec<&str> = lines.iter().map(|(text, _)| text.as_str()).collect();
        assert_eq!(words.concat(), "abcdef", "{white_space}");
        assert_eq!(ys.first(), Some(&0.0), "{white_space}");
        assert_eq!(ys.last(), Some(&20.0), "{white_space}: ef starts line 2");
    }
    // Collapsible white space still collapses the newline.
    let (_doc, tree) = page("<div>ab\ncd</div>", "div { line-height: 20px }", 400.0);
    assert!(text_lines(&tree).iter().all(|(_, y)| *y == 0.0));
}

#[test]
fn a_shrink_to_fit_box_takes_its_widest_preserved_line() {
    let (doc, tree) = page(
        "<div id=\"f\">abcd\nab</div>",
        "#f { float: left; white-space: pre-wrap }",
        400.0,
    );
    // MetricShaper: 8px a character at 16px.
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.width, 32.0);
}

#[test]
fn loose_text_in_a_flex_container_breaks_at_preserved_newlines() {
    let (doc, tree) = page(
        "<div id=\"f\">abcd\nab</div>",
        "#f { display: flex; white-space: pre-wrap; line-height: 20px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.height, 40.0);
    assert_eq!(text_lines(&tree).last().map(|(_, y)| *y), Some(20.0));
}

/// A shaper with a font whose natural line height is 1.5em, unlike
/// [`MetricShaper`]'s 1.2em.
struct TallFont;

impl Shaper for TallFont {
    fn advance(&self, text: &str, request: noonoo_text::FontRequest, font_size: f32) -> f32 {
        MetricShaper.advance(text, request, font_size)
    }

    fn line_height(&self, _request: noonoo_text::FontRequest, font_size: f32) -> f32 {
        font_size * 1.5
    }
}

/// bliss-diff: `line-height: normal` was a fixed 1.2em, where browsers (and
/// bliss) use the font's own ascent, descent and line gap.
#[test]
fn a_normal_line_height_is_the_fonts_own() {
    let doc = noonoo_html::parse(
        r#"<p id="n">a</p><p id="z" style="line-height: 0">a</p><p id="x" style="line-height: 30px">a</p>"#,
    );
    let sheet = noonoo_css::Stylesheet::parse("body, p { margin: 0 }").unwrap();
    let styles = noonoo_style::compute(&doc, &[sheet]);
    let tree = layout(&doc, &styles, Size::new(400.0, 600.0), &TallFont);
    let height = |name: &str| tree.get(id(&doc, name)).unwrap().rect.height;
    assert_eq!(height("n"), 24.0);
    assert_eq!(height("z"), 0.0, "a zero line height is not `normal`");
    assert_eq!(height("x"), 30.0);
}

/// A highlighted code block: the newline sits inside an inline element's
/// own text (`<span>`), which lays out as one item on the line.
#[test]
fn a_preserved_newline_inside_an_inline_element_breaks_the_line() {
    let (_doc, tree) = page(
        r#"<pre><span id="s">ab\ncd</span>ef</pre>"#.replace("\\n", "\n").as_str(),
        "pre { margin: 0; line-height: 20px }",
        400.0,
    );
    let lines = drawn_text(&tree);
    let at = |word: &str| lines.iter().find(|(text, _)| text == word).map(|(_, y)| *y);
    assert_eq!(at("ab"), Some(0.0), "{lines:?}");
    assert_eq!(at("cd"), Some(20.0), "{lines:?}");
    assert_eq!(at("ef"), Some(20.0), "ef follows cd on its line: {lines:?}");
    // Collapsible white space keeps the element on one line.
    let (_doc, tree) = page(
        "<div><span>ab\ncd</span>ef</div>",
        "div { line-height: 20px }",
        400.0,
    );
    assert!(drawn_text(&tree).iter().all(|(_, y)| *y == 0.0));
}

/// Every box that draws text, element runs included, as (text, y).
fn drawn_text(tree: &LayoutTree) -> Vec<(String, f32)> {
    tree.boxes()
        .iter()
        .filter_map(|b| {
            b.text
                .clone()
                .filter(|t| !t.is_empty())
                .map(|t| (t, b.rect.y))
        })
        .collect()
}

#[test]
fn a_shrink_to_fit_box_takes_the_widest_line_of_an_inline_element() {
    let (doc, tree) = page(
        "<div id=\"f\"><span>abcd\nab</span></div>",
        "#f { float: left; white-space: pre }",
        400.0,
    );
    // MetricShaper: 8px a character at 16px.
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.width, 32.0);
}

/// bliss-diff (the devtools console): an inline element's text was one
/// unbreakable run, so a long `<span>` overflowed its line instead of
/// wrapping.
#[test]
fn an_inline_elements_text_wraps_at_the_line_end() {
    let (doc, tree) = page(
        r#"<div><span id="s">aaa bbb ccc</span> dd</div>"#,
        "div { width: 80px; line-height: 20px }",
        400.0,
    );
    // MetricShaper: 8px a character. "aaa bbb" (56px) fits in 80px, and
    // " ccc" would not; "ccc dd" shares the second line.
    let lines = drawn_text(&tree);
    assert_eq!(
        lines,
        vec![
            ("aaa bbb".to_owned(), 0.0),
            ("ccc".to_owned(), 20.0),
            ("dd".to_owned(), 20.0),
        ]
    );
    // One box per line for the span, each its line's share of the text.
    let span = id(&doc, "s");
    let fragments: Vec<Rect> = tree
        .boxes()
        .iter()
        .filter(|b| b.node == Some(span))
        .map(|b| b.rect)
        .collect();
    // Each wraps its font's content area (19.2px) in a 20px line, half the
    // leading (0.4px) below the line's top.
    let expected = [
        Rect::new(0.0, 0.4, 56.0, 19.2),
        Rect::new(0.0, 20.4, 24.0, 19.2),
    ];
    assert_eq!(fragments.len(), 2);
    for (got, want) in fragments.iter().zip(expected) {
        assert!(
            (got.x - want.x).abs() < 0.01
                && (got.y - want.y).abs() < 0.01
                && (got.width - want.width).abs() < 0.01
                && (got.height - want.height).abs() < 0.01,
            "{got:?} vs {want:?}"
        );
    }
}

#[test]
fn a_nowrap_inline_element_stays_on_one_line() {
    let (_doc, tree) = page(
        r#"<div><span>aaa bbb ccc</span></div>"#,
        "div { width: 80px; line-height: 20px } span { white-space: nowrap }",
        400.0,
    );
    assert_eq!(drawn_text(&tree), vec![("aaa bbb ccc".to_owned(), 0.0)]);
}

#[test]
fn a_shrink_to_fit_box_can_narrow_to_an_inline_elements_longest_word() {
    let (doc, tree) = page(
        r#"<div id="w"><div id="f"><span>aaaa bb cc</span></div></div>"#,
        "#w { width: 40px } #f { float: left }",
        400.0,
    );
    // Too narrow for the 80px line: the float shrinks to fit its 40px
    // container, no narrower than "aaaa".
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.width, 40.0);
}

/// The x of each drawn text run, in reading order.
fn drawn_x(tree: &LayoutTree) -> Vec<(String, f32)> {
    tree.boxes()
        .iter()
        .filter_map(|b| {
            b.text
                .clone()
                .filter(|t| !t.is_empty())
                .map(|t| (t, b.rect.x))
        })
        .collect()
}

/// An element nested in an inline element was folded into the outer one's
/// text, so `<b>` inside `<a>` laid out (and painted) in the link's style.
#[test]
fn a_nested_inline_element_keeps_its_own_style_and_box() {
    let (doc, tree) = page(
        r#"<p><a id="a">x <b id="b">yy</b> z</a></p>"#,
        "p { margin: 0 }",
        400.0,
    );
    // MetricShaper: 8px a character, 8.8px in bold.
    assert_eq!(
        drawn_x(&tree),
        vec![
            ("x".to_owned(), 0.0),
            ("yy".to_owned(), 16.0),
            ("z".to_owned(), 41.6)
        ]
    );
    let b = tree.get(id(&doc, "b")).unwrap();
    assert_eq!(b.style_node, id(&doc, "b"));
    assert!((b.rect.width - 17.6).abs() < 0.01, "{:?}", b.rect);
}

/// Text that touches an element in the source touches it on the line: no
/// space is drawn between them, and the line can't break there.
#[test]
fn an_element_boundary_without_white_space_adds_no_space() {
    let (_doc, tree) = page(
        r#"<p><b>Bold</b>ness, <a>x<b>y</b></a></p>"#,
        "p { margin: 0 }",
        400.0,
    );
    let at = drawn_x(&tree);
    // "Bold" is 35.2px in bold; "ness," follows it directly.
    assert_eq!(at[1], ("ness,".to_owned(), 35.2), "{at:?}");
    // " x" after the space, then "y" against it.
    let x = at[2].1;
    assert_eq!(at[3], ("y".to_owned(), x + 8.0), "{at:?}");
}

#[test]
fn a_word_split_across_elements_stays_on_one_line() {
    let (_doc, tree) = page(
        r#"<div><b>Bold</b>ness</div>"#,
        "div { width: 40px; line-height: 20px }",
        400.0,
    );
    // 35.2 + 32px overflows 40px, but there's no break opportunity.
    assert!(drawn_text(&tree).iter().all(|(_, y)| *y == 0.0));
}

#[test]
fn hidden_and_positioned_descendants_leave_the_line() {
    let (doc, tree) = page(
        r#"<p><span>a<span hidden>h</span>b<span id="abs" style="position: absolute">c</span></span></p>"#,
        "p { margin: 0 }",
        400.0,
    );
    assert_eq!(
        drawn_x(&tree)
            .into_iter()
            .filter(|(t, _)| t != "c")
            .collect::<Vec<_>>(),
        // Two text nodes of one span, touching: no space between.
        vec![("a".to_owned(), 0.0), ("b".to_owned(), 8.0)]
    );
    // The positioned descendant still gets laid out as its own box.
    assert!(tree.get(id(&doc, "abs")).is_some());
}

#[test]
fn an_atomic_inline_against_text_gets_no_space_but_may_wrap() {
    let (doc, tree) = page(
        r#"<div>ab<i id="i"></i>cd</div>"#,
        "div { width: 100px } i { display: inline-block; width: 20px; height: 10px }",
        400.0,
    );
    let i = tree.get(id(&doc, "i")).unwrap().rect;
    assert_eq!(i.x, 16.0, "touching ab");
    assert_eq!(drawn_x(&tree)[1], ("cd".to_owned(), 36.0));
    // Too narrow for all three: the line breaks beside the box.
    let (doc, tree) = page(
        r#"<div>ab<i id="i"></i>cd</div>"#,
        "div { width: 30px } i { display: inline-block; width: 20px; height: 10px }",
        400.0,
    );
    assert!(tree.get(id(&doc, "i")).unwrap().rect.y > 0.0);
}

/// The boxes an element has, in order: one per line it's on.
fn fragments(doc: &Document, tree: &LayoutTree, name: &str) -> Vec<LayoutBox> {
    let node = id(doc, name);
    tree.boxes()
        .iter()
        .filter(|b| b.node == Some(node))
        .cloned()
        .collect()
}

/// bliss-diff (devtools badges): an inline element's padding, border and
/// margin took no room on the line and painted nowhere.
#[test]
fn an_inline_elements_horizontal_edges_take_room_on_the_line() {
    let (doc, tree) = page(
        r#"<p id="p">a<span id="s">b</span>c</p>"#,
        "p { margin: 0 } span { margin: 0 3px; border: 2px solid #000; padding: 0 4px }",
        400.0,
    );
    // a (8px), the 3px margin, then the span's border box: 2 + 4 + 8 + 4 + 2.
    let span = fragments(&doc, &tree, "s");
    assert_eq!(span.len(), 1);
    assert_eq!(span[0].rect, Rect::new(11.0, -2.0, 20.0, 23.2));
    assert_eq!(
        (
            span[0].border.left,
            span[0].padding.left,
            span[0].padding.right
        ),
        (2.0, 4.0, 4.0)
    );
    assert_eq!(
        drawn_x(&tree),
        vec![
            ("a".to_owned(), 0.0),
            ("b".to_owned(), 17.0),
            ("c".to_owned(), 34.0)
        ]
    );
    // Vertical padding and borders don't make the line taller.
    assert_eq!(tree.get(id(&doc, "p")).unwrap().rect.height, 19.2);
}

#[test]
fn a_wrapped_inline_element_puts_its_start_edge_first_and_end_edge_last() {
    let (doc, tree) = page(
        r#"<div><span id="s">aaa bbb</span></div>"#,
        "div { width: 60px; line-height: 20px } span { padding: 0 5px }",
        400.0,
    );
    let span = fragments(&doc, &tree, "s");
    assert_eq!(span.len(), 2);
    // The content area (19.2px) half the leading (0.4px) into each line.
    let near = |a: Rect, b: Rect| {
        (a.x - b.x).abs() < 0.01
            && (a.y - b.y).abs() < 0.01
            && (a.width - b.width).abs() < 0.01
            && (a.height - b.height).abs() < 0.01
    };
    assert!(
        near(span[0].rect, Rect::new(0.0, 0.4, 29.0, 19.2)),
        "{:?}",
        span[0].rect
    );
    assert_eq!((span[0].padding.left, span[0].padding.right), (5.0, 0.0));
    assert!(
        near(span[1].rect, Rect::new(0.0, 20.4, 29.0, 19.2)),
        "{:?}",
        span[1].rect
    );
    assert_eq!((span[1].padding.left, span[1].padding.right), (0.0, 5.0));
}

#[test]
fn an_outer_inline_fragment_spans_its_nested_elements() {
    let (doc, tree) = page(
        r#"<p><a id="a">x<b id="b">y</b>z</a></p>"#,
        "p { margin: 0 } a { padding: 0 2px } b { padding: 0 3px }",
        400.0,
    );
    let a = fragments(&doc, &tree, "a");
    let b = fragments(&doc, &tree, "b");
    assert_eq!(a.len(), 1);
    assert!((a[0].rect.x - 0.0).abs() < 0.01 && (a[0].rect.width - 34.8).abs() < 0.01);
    assert!((b[0].rect.x - 10.0).abs() < 0.01 && (b[0].rect.width - 14.8).abs() < 0.01);
    // The outer fragment comes first, so it paints under the inner one.
    let index = |node| tree.boxes().iter().position(|b| b.node == Some(node));
    assert!(index(id(&doc, "a")) < index(id(&doc, "b")));
}

#[test]
fn an_inline_elements_padding_counts_in_a_shrink_to_fit_width() {
    let (doc, tree) = page(
        r#"<div id="f"><span>ab</span></div>"#,
        "#f { float: left } span { padding: 0 10px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.width, 36.0);
}

/// `pre` and `pre-wrap` keep their spaces: runs of them, indentation, and
/// tabs (to the next multiple of 8 columns). MetricShaper: 8px a character.
#[test]
fn preformatted_text_keeps_its_spaces() {
    let x_of = |html: &str, css: &str, word: &str| {
        let (_doc, tree) = page(html, css, 400.0);
        drawn_x(&tree)
            .into_iter()
            .find(|(text, _)| text.ends_with(word))
            .map(|(_, x)| x)
    };
    let pre = "pre { margin: 0 }";
    assert_eq!(
        x_of("<pre>a   b</pre>", pre, "b"),
        Some(32.0),
        "a run of spaces"
    );
    assert_eq!(
        x_of("<pre>x\n    y</pre>", pre, "y"),
        Some(32.0),
        "indentation"
    );
    assert_eq!(x_of("<pre>a\tb</pre>", pre, "b"), Some(64.0), "a tab");
    assert_eq!(
        x_of("<pre><span>a</span>  <span>b</span></pre>", pre, "b"),
        Some(24.0),
        "spaces between elements"
    );
    assert_eq!(
        x_of("<pre><span>ab</span>\tc</pre>", pre, "c"),
        Some(64.0),
        "a tab counts columns across elements"
    );
    assert_eq!(
        x_of("<div>a   b</div>", "", "b"),
        Some(16.0),
        "normal collapses"
    );
}

#[test]
fn pre_wrap_drops_the_spaces_where_it_wraps() {
    let (_doc, tree) = page(
        "<div>aaa  bbb</div>",
        "div { white-space: pre-wrap; width: 40px; line-height: 20px }",
        400.0,
    );
    assert_eq!(
        drawn_x(&tree)
            .into_iter()
            .zip(drawn_text(&tree))
            .map(|((t, x), (_, y))| (t, x, y))
            .collect::<Vec<_>>(),
        vec![("aaa".to_owned(), 0.0, 0.0), ("bbb".to_owned(), 0.0, 20.0)]
    );
}

#[test]
fn preserved_spaces_count_in_a_shrink_to_fit_width() {
    let (doc, tree) = page(
        "<div id=\"f\">a   b</div>",
        "#f { float: left; white-space: pre }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.width, 40.0);
}

#[test]
fn loose_preformatted_text_in_a_flex_container_keeps_its_spaces() {
    let (_doc, tree) = page(
        "<div id=\"f\">a   b</div>",
        "#f { display: flex; white-space: pre }",
        400.0,
    );
    assert_eq!(drawn_text(&tree)[0].0, "a   b");
    let run = tree
        .boxes()
        .iter()
        .find(|b| b.text.as_deref() == Some("a   b"))
        .unwrap();
    assert_eq!(run.rect.width, 40.0);
}

/// An empty inline element still makes an inline box: its padding and
/// borders take room on the line and paint (an icon slot, a dot).
#[test]
fn an_empty_inline_element_with_edges_still_has_a_box() {
    let (doc, tree) = page(
        r#"<p>a<span id="s"></span>b<i id="plain"></i>c</p>"#,
        "p { margin: 0 } span { padding: 0 4px; border: 1px solid #000 }",
        400.0,
    );
    let span = fragments(&doc, &tree, "s");
    assert_eq!(span.len(), 1);
    assert_eq!((span[0].rect.x, span[0].rect.width), (8.0, 10.0));
    assert_eq!(
        drawn_x(&tree),
        vec![
            ("a".to_owned(), 0.0),
            ("b".to_owned(), 18.0),
            ("c".to_owned(), 26.0)
        ]
    );
    // Without edges an empty element takes no room and draws no run, but
    // keeps a zero-width box where it sits, as in Chromium.
    let plain = fragments(&doc, &tree, "plain");
    assert_eq!(plain.len(), 1);
    assert_eq!((plain[0].rect.x, plain[0].rect.width), (26.0, 0.0));
    assert!(tree.boxes().iter().all(|b| b.text.as_deref() != Some("")));
}

#[test]
fn an_empty_inline_elements_edges_count_in_a_shrink_to_fit_width() {
    let (doc, tree) = page(
        r#"<div id="f">a<span></span></div>"#,
        "#f { float: left } span { padding: 0 10px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.width, 28.0);
}

/// A block inside an inline (CSS 2 §9.2.1.1) breaks the line: it is laid
/// out as a block between the inline content before and after it, with its
/// own width, height and margins. MetricShaper lines are 19.2px.
#[test]
fn a_block_inside_an_inline_is_laid_out_as_a_block() {
    let (doc, tree) = page(
        r#"<div id="c"><a>x <div id="b"></div> y</a></div><p id="after">z</p>"#,
        "p { margin: 0 } #b { width: 10px; height: 10px; margin: 3px 2px 6px }",
        400.0,
    );
    let block = tree.get(id(&doc, "b")).unwrap().rect;
    assert_eq!(
        (block.x, block.y, block.width, block.height),
        (2.0, 22.2, 10.0, 10.0)
    );
    assert_eq!(
        text_lines(&tree),
        vec![
            ("x".to_owned(), 0.0),
            ("y".to_owned(), 38.2),
            ("z".to_owned(), 57.4)
        ]
    );
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.height, 57.4);
}

/// A block alone in an inline makes no blank lines around it.
#[test]
fn a_lone_block_inside_an_inline_adds_no_lines() {
    let (doc, tree) = page(
        r#"<div id="c"><a href="x"><div id="b"></div></a></div>"#,
        "#b { width: 10px; height: 10px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.y, 0.0);
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.height, 10.0);
}

/// A shrink-to-fit container sizes around a block inside an inline.
#[test]
fn a_block_inside_an_inline_counts_in_a_shrink_to_fit_width() {
    let (doc, tree) = page(
        r#"<div id="f"><a href="x"><div id="b"></div></a></div>"#,
        "#f { float: left } #b { width: 10px; height: 10px; margin: 0 2px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.width, 14.0);
}

/// An auto-width table shrinks to fit its columns (CSS 2 §17.5.2): it
/// doesn't fill its container like a block. MetricShaper: 8px a character.
#[test]
fn an_auto_width_table_shrinks_to_fit_its_columns() {
    let (doc, tree) = page(
        r#"<table id="t"><tr><td>ab</td><td>c</td></tr><tr><td>d</td><td>efgh</td></tr></table>"#,
        "table { border-spacing: 0 } td { padding: 0 }",
        400.0,
    );
    // Columns: 16 and 32, the widest cell in each.
    assert_eq!(tree.get(id(&doc, "t")).unwrap().rect.width, 48.0);
}

/// A table's set width is resolved once, against its container, and its
/// columns stretch to fill it in proportion to their content.
#[test]
fn a_tables_columns_stretch_to_its_set_width() {
    let (doc, tree) = page(
        r#"<table id="t"><tr><td id="a">ab</td><td id="b">c</td></tr></table>"#,
        "table { border-spacing: 0 } #t { width: 50% } td { padding: 0 }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "t")).unwrap().rect.width, 200.0);
    let (a, b) = (
        tree.get(id(&doc, "a")).unwrap().rect,
        tree.get(id(&doc, "b")).unwrap().rect,
    );
    assert!((a.width - 400.0 / 3.0).abs() < 0.01, "{}", a.width);
    assert!((b.x + b.width - 200.0).abs() < 0.01, "{}", b.x + b.width);
}

/// A table's `width` attribute sizes it, as on Hacker News' main table.
#[test]
fn a_tables_width_attribute_sizes_it() {
    let (doc, tree) = page(
        r#"<table id="t" width="85%"><tr><td>a</td></tr></table>"#,
        "",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "t")).unwrap().rect.width, 340.0);
}

/// A shrink-to-fit box sizes around a table's columns, not its widest row.
#[test]
fn a_tables_max_content_width_sums_its_widest_columns() {
    let (doc, tree) = page(
        r#"<div id="f"><table><tr><td>ab</td><td>c</td></tr><tr><td>d</td><td>efgh</td></tr></table></div>"#,
        "table { border-spacing: 0 } #f { float: left } td { padding: 0 }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.width, 48.0);
}

/// `<center>` (`text-align: -webkit-center`) centres its blocks' margin
/// boxes as well as its text, and the centring inherits.
#[test]
fn center_centres_its_blocks_and_its_text() {
    let (doc, tree) = page(
        r#"<center><table id="t" style="width: 50%"><tr><td>a</td></tr></table><div id="d"></div><div><p id="p"></p></div>ab</center>"#,
        "#d { width: 100px; margin-left: 20px } p { width: 100px; margin: 0 }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "t")).unwrap().rect.x, 100.0);
    assert_eq!(tree.get(id(&doc, "d")).unwrap().rect.x, 160.0);
    assert_eq!(tree.get(id(&doc, "p")).unwrap().rect.x, 150.0);
    assert!(drawn_x(&tree).contains(&("ab".to_owned(), 192.0)));
}

/// Plain `text-align: center` leaves blocks where they are.
#[test]
fn text_align_center_does_not_move_blocks() {
    let (doc, tree) = page(
        r#"<div style="text-align: center"><div id="d"></div></div>"#,
        "#d { width: 100px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "d")).unwrap().rect.x, 0.0);
}

/// A cell's default pixel of padding, and `cellpadding="0"` taking it away.
#[test]
fn cells_take_their_padding_in_table_layout() {
    let (doc, tree) = page(
        r#"<table id="t"><tr><td>ab</td><td></td></tr></table><table id="z" cellpadding="0"><tr><td>ab</td></tr></table>"#,
        "table { border-spacing: 0 }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "t")).unwrap().rect.width, 20.0);
    assert_eq!(tree.get(id(&doc, "z")).unwrap().rect.width, 16.0);
}

/// Each `<br>` ends a line; only the last line of a block, if empty, is
/// dropped. So `x<br><br>` is two lines, as in browsers.
#[test]
fn every_break_but_a_trailing_one_makes_a_line() {
    let (doc, tree) = page(
        r#"<div id="a">x<br><br></div><div id="b">x<br></div><div id="c"><br><br></div><div id="e"><br></div><div id="f">x<br><br><br>y</div>"#,
        "",
        400.0,
    );
    let lines = |name: &str| tree.get(id(&doc, name)).unwrap().rect.height / 19.2;
    for (name, expected) in [("a", 2.0), ("b", 1.0), ("c", 2.0), ("e", 1.0), ("f", 4.0)] {
        assert!(
            (lines(name) - expected).abs() < 0.001,
            "{name}: {}",
            lines(name)
        );
    }
}

/// A text field has a browser's default padding and border, and the line
/// that holds it grows to fit. MetricShaper: `size` 2 is 16px of content.
#[test]
fn a_text_field_has_padding_and_a_border() {
    let (doc, tree) = page(
        r#"<div id="d"><input id="i" size="2"></div>"#,
        "input { font-size: 16px }",
        400.0,
    );
    let input = tree.get(id(&doc, "i")).unwrap().rect;
    assert_eq!(input.width, 24.0);
    assert!((input.height - 25.2).abs() < 0.01, "{}", input.height);
    assert!((tree.get(id(&doc, "d")).unwrap().rect.height - 25.2).abs() < 0.01);
}

/// A block inside an inline, with nothing before or after it, collapses its
/// margins with its container's and its siblings', as if the inline weren't
/// there (HN's upvote arrows: `<center><a><div class="votearrow">`).
#[test]
fn a_block_inside_an_inline_collapses_its_margins_through_its_container() {
    let (doc, tree) = page(
        r#"<div id="c"><div id="x"><a href="v"><div id="v"></div></a></div><p id="p">z</p></div>"#,
        "#c { padding: 1px } #v { height: 10px; margin: 3px 2px 6px } p { margin: 4px 0 0 }",
        400.0,
    );
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    // The 3px top margin collapses out of #x; #x is just the arrow.
    assert_eq!((rect("x").y, rect("x").height), (4.0, 10.0));
    assert_eq!(rect("v").y, 4.0);
    // The 6px bottom margin collapses out too, with the paragraph's 4px.
    assert_eq!(rect("p").y, 20.0);
    assert!(
        (rect("c").height - 40.2).abs() < 0.01,
        "{}",
        rect("c").height
    );
}

/// A box that starts a block formatting context (a table cell, an
/// inline-block, a float) keeps its children's margins inside it, padding
/// or not (CSS 2 §8.3.1).
#[test]
fn margins_never_collapse_through_a_formatting_context_root() {
    let (doc, tree) = page(
        r#"<table cellpadding="0"><tr><td id="d"><p>x</p></td></tr></table><div><span id="i" style="display: inline-block"><p>y</p></span></div><div id="f" style="float: left"><p>z</p></div>"#,
        "p { margin: 5px 0 }",
        400.0,
    );
    for name in ["d", "i", "f"] {
        let height = tree.get(id(&doc, name)).unwrap().rect.height;
        assert!((height - 29.2).abs() < 0.01, "{name}: {height}");
    }
}

/// White space between blocks makes no box (CSS 2 §9.2.2.1): the blocks'
/// margins still collapse across it, and with their container's.
#[test]
fn white_space_between_blocks_does_not_stop_margins_collapsing() {
    let (doc, tree) = page(
        "<div id=\"c\">\n  <p id=\"a\">a</p>\n  <p id=\"b\">b</p>\n</div>",
        "p { margin: 10px 0 }",
        400.0,
    );
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    assert_eq!(rect("c").y, 10.0);
    assert!((rect("b").y - 39.2).abs() < 0.01, "{}", rect("b").y);
    assert!(
        (rect("c").height - 48.4).abs() < 0.01,
        "{}",
        rect("c").height
    );
}

/// Preserved white space (`pre`) between blocks is content: its newline
/// makes a blank line, which separates the blocks' margins.
#[test]
fn preserved_white_space_between_blocks_is_a_line() {
    let (doc, tree) = page(
        "<div style=\"white-space: pre\"><p>a</p>\n<p id=\"d\">d</p></div>",
        "p { margin: 10px 0 } div { border: 1px solid }",
        400.0,
    );
    let y = tree.get(id(&doc, "d")).unwrap().rect.y;
    // 1 border + 10 + a + 10 + the blank line + d's 10.
    assert!(
        (y - (1.0 + 10.0 + 19.2 + 10.0 + 19.2 + 10.0)).abs() < 0.01,
        "{y}"
    );
}

/// A no-break space is not collapsible white space: runs of them keep their
/// width, and a line never breaks at one. MetricShaper: 8px a character.
#[test]
fn no_break_spaces_keep_their_width_and_never_break() {
    let (_doc, tree) = page(
        "<p>a\u{a0}\u{a0}b</p><div style=\"width: 30px\">aa\u{a0}bb cc</div>",
        "p { margin: 0 }",
        400.0,
    );
    let lines = text_lines(&tree);
    assert_eq!(lines[0].0, "a\u{a0}\u{a0}b");
    assert_eq!(
        lines[1..]
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>(),
        ["aa\u{a0}bb", "cc"]
    );
    let widths: Vec<f32> = tree
        .boxes()
        .iter()
        .filter(|b| b.text.as_deref() == Some("a\u{a0}\u{a0}b"))
        .map(|b| b.rect.width)
        .collect();
    assert_eq!(widths, [32.0]);
}

/// A word joined by a no-break space is one unbreakable piece for
/// min-content sizing too.
#[test]
fn a_no_break_space_holds_together_in_min_content() {
    let (doc, tree) = page(
        "<div style=\"width: 10px\"><table id=\"t\" cellpadding=\"0\"><tr><td>a\u{a0}b c</td></tr></table></div>",
        "table { border-spacing: 0 }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "t")).unwrap().rect.width, 24.0);
}

/// A table too narrow for its columns' max-content gives each column its
/// min-content first and shares the rest by how much more each wants
/// (CSS 2 §17.5.2.2): a no-wrap column keeps its text whole, as Wikipedia's
/// navbox groups do. MetricShaper: 8px a character.
#[test]
fn table_columns_never_shrink_below_their_min_content() {
    let (doc, tree) = page(
        r#"<table id="t" style="width: 400px" cellpadding="0"><tr><td id="g" style="white-space: nowrap">Features here</td><td id="l">one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty</td></tr></table>"#,
        "table { border-spacing: 0 }",
        400.0,
    );
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    assert_eq!(rect("g").width, 104.0);
    assert_eq!(rect("l").width, 296.0);
}

/// Between min- and max-content, each column gets its minimum plus a share
/// of the space left in proportion to how much wider it would like to be.
#[test]
fn table_columns_share_the_space_above_their_minimums() {
    let (doc, tree) = page(
        // Column a: min 16 ("aa"), max 64 ("aa bb cc"); column b: min and
        // max 8. The minimums take 24 of 40; a alone wants more, so it
        // gets all 16 left: 16 + 48 * 16 / 48 = 32.
        r#"<table id="t" style="width: 40px" cellpadding="0"><tr><td id="a">aa bb cc</td><td id="b">d</td></tr></table>"#,
        "table { border-spacing: 0 }",
        400.0,
    );
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    assert_eq!(rect("a").width, 32.0);
    assert_eq!(rect("b").width, 8.0);
}

/// A table is never narrower than its columns' minimums: a set width too
/// small grows, and its min-content is the sum of its columns'.
#[test]
fn a_table_is_at_least_its_min_content_wide() {
    let (doc, tree) = page(
        r#"<table id="t" style="width: 50px" cellpadding="0"><tr><td>abcdefgh ij</td></tr></table><div style="width: 10px"><table id="u" cellpadding="0"><tr><td>abc d</td><td>ef</td></tr></table></div>"#,
        "table { border-spacing: 0 }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "t")).unwrap().rect.width, 64.0);
    assert_eq!(tree.get(id(&doc, "u")).unwrap().rect.width, 40.0);
}

/// A cell fills its column: its own `width` (Wikipedia's `width: 1%` group
/// cells) only informed the column sizing, and isn't resolved again inside.
#[test]
fn a_cell_fills_its_column_whatever_its_width() {
    let (doc, tree) = page(
        r#"<table style="width: 400px" cellpadding="0"><tr><td id="g" style="width: 1%; white-space: nowrap">Features here</td><td id="l" style="width: 100%">one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty</td></tr></table>"#,
        "table { border-spacing: 0 }",
        400.0,
    );
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    assert_eq!(rect("g").width, 104.0);
    assert_eq!((rect("l").x, rect("l").width), (104.0, 296.0));
}

/// Space beyond the columns' max-content goes to the auto columns: a column
/// with a pixel width keeps it (HN's 18px logo column).
#[test]
fn a_fixed_width_column_keeps_its_width_in_a_wider_table() {
    let (doc, tree) = page(
        r#"<table style="width: 400px" cellpadding="0"><tr><td id="a" style="width: 20px">a</td><td id="b">bb</td><td id="c">cccc</td></tr></table>"#,
        "table { border-spacing: 0 }",
        400.0,
    );
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    assert_eq!(rect("a").width, 20.0);
    // The auto columns share the other 380 by their max-content, 16 : 32.
    assert!(
        (rect("b").width - 380.0 / 3.0).abs() < 0.01,
        "{}",
        rect("b").width
    );
    assert!(
        (rect("c").width - 760.0 / 3.0).abs() < 0.01,
        "{}",
        rect("c").width
    );
}

/// A percentage cell width makes its column that share of the table, before
/// the auto columns get the rest. MetricShaper: 8px a character.
#[test]
fn a_percentage_column_takes_its_share_of_the_table() {
    let (doc, tree) = page(
        r#"<table style="width: 400px" cellpadding="0"><tr><td id="a" style="width: 25%">a</td><td id="b">b</td></tr></table>"#,
        "table { border-spacing: 0 }",
        400.0,
    );
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    assert_eq!((rect("a").width, rect("b").width), (100.0, 300.0));
}

/// A `width: 100%` column (Wikipedia's navbox lists) takes all the room the
/// other columns' minimums leave it.
#[test]
fn a_full_width_column_leaves_the_others_their_minimums() {
    let (doc, tree) = page(
        r#"<table style="width: 400px" cellpadding="0"><tr><td id="g" style="white-space: nowrap">Group</td><td id="l" style="width: 100%">x</td></tr></table>"#,
        "table { border-spacing: 0 }",
        400.0,
    );
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    assert_eq!((rect("g").width, rect("l").width), (40.0, 360.0));
}

/// Short of room, a percentage column reaches its share first; the auto
/// columns grow above their minimums with what's left.
#[test]
fn a_percentage_column_grows_before_auto_columns() {
    let (doc, tree) = page(
        r#"<table style="width: 200px" cellpadding="0"><tr><td id="a" style="width: 50%">aaaa aaaa</td><td id="b">bbbbb bbbbb bbbbb bbbbb bbbbb bbbbb bbbbb bbbbb</td></tr></table>"#,
        "table { border-spacing: 0 }",
        400.0,
    );
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    assert_eq!((rect("a").width, rect("b").width), (100.0, 100.0));
}

/// A row item's main-axis margins take space in the line: the next item
/// starts after the margin.
#[test]
fn row_item_margins_push_the_next_item() {
    let (doc, tree) = page(
        r#"<div id="row"><div id="a">abc</div><div id="b">de</div></div>"#,
        "#row { display: flex } #a { margin-left: 10px; margin-right: 24px }",
        1000.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap().rect;
    let b = tree.get(id(&doc, "b")).unwrap().rect;
    assert_eq!((a.x, a.width), (10.0, 24.0));
    assert_eq!(b.x, 10.0 + 24.0 + 24.0);
}

/// Margins count against the free space a growing item fills.
#[test]
fn row_item_margins_come_out_of_the_free_space() {
    let (doc, tree) = page(
        r#"<div id="row"><div id="a"></div><div id="b"></div></div>"#,
        "#row { display: flex; width: 400px } \
         #a { flex-grow: 1; margin-right: 20px } #b { width: 100px; margin-left: 30px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 250.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.x, 300.0);
}

/// Margins count as overflow a shrinking line has to give up.
#[test]
fn row_item_margins_count_as_overflow() {
    let (doc, tree) = page(
        r#"<div id="row"><div id="a"></div><div id="b"></div></div>"#,
        "#row { display: flex; width: 300px } \
         #a { width: 200px; margin-right: 100px; min-width: 0 } #b { width: 200px; min-width: 0 }",
        400.0,
    );
    // 400px of items + 100px of margin in 300: each gives up 100.
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 100.0);
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.x, 200.0);
}

/// rust-lang.org's two-column rows: a `max-width` item freezes at its
/// maximum (CSS Flexbox 9.7) and the other shrinks into the rest.
#[test]
fn a_max_width_row_item_freezes_and_the_other_takes_the_rest() {
    let long = "word ".repeat(200);
    for max in ["500px", "50%"] {
        let (doc, tree) = page(
            &format!(r#"<div id="row"><div id="a">{long}</div><div id="b">{long}</div></div>"#),
            &format!("#row {{ display: flex }} #a {{ max-width: {max} }}"),
            1000.0,
        );
        assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 500.0, "{max}");
        let b = tree.get(id(&doc, "b")).unwrap().rect;
        assert_eq!((b.x, b.width), (500.0, 500.0), "{max}");
    }
}

/// A grow factor sum under 1 takes only that share of the free space.
#[test]
fn fractional_grow_factors_take_a_share_of_the_free_space() {
    let (doc, tree) = page(
        r#"<div id="row"><div id="a"></div></div>"#,
        "#row { display: flex; width: 400px } #a { width: 100px; flex-grow: 0.5 }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "a")).unwrap().rect.width, 250.0);
}

/// `margin-left: auto` takes the free space, pushing the item to the end.
#[test]
fn an_auto_margin_row_item_takes_the_free_space() {
    let (doc, tree) = page(
        r#"<div id="row"><div id="a"></div><div id="b"></div><div id="c"></div></div>"#,
        "#row { display: flex; width: 400px; justify-content: center } \
         #a, #b, #c { width: 50px } #b { margin-left: auto } #c { margin: 0 auto }",
        400.0,
    );
    // 250px free over three auto margins: b's left, c's left and right.
    let x = |name| tree.get(id(&doc, name)).unwrap().rect.x;
    assert_eq!(x("a"), 0.0);
    assert!((x("b") - (50.0 + 250.0 / 3.0)).abs() < 0.01);
    assert!((x("c") - (100.0 + 500.0 / 3.0)).abs() < 0.01);
}

/// An em space doesn't collapse, but a line may still break after it
/// (UAX #14 class BA), with no space added at the break.
#[test]
fn a_line_breaks_after_a_wide_space() {
    // MetricShaper: 8px per character. "aaa\u{2003}" is 32, "bbb" 24.
    let (_doc, tree) = page(
        "<div>aaa\u{2003}bbb ccc</div>",
        "div { width: 50px }",
        400.0,
    );
    let ys: Vec<f32> = text_lines(&tree).iter().map(|(_, y)| *y).collect();
    assert_eq!(
        ys.last().copied(),
        Some(2.0 * 19.2),
        "{:?}",
        text_lines(&tree)
    );
    // On one line the pieces touch: the em space is the only space.
    let (_doc, tree) = page("<div>aaa\u{2003}bbb</div>", "", 400.0);
    let drawn = drawn_x(&tree);
    assert_eq!(drawn.last().unwrap().1, 32.0, "{drawn:?}");
    // A figure space (U+2007) is no break.
    let (_doc, tree) = page("<div>aaa\u{2007}bbb</div>", "div { width: 50px }", 400.0);
    assert!(text_lines(&tree).iter().all(|(_, y)| *y == 0.0));
}

/// The break after an em space narrows the min-content width too.
#[test]
fn min_content_breaks_after_a_wide_space() {
    let (doc, tree) = page(
        "<table><tr><td id=c>aaaa\u{3000}bb</table>",
        "td { padding: 0 }",
        10.0,
    );
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.width, 40.0);
}

/// `break_pieces` splits a word after each breaking space.
#[test]
fn breaking_spaces_split_words() {
    assert_eq!(
        noonoo_text::break_pieces("a\u{2003}b\u{3000}c\u{2007}d").collect::<Vec<_>>(),
        ["a\u{2003}", "b\u{3000}", "c\u{2007}d"]
    );
}

/// HN's "(example.com)" wrapped: a shrink-to-fit box's max-content width
/// dropped the space between text and an inline element.
#[test]
fn max_content_keeps_the_space_beside_an_inline_element() {
    // MetricShaper: 8px per character, spaces included.
    for html in [
        "xx <span>yy</span>",
        "xx<span> yy</span>",
        "<span>xx </span>yy",
        "xx <span> </span> yy",
        "xx <span><a>yy</a></span> ",
        " <span>xx</span> yy ",
    ] {
        let (doc, tree) = page(
            &format!(r#"<div id="f">{html}</div>"#),
            "#f { float: left }",
            400.0,
        );
        let f = tree.get(id(&doc, "f")).unwrap().rect;
        assert_eq!((f.width, f.height), (40.0, 19.2), "{html}");
    }
}

/// An inline element's edges count in max-content; its `width` doesn't.
#[test]
fn max_content_counts_inline_edges_not_inline_width() {
    let (doc, tree) = page(
        r#"<div id="f">xx <span>yy</span></div>"#,
        "#f { float: left } span { padding: 0 3px; border-left: 2px solid; width: 300px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.width, 48.0);
}

/// A `<br>` ends the run: the widest line is the max-content width.
#[test]
fn max_content_breaks_at_br() {
    let (doc, tree) = page(
        r#"<div id="f">aa <span>b</span><br>cccc</div>"#,
        "#f { float: left }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "f")).unwrap().rect.width, 32.0);
}

/// Text that touches an inline element is one word: no break between.
#[test]
fn min_content_joins_words_across_inline_elements() {
    let (doc, tree) = page(
        "<table><tr><td id=c>xx<span>yy</span>z a</table>",
        "td { padding: 0 }",
        10.0,
    );
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.width, 40.0);
}

/// A flex row's min-content sums its items with gaps between them; no
/// empty entries for white space between items add phantom gaps.
#[test]
fn a_flex_rows_min_content_has_no_phantom_gaps() {
    let (doc, tree) = page(
        "<table><tr><td id=c><div id=r> <div id=a></div> <div id=b></div> </div></table>",
        "td { padding: 0 } #r { display: flex; column-gap: 4px } #a, #b { width: 30px }",
        10.0,
    );
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.width, 64.0);
}

/// CSS 2 §9.4.2: a line with no text, no preserved white space and no
/// inline element with edges is a phantom: it takes no height and margins
/// collapse past it, out of the container as if it weren't there.
#[test]
fn an_empty_inline_run_does_not_stop_margin_collapsing() {
    // The div's height is the p's line alone: both p margins collapse out.
    for html in [
        "<div id=d><span></span><p>e</p></div>",
        "<div id=d><p>e</p><span></span></div>",
        "<div id=d><b> </b><p>e</p></div>",
        "<div id=d><a name=top></a>\n<p>e</p></div>",
        "<div id=d><span><i></i> </span> <p>e</p> <em></em></div>",
    ] {
        let (doc, tree) = page(html, "p { margin: 16px 0 }", 400.0);
        let d = tree.get(id(&doc, "d")).unwrap().rect;
        assert_eq!((d.y, d.height), (16.0, 19.2), "{html}");
    }
}

/// Content that does make a line still stops collapsing: an image, a
/// `<br>`, an inline with padding.
#[test]
fn a_line_with_content_still_stops_margin_collapsing() {
    for html in [
        "<div id=d><img><p>e</p></div>",
        "<div id=d><br><p>e</p></div>",
        "<div id=d><span style='padding-left:1px'></span><p>e</p></div>",
    ] {
        let (doc, tree) = page(html, "p { margin: 16px 0 }", 400.0);
        let d = tree.get(id(&doc, "d")).unwrap().rect;
        assert_eq!(d.y, 0.0, "{html}");
    }
}

/// `white-space: pre-line` collapses spaces too: spaces alone between
/// blocks make no line. A newline in it is a line break.
#[test]
fn pre_line_spaces_between_blocks_make_no_line() {
    let (doc, tree) = page(
        "<div id=d><p>a</p>   <p>b</p></div>",
        "#d { white-space: pre-line } p { margin: 16px 0 }",
        400.0,
    );
    let d = tree.get(id(&doc, "d")).unwrap().rect;
    assert_eq!((d.y, d.height), (16.0, 2.0 * 19.2 + 16.0));
}

/// Only a block formatting context root grows to hold its floats (CSS 2
/// §10.6.7); a plain block's height is its in-flow content alone.
#[test]
fn a_plain_block_does_not_grow_to_hold_its_floats() {
    let (doc, tree) = page(
        "<div id=p><div id=f></div></div><div id=n>x</div>",
        "#f { float: left; width: 10px; height: 50px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "p")).unwrap().rect.height, 0.0);
    // The next block starts at the top; its line flows beside the float.
    let n = tree.get(id(&doc, "n")).unwrap().rect;
    assert_eq!(n.y, 0.0);
}

/// A BFC root holds its floats, nested ones too: `flow-root`, a clipping
/// box, an inline-block.
#[test]
fn a_bfc_root_grows_to_hold_its_floats() {
    for root in [
        "display: flow-root",
        "overflow: hidden",
        "display: inline-block",
    ] {
        let (doc, tree) = page(
            "<div id=r><div><div id=f></div></div></div>",
            &format!("#r {{ {root} }} #f {{ float: left; width: 10px; height: 50px }}"),
            400.0,
        );
        assert_eq!(tree.get(id(&doc, "r")).unwrap().rect.height, 50.0, "{root}");
    }
}

/// `display: flow-root` is a block box: full width, its own context, so
/// its children's margins stay inside.
#[test]
fn flow_root_is_a_block_that_keeps_child_margins() {
    let (doc, tree) = page(
        "<div id=r><p>x</p></div>",
        "#r { display: flow-root } p { margin: 10px 0 }",
        400.0,
    );
    let r = tree.get(id(&doc, "r")).unwrap().rect;
    assert_eq!((r.y, r.width, r.height), (0.0, 400.0, 39.2));
}

fn body_and_root(doc: &Document, tree: &LayoutTree) -> (Rect, Rect) {
    let body = doc.get_elements_by_tag_name("body")[0];
    let html = doc.get_elements_by_tag_name("html")[0];
    (tree.get(body).unwrap().rect, tree.get(html).unwrap().rect)
}

/// CSS Quirks §3.6–3.7: in quirks mode the root fills the viewport and an
/// auto-height body fills the root, less its margins.
#[test]
fn in_quirks_mode_html_and_body_fill_the_viewport() {
    let (doc, tree) = page("<p>x</p>", "body { margin: 8px }", 400.0);
    let (body, html) = body_and_root(&doc, &tree);
    assert_eq!(html.height, 600.0);
    assert_eq!((body.y, body.height), (8.0, 584.0));
    // Content taller than the viewport still wins. A filling body keeps its
    // last child's bottom margin inside (not checked against a browser when
    // the content outgrows the fill).
    let (doc, tree) = page("<p>x</p>", "p { height: 900px }", 400.0);
    assert_eq!(body_and_root(&doc, &tree).0.height, 916.0);
    // A set height, or standards mode, doesn't fill.
    let (doc, tree) = page("<p>x</p>", "body { height: 50px }", 400.0);
    assert_eq!(body_and_root(&doc, &tree).0.height, 50.0);
    let (doc, tree) = page("<!doctype html><p>x</p>", "", 400.0);
    assert_eq!(body_and_root(&doc, &tree).0.height, 19.2);
}

/// CSS Quirks §3.5: a percentage height looks past auto-height blocks to
/// the nearest ancestor with a height (the viewport at the top).
#[test]
fn in_quirks_mode_percentage_heights_skip_auto_height_blocks() {
    let (doc, tree) = page(
        "<div><div id=d></div></div><div style='height:200px'><div><div id=e></div></div></div>",
        "#d, #e { height: 50% }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "d")).unwrap().rect.height, 300.0);
    assert_eq!(tree.get(id(&doc, "e")).unwrap().rect.height, 100.0);
    // Standards mode: auto-height parents leave it auto.
    let (doc, tree) = page(
        "<!doctype html><div><div id=d></div></div>",
        "#d { height: 50% }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "d")).unwrap().rect.height, 0.0);
}

/// The root element is a formatting context root (CSS 2 §9.4.1): body's
/// margin collapses with its first child's but never out of `<html>`, so
/// the root's box starts at the top of the canvas.
#[test]
fn margins_do_not_collapse_out_of_the_root() {
    let (doc, tree) = page_with_ua_body("<!doctype html><p>x</p>", 400.0);
    let (body, html) = body_and_root(&doc, &tree);
    assert_eq!((html.y, html.height), (0.0, 16.0 + 19.2 + 16.0));
    assert_eq!(body.y, 16.0);
}

/// In quirks mode a table cell's `height` is its border box: padding is
/// inside it. Standards mode adds the padding.
#[test]
fn in_quirks_mode_a_cells_height_includes_its_padding() {
    let css = "td { height: 50px; padding: 10px; border: 0 }";
    let html = "<table><tr><td id=c>x</td></tr></table>";
    let (doc, tree) = page(html, css, 400.0);
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.height, 50.0);
    let (doc, tree) = page(&format!("<!doctype html>{html}"), css, 400.0);
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.height, 70.0);
}

/// The y of the text box drawing `text`.
fn text_y(tree: &LayoutTree, text: &str) -> f32 {
    tree.boxes()
        .iter()
        .find(|b| b.kind == BoxKind::Text && b.text.as_deref() == Some(text))
        .unwrap_or_else(|| panic!("no text box {text:?}"))
        .rect
        .y
}

// MetricShaper's default metrics: ascent 2/3 and descent 1/3 of its
// 1.2em line, so 12.8 and 6.4 at 16px.

/// CSS 2 §10.6.1: an inline's padding, border and background wrap its
/// content area (the font's ascent + descent) around the baseline, not
/// the line height.
#[test]
fn an_inline_box_wraps_the_content_area_not_the_line_height() {
    let (doc, tree) = page(
        "<div style='line-height:40px'><span id=s style='padding:2px'>x</span></div>",
        "",
        400.0,
    );
    // Half the 40px line's leading (10.4) sits above the content area.
    let s = tree.get(id(&doc, "s")).unwrap().rect;
    assert!(
        (s.y - 8.4).abs() < 0.01 && (s.height - 23.2).abs() < 0.01,
        "{s:?}"
    );
}

/// Mixed font sizes share a baseline (CSS 2 §10.8).
#[test]
fn mixed_font_sizes_share_a_baseline() {
    let (doc, tree) = page(
        "<div id=d>a <span style='font-size:40px'>BIG</span> b</div>",
        "",
        400.0,
    );
    // BIG's 48px line: ascent 32 above the baseline, descent 16 below.
    assert_eq!(text_y(&tree, "BIG"), 0.0);
    assert!((text_y(&tree, "a") - 19.2).abs() < 0.01);
    assert_eq!(tree.get(id(&doc, "d")).unwrap().rect.height, 48.0);
}

/// An inline-block with no lines sits on the baseline by its bottom
/// margin edge, leaving the descent below it.
#[test]
fn an_empty_inline_block_sits_on_the_baseline() {
    let (doc, tree) = page(
        "<div id=d>x <span id=i></span> y</div>",
        "#i { display: inline-block; width: 10px; height: 50px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "i")).unwrap().rect.y, 0.0);
    assert!((text_y(&tree, "x") - 37.2).abs() < 0.01);
    assert!((tree.get(id(&doc, "d")).unwrap().rect.height - 56.4).abs() < 0.01);
}

/// An inline-block with text aligns its last line's baseline; a clipping
/// one uses its bottom margin edge.
#[test]
fn an_inline_block_aligns_its_last_lines_baseline() {
    let (doc, tree) = page(
        "<div id=d>x <span id=b>y</span></div>",
        "#b { display: inline-block; padding: 5px }",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "b")).unwrap().rect.y, 0.0);
    assert!((text_y(&tree, "x") - 5.0).abs() < 0.01);
    assert!((tree.get(id(&doc, "d")).unwrap().rect.height - 29.2).abs() < 0.01);

    let (doc, tree) = page(
        "<div id=d>x <span id=b>y</span></div>",
        "#b { display: inline-block; padding: 5px; overflow: hidden }",
        400.0,
    );
    // Bottom edge on the baseline: 29.2 above it, the strut's 6.4 below.
    assert!((tree.get(id(&doc, "d")).unwrap().rect.height - 35.6).abs() < 0.01);
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

/// `sup` rises a third of its parent's font size plus a pixel (as Blink),
/// in a 0.8em font with `line-height: normal`.
#[test]
fn sup_and_sub_shift_their_text() {
    // sup: 12.8px font, ascent 10.24, raised 16/3 + 1.
    let (doc, tree) = page("<!doctype html><div id=d>x<sup>2</sup></div>", "", 400.0);
    let up = 10.24 + 16.0 / 3.0 + 1.0;
    assert!(near(text_y(&tree, "2"), 0.0), "{}", text_y(&tree, "2"));
    assert!(near(text_y(&tree, "x"), up - 12.8));
    assert!(near(tree.get(id(&doc, "d")).unwrap().rect.height, up + 6.4));
    // sub: lowered 16/5 + 1 = 4.2; the strut still sets the top.
    let (doc, tree) = page("<!doctype html><div id=d>x<sub>2</sub></div>", "", 400.0);
    assert!(near(text_y(&tree, "x"), 0.0));
    assert!(near(text_y(&tree, "2"), 12.8 - 10.24 + 4.2));
    assert!(near(
        tree.get(id(&doc, "d")).unwrap().rect.height,
        12.8 + 5.12 + 4.2
    ));
}

/// `middle` centres a box on the baseline plus half the x-height.
#[test]
fn middle_centres_on_the_x_height() {
    let (doc, tree) = page(
        "<div id=d>x <span id=b></span></div>",
        "#b { display: inline-block; width: 10px; height: 30px; vertical-align: middle }",
        400.0,
    );
    // Its middle 4px (half a 0.5em x-height) above the baseline: 19 of it
    // above, 11 below.
    assert!(near(tree.get(id(&doc, "b")).unwrap().rect.y, 0.0));
    assert!(near(text_y(&tree, "x"), 19.0 - 12.8));
    assert!(near(tree.get(id(&doc, "d")).unwrap().rect.height, 30.0));
}

/// A length raises (or lowers) a box from the baseline.
#[test]
fn a_length_raises_a_box() {
    let (doc, tree) = page(
        "<div id=d>x <span id=b></span></div>",
        "#b { display: inline-block; width: 10px; height: 20px; vertical-align: 10px }",
        400.0,
    );
    assert!(near(tree.get(id(&doc, "b")).unwrap().rect.y, 0.0));
    assert!(near(text_y(&tree, "x"), 30.0 - 12.8));
    assert!(near(tree.get(id(&doc, "d")).unwrap().rect.height, 36.4));
}

/// `top` and `bottom` align with the line box itself, which grows to hold
/// them.
#[test]
fn top_and_bottom_align_with_the_line_box() {
    let (doc, tree) = page(
        "<div id=d>x <span id=t></span><span id=b></span></div>",
        "#t, #b { display: inline-block; width: 10px }
         #t { height: 40px; vertical-align: top } #b { height: 30px; vertical-align: bottom }",
        400.0,
    );
    assert!(near(tree.get(id(&doc, "d")).unwrap().rect.height, 40.0));
    assert!(near(tree.get(id(&doc, "t")).unwrap().rect.y, 0.0));
    assert!(near(tree.get(id(&doc, "b")).unwrap().rect.y, 10.0));
    assert!(near(text_y(&tree, "x"), 0.0));
}

/// `text-top` and `text-bottom` align with the parent's content area.
#[test]
fn text_top_and_text_bottom_align_with_the_parents_font() {
    let (doc, tree) = page(
        "<div>x <span id=t></span><span id=b></span></div>",
        "#t, #b { display: inline-block; width: 10px; height: 10px }
         #t { vertical-align: text-top } #b { vertical-align: text-bottom }",
        400.0,
    );
    assert!(near(tree.get(id(&doc, "t")).unwrap().rect.y, 0.0));
    assert!(near(tree.get(id(&doc, "b")).unwrap().rect.y, 9.2));
}

fn rect_of(doc: &Document, tree: &LayoutTree, name: &str) -> Rect {
    tree.get(id(doc, name)).unwrap().rect
}

/// HTML's UA sheet: tables space their cells 2px apart and from the
/// table's edges (`border-spacing`, CSS 2 §17.6.1).
#[test]
fn table_cells_are_spaced_by_border_spacing() {
    let html = "<table id=t><tr><td id=a>x</td><td id=b>y</td></tr></table>";
    let (doc, tree) = page(html, "td { padding: 0 }", 400.0);
    assert_eq!(rect_of(&doc, &tree, "a").x, 2.0);
    assert_eq!(rect_of(&doc, &tree, "a").y, 2.0);
    assert_eq!(rect_of(&doc, &tree, "b").x, 12.0);
    let t = rect_of(&doc, &tree, "t");
    assert!(near(t.width, 22.0) && near(t.height, 23.2), "{t:?}");

    // Two values: horizontal, then vertical.
    let (doc, tree) = page(
        html,
        "td { padding: 0 } table { border-spacing: 5px 10px }",
        400.0,
    );
    assert_eq!(rect_of(&doc, &tree, "b").x, 18.0);
    let t = rect_of(&doc, &tree, "t");
    assert!(near(t.width, 31.0) && near(t.height, 39.2), "{t:?}");

    // `cellspacing` sets it; collapsed borders have none.
    let spaced = "<table id=t cellspacing=0><tr><td id=a>x</td><td id=b>y</td></tr></table>";
    let (doc, tree) = page(spaced, "td { padding: 0 }", 400.0);
    assert_eq!(rect_of(&doc, &tree, "b").x, 8.0);
    let (doc, tree) = page(
        html,
        "td { padding: 0 } table { border-collapse: collapse }",
        400.0,
    );
    assert_eq!(rect_of(&doc, &tree, "b").x, 8.0);
}

/// A fixed-width table's columns share what the spacing leaves; a cell
/// spanning columns takes the spacing between them.
#[test]
fn spacing_comes_out_of_the_columns_and_into_spanning_cells() {
    let (doc, tree) = page(
        "<table width=100><tr><td id=a>x</td><td id=b>y</td></tr></table>",
        "td { padding: 0 }",
        400.0,
    );
    let (a, b) = (rect_of(&doc, &tree, "a"), rect_of(&doc, &tree, "b"));
    assert!(near(a.width + b.width, 94.0), "{a:?} {b:?}");
    assert!(near(b.x, 2.0 + a.width + 2.0));
    let (doc, tree) = page(
        "<table><tr><td id=w colspan=2>aaaaaaaaaa</td></tr><tr><td id=a>x</td><td id=b>y</td></tr></table>",
        "td { padding: 0 }",
        400.0,
    );
    let (w, a, b) = (
        rect_of(&doc, &tree, "w"),
        rect_of(&doc, &tree, "a"),
        rect_of(&doc, &tree, "b"),
    );
    assert!(near(w.width, 80.0), "{w:?}");
    assert!(near(w.width, a.width + 2.0 + b.width));
}

/// `<col>` and `<colgroup>` widths size their columns, from attributes or
/// CSS, across their `span`; a fixed column wraps its content to its width.
#[test]
fn col_and_colgroup_widths_size_their_columns() {
    let css = "table { border-spacing: 0 } td { padding: 0 }";
    let widths = |html: &str, names: &[&str]| {
        let (doc, tree) = page(html, css, 400.0);
        names
            .iter()
            .map(|name| rect_of(&doc, &tree, name).width)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        widths(
            r#"<table><col width=50><col style="width: 30px"><tr><td id=a>x<td id=b>y</table>"#,
            &["a", "b"]
        ),
        [50.0, 30.0]
    );
    assert_eq!(
        widths(
            "<table><colgroup width=40 span=2></colgroup><col><tr><td id=a>x<td id=b>y<td id=c>z</table>",
            &["a", "b", "c"]
        ),
        [40.0, 40.0, 8.0]
    );
    assert_eq!(
        widths(
            "<table><colgroup width=90><col span=2 width=20></colgroup><tr><td id=a>x<td id=b>y</table>",
            &["a", "b"]
        ),
        [20.0, 20.0]
    );
    assert_eq!(
        widths(
            r#"<table width=200><col width="25%"><tr><td id=a>x<td id=b>y</table>"#,
            &["a", "b"]
        ),
        [50.0, 150.0]
    );
    assert_eq!(
        widths(
            "<table><col width=40><tr><td id=a>aaaa bbbb cccc</table>",
            &["a"]
        ),
        [40.0]
    ); // A cell's pixel width holds its column the same way against a wider
    // neighbour in another row.
    assert_eq!(
        widths(
            "<table><tr><td id=a width=40>x<tr><td id=b>aaaa bbbb cccc</table>",
            &["a", "b"]
        ),
        [40.0, 40.0]
    );
}

/// A table with no width of its own widens until its percentage columns
/// get their share, as browsers' auto layout does: a 25% column holding
/// 8px needs a 32px table. Percentages that leave nothing for the other
/// columns widen it to its containing block.
#[test]
fn percentage_columns_widen_a_shrink_to_fit_table() {
    let css = "table { border-spacing: 0 } td { padding: 0 }";
    let rects = |html: &str, names: &[&str]| {
        let (doc, tree) = page(html, css, 400.0);
        names
            .iter()
            .map(|name| rect_of(&doc, &tree, name).width)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        rects(
            r#"<table id=t><tr><td id=a width="25%">x<td id=b>y</table>"#,
            &["t", "a", "b"]
        ),
        [32.0, 8.0, 24.0]
    );
    assert_eq!(
        rects(
            r#"<table id=t><tr><td id=a width="50%">x<td id=b width="50%">yyyy</table>"#,
            &["t", "a", "b"]
        ),
        [64.0, 32.0, 32.0]
    );
    assert_eq!(
        rects(
            r#"<table id=t><tr><td id=a>x<td id=b width="100%">y</table>"#,
            &["t", "a", "b"]
        ),
        [400.0, 8.0, 392.0]
    );
}

/// A shaper whose average character is 0.5em and widest 2em, as a real
/// font's `OS/2` average and bounding box differ.
struct WideGlyphs;

impl Shaper for WideGlyphs {
    fn advance(&self, text: &str, request: noonoo_text::FontRequest, font_size: f32) -> f32 {
        MetricShaper.advance(text, request, font_size)
    }

    fn line_height(&self, _request: noonoo_text::FontRequest, font_size: f32) -> f32 {
        font_size * 1.2
    }

    fn char_widths(&self, _request: noonoo_text::FontRequest, font_size: f32) -> (f32, f32) {
        (font_size * 0.5, font_size * 2.0)
    }
}

/// Chromium's text field: `size` average characters, plus the room the
/// widest character needs beyond an average one (size 1 at 16px of
/// DejaVu Sans is 45px wide, not 8).
#[test]
fn a_text_field_makes_room_for_its_widest_character() {
    let doc = noonoo_html::parse(r#"<input id="a" size="5"><input id="b">"#);
    let sheet = noonoo_css::Stylesheet::parse(
        "body { margin: 0 } input { padding: 0; border: 0; font-size: 16px }",
    )
    .unwrap();
    let styles = noonoo_style::compute(&doc, &[sheet]);
    let tree = layout(&doc, &styles, Size::new(400.0, 600.0), &WideGlyphs);
    let width = |name: &str| tree.get(id(&doc, name)).unwrap().content_box().width;
    // 5 × 8 + (32 − 8), and 20 × 8 + 24.
    assert_eq!(width("a"), 64.0);
    assert_eq!(width("b"), 184.0);
}

/// A textarea is `cols` average characters wide plus the scrollbar
/// browsers keep room for, and `rows` lines tall (20 × 2 by default),
/// whatever text it holds; CSS sizes still win.
#[test]
fn a_textarea_is_sized_by_cols_and_rows() {
    let (doc, tree) = page(
        r#"<textarea id="a" cols="10" rows="3">a very long value that would wrap onto many lines in ten columns</textarea><br><textarea id="b"></textarea><br><textarea id="c" cols="10" style="width: 50px; height: 20px"></textarea>"#,
        "textarea { padding: 0; border: 0; font-size: 16px }",
        400.0,
    );
    let size = |name: &str| {
        let rect = tree.get(id(&doc, name)).unwrap().content_box();
        (rect.width, rect.height)
    };
    // MetricShaper: an average character is 8px, a line 19.2px.
    let (width, height) = size("a");
    assert_eq!(width, 95.0);
    assert!(near(height, 57.6), "{height}");
    let (width, height) = size("b");
    assert_eq!(width, 175.0);
    assert!(near(height, 38.4), "{height}");
    assert_eq!(size("c"), (50.0, 20.0));
}

/// An image's `width` and `height` attributes size it before (or without)
/// its pixels, as presentational hints: the page doesn't jump when the
/// image arrives. Author CSS still wins.
#[test]
fn img_width_and_height_attributes_size_it_before_it_loads() {
    let (doc, tree) = page(
        r#"<img id="a" src="x.png" width="50" height="40"><img id="b" src="x.png" width="60" alt="pic"><img id="c" src="x.png" width="50" height="40" style="width: 20px">"#,
        "",
        400.0,
    );
    let size = |name: &str| {
        let rect = tree.get(id(&doc, name)).unwrap().content_box();
        (rect.width, rect.height)
    };
    assert_eq!(size("a"), (50.0, 40.0));
    assert_eq!(size("b").0, 60.0);
    assert_eq!(size("c"), (20.0, 40.0));
}

/// A collapsed space at the start of an inline element belongs to it:
/// Chromium's link starts at the space and is a space wider
/// (info.cern.ch's `<A ...>\nhypermedia</A>`, lite.cnn's `<span> | </span>`).
#[test]
fn a_leading_space_inside_an_inline_is_part_of_it() {
    let (doc, tree) = page(
        "<p>a<span id=s> b</span> c<span id=t> | </span>d</p>",
        "p { margin: 0 }",
        400.0,
    );
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    // MetricShaper: 8px a character.
    assert_eq!((rect("s").x, rect("s").width), (8.0, 16.0));
    // "a b c" then " | " (its trailing space collapses into nothing after it,
    // but it is the span's own), then "d".
    assert_eq!((rect("t").x, rect("t").width), (40.0, 24.0)); // Outside the span, the space stays outside; a padded span spans its
    // padding and its space.
    let (doc, tree) = page(
        r#"<p>a <span id=u>b</span><span id=v style="padding: 0 5px"> c </span>d</p>"#,
        "p { margin: 0 }",
        400.0,
    );
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    assert_eq!((rect("u").x, rect("u").width), (16.0, 8.0));
    assert_eq!((rect("v").x, rect("v").width), (24.0, 34.0));
}

/// A table's `width` is its border box, as Chromium's UA sheet sets
/// `box-sizing: border-box` on tables (HN's `<table width=100%
/// style="padding:2px">` fills its cell, no wider).
#[test]
fn a_tables_width_includes_its_padding_and_border() {
    let (doc, tree) = page(
        r#"<div style="width: 200px"><table id=t width="100%" style="padding: 2px; border: 3px solid"><tr><td>x</td></tr></table><table id=u style="width: 100px; padding: 5px"><tr><td>x</td></tr></table></div>"#,
        "",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "t")).unwrap().rect.width, 200.0);
    assert_eq!(tree.get(id(&doc, "u")).unwrap().rect.width, 100.0);
}

/// A cell is as tall as its row, whatever its own `height` (HN's header
/// cell asks for 10px beside a 20px logo cell).
#[test]
fn a_cell_stretches_to_its_row_past_its_own_height() {
    let (doc, tree) = page(
        r#"<table style="border-spacing: 0"><tr><td style="height: 20px; padding: 0">a</td><td id=c style="height: 10px; padding: 0">b</td></tr></table>"#,
        "",
        400.0,
    );
    assert_eq!(tree.get(id(&doc, "c")).unwrap().rect.height, 20.0);
}

/// A cell's content sits in the middle of its row by default, or at the
/// top or bottom as `vertical-align` (or `valign`) says; the cell's own
/// box still fills the row.
#[test]
fn cell_content_aligns_vertically_in_its_row() {
    let (doc, tree) = page(
        r#"<table style="border-spacing: 0"><tr><td style="height: 40px">a</td><td id=m>mid</td><td valign=top>top</td><td style="vertical-align: bottom">bot</td></tr></table>"#,
        "td { padding: 0 }",
        400.0,
    );
    // MetricShaper: a line is 19.2px; the row 40px.
    assert!(near(text_y(&tree, "mid"), 10.4), "{}", text_y(&tree, "mid"));
    assert!(near(text_y(&tree, "top"), 0.0));
    assert!(near(text_y(&tree, "bot"), 20.8), "{}", text_y(&tree, "bot"));
    assert_eq!(tree.get(id(&doc, "m")).unwrap().rect.height, 40.0);
}

/// A row is at least its own `height`, cells or not (HN's
/// `<tr style='height:10px'/>` spacers).
#[test]
fn a_rows_height_holds_even_when_empty() {
    let (doc, tree) = page(
        r#"<table style="border-spacing: 0"><tr style="height: 10px"></tr><tr style="height: 30px"><td id=c style="padding: 0">x</td></tr><tr><td id=d>y</td></tr></table>"#,
        "",
        400.0,
    );
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    assert_eq!((rect("c").y, rect("c").height), (10.0, 30.0));
    assert_eq!(rect("d").y, 40.0);
}

/// A collapsed space is as wide as a space in the font of the text it sits
/// in, not the paragraph's (HN's small `(site)` after a title wrapped a
/// line Chromium kept whole).
#[test]
fn a_space_takes_the_width_of_its_own_font() {
    let (doc, tree) = page(
        r#"<p>a<span id=s style="font-size: 8px"> b</span> <b id=w>c</b></p>"#,
        "p { margin: 0; font-size: 16px } b { font-weight: normal }",
        400.0,
    );
    // MetricShaper: half the font size a character.
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    assert_eq!((rect("s").x, rect("s").width), (8.0, 8.0));
    // The space after the span is the paragraph's: 8px.
    assert_eq!(rect("w").x, 24.0);
}

/// Quirks mode's line height quirk: a line with no text has no strut, so a
/// lone image's line is the image (HN's `<img height=10>` spacer before its
/// footer). In standards mode the strut holds the line open as before.
#[test]
fn in_quirks_mode_a_line_without_text_has_no_strut() {
    let html =
        r#"<div id=d><img width=10 height=10></div><div id=e><img width=10 height=10> x</div>"#;
    let (doc, tree) = page(html, "", 400.0);
    let height = |tree: &LayoutTree, doc: &Document, name: &str| {
        tree.get(id(doc, name)).unwrap().rect.height
    };
    assert_eq!(height(&tree, &doc, "d"), 10.0);
    // Text on the line brings the strut back.
    assert!(
        near(height(&tree, &doc, "e"), 19.2),
        "{}",
        height(&tree, &doc, "e")
    );
    let (doc, tree) = page(&format!("<!doctype html>{html}"), "", 400.0);
    assert!(
        near(height(&tree, &doc, "d"), 19.2),
        "{}",
        height(&tree, &doc, "d")
    );
}

/// A cell centres its content by the height its layout gave it, margins it
/// contains included: HN's footer cell ends in a form whose quirks-mode
/// bottom margin stays in the cell, leaving no room to centre in.
#[test]
fn a_cell_centres_its_content_margins_included() {
    let (_, tree) = page(
        r#"<table style="border-spacing: 0"><tr><td style="height: 60px; padding: 0">a</td><td style="padding: 0"><div><p style="margin: 0 0 20px">x</p></div></td></tr></table>"#,
        "",
        400.0,
    );
    // 19.2px of line and 20px of margin in a 60px row: 20.8px to share.
    assert!(near(text_y(&tree, "x"), 10.4), "{}", text_y(&tree, "x"));
}

/// The quirk counts only text an inline box holds itself: small text in a
/// span doesn't bring back its block's larger strut (HN's footer links).
#[test]
fn in_quirks_mode_only_direct_text_brings_back_a_strut() {
    let html = r#"<div id=a style="font-size: 20px"><span style="font-size: 10px">x</span></div><div id=b style="font-size: 20px">y<span style="font-size: 10px">x</span></div><div id=c style="font-size: 20px"><span style="font-size: 10px"><b style="font-size: 5px">z</b></span></div>"#;
    let (doc, tree) = page(html, "b { font-weight: normal }", 400.0);
    let height = |name: &str| tree.get(id(&doc, name)).unwrap().rect.height;
    // MetricShaper: a line is 1.2em.
    assert!(near(height("a"), 12.0), "{}", height("a"));
    assert!(near(height("b"), 24.0), "{}", height("b"));
    // The span holds no text itself: only the 5px text's box counts.
    assert!(near(height("c"), 6.0), "{}", height("c"));
}

/// A `border-box` size never squeezes the padding and border: the content
/// box is at least empty, so the box is at least its edges.
#[test]
fn a_border_box_size_is_at_least_its_padding_and_border() {
    let (doc, tree) = page(
        r#"<div id=d style="box-sizing: border-box; width: 10px; height: 10px; padding: 8px; border: 1px solid"></div>"#,
        "",
        400.0,
    );
    let rect = tree.get(id(&doc, "d")).unwrap().rect;
    assert_eq!((rect.width, rect.height), (18.0, 18.0));
}

/// A checkbox or radio's baseline is the bottom of its border box, as in
/// Chromium: its bottom margin hangs below the line's baseline instead of
/// lifting it (Wikipedia's menu label sat 3px low).
#[test]
fn a_checkboxs_baseline_is_its_border_box_bottom() {
    let (doc, tree) = page(
        r#"<!doctype html><div id=d><input type=checkbox>x</div>"#,
        "body { margin: 0 } div { font-size: 16px }",
        400.0,
    );
    // MetricShaper strut: 12.8 above, 6.4 below; the checkbox 3 + 13 above
    // and its 3px margin below.
    assert!(near(text_y(&tree, "x"), 3.2), "{}", text_y(&tree, "x"));
    assert!(near(tree.get(id(&doc, "d")).unwrap().rect.height, 22.4));
}

/// An image that hasn't loaded but has a size shows its alt text clipped
/// in that size, as Chromium does, so its baseline is its bottom edge, not
/// the alt text's (Wikipedia's wordmark and tagline sat top-aligned).
#[test]
fn a_sized_unloaded_image_sits_on_its_bottom_edge() {
    let (doc, tree) = page(
        r#"<!doctype html><div><img id=a alt="Wikipedia" style="width: 140px; height: 22px"><img id=b alt="The Free Encyclopedia" width=140 height=11></div>"#,
        "body { margin: 0 }",
        400.0,
    );
    let bottom = |name| {
        let rect = tree.get(id(&doc, name)).unwrap().rect;
        rect.y + rect.height
    };
    assert!(
        near(bottom("a"), bottom("b")),
        "{} vs {}",
        bottom("a"),
        bottom("b")
    );
}

/// A box the margins collapse through sits where it would with a bottom
/// border (CSS 2 §8.3.1): after the margin before it and its own leading
/// margins, as Chromium places Wikipedia's empty menus. Content after it
/// doesn't move.
#[test]
fn a_collapsed_through_box_sits_below_its_top_margin() {
    let (doc, tree) = page(
        r#"<!doctype html><div>a</div><div id=e><ul id=u></ul></div><div id=n>b</div>"#,
        "body { margin: 0 } div { font-size: 16px }",
        400.0,
    );
    let y = |name| tree.get(id(&doc, name)).unwrap().rect.y;
    assert!(near(y("e"), 35.2), "{}", y("e"));
    assert!(near(y("u"), 35.2), "{}", y("u"));
    assert!(near(y("n"), 35.2), "{}", y("n"));
}

/// Embedded content that hasn't loaded keeps the ratio of its `width` and
/// `height` attributes on an auto axis (HTML maps them to `aspect-ratio:
/// auto w / h`), as gnu.org's video does with `height: auto`.
#[test]
fn unloaded_media_keeps_its_attribute_ratio() {
    let (doc, tree) = page(
        r#"<!doctype html><video id=v width=512 height=288></video><img id=i width=40 height=30 alt="x">"#,
        "body { margin: 0 } video, img { display: block; width: 256px; height: auto; padding: 2px }",
        400.0,
    );
    let content = |name| tree.get(id(&doc, name)).unwrap().content_box().height;
    assert!(near(content("v"), 144.0), "{}", content("v"));
    assert!(near(content("i"), 192.0), "{}", content("i"));
}

/// A block-level replaced element is never collapsed through, even when
/// its children (a video's `source`s and `track`s) show nothing: the
/// content after it starts below it, as on gnu.org.
#[test]
fn a_block_video_holds_its_place() {
    let (doc, tree) = page(
        r#"<!doctype html><video id=v width=512 height=288><source src=a.webm><track src=a.vtt></video><p id=p>x</p>"#,
        "body { margin: 0 } video { display: block; width: 256px; height: auto } source, track { display: none } p { margin: 0 }",
        400.0,
    );
    let y = tree.get(id(&doc, "p")).unwrap().rect.y;
    assert!(near(y, 144.0), "{y}");
}

/// An image that hasn't loaded and has a percentage width has no natural
/// size, so its alt text adds nothing to a shrink-to-fit width: the box
/// shrinks to its other content and the image takes its share of that, as
/// in Chromium (gnu.org's distro thumbnails).
#[test]
fn a_percentage_width_unloaded_image_adds_no_intrinsic_width() {
    let (doc, tree) = page(
        r#"<!doctype html><div id=d><img alt="a long description of the screenshot"><br>xx</div>"#,
        "body { margin: 0 } div { display: inline-block; font-size: 16px } img { width: 50% }",
        800.0,
    );
    let width = tree.get(id(&doc, "d")).unwrap().rect.width;
    assert!(near(width, 16.0), "{width}");
}

/// A float that doesn't fit beside an earlier one drops below it, wherever
/// the line is: the fit test once rounded away below y = 2 (`y +
/// f32::EPSILON == y`), so gnu.org's two columns overlapped instead of
/// stacking as in Chromium.
#[test]
fn a_float_too_wide_for_the_line_drops_below_the_last() {
    let text = "word ".repeat(80);
    let (doc, tree) = page(
        &format!(
            r#"<!doctype html><div><div id=a style="float: left">{text}</div><div id=b style="float: right">word word</div></div>"#
        ),
        "body { margin: 8px }",
        400.0,
    );
    let a = tree.get(id(&doc, "a")).unwrap().rect;
    let b = tree.get(id(&doc, "b")).unwrap().rect;
    assert!(near(b.y, a.y + a.height), "{a:?} {b:?}");
}

/// A shaper whose primary face is 15 + 4 px at 16px, with non-ASCII text
/// drawn from a taller fallback (17 + 7 px), as DejaVu Sans and Noto Sans
/// Thai are in Chromium's probe.
struct FallbackFont;

impl Shaper for FallbackFont {
    fn advance(&self, text: &str, request: noonoo_text::FontRequest, font_size: f32) -> f32 {
        MetricShaper.advance(text, request, font_size)
    }

    fn line_height(&self, _request: noonoo_text::FontRequest, font_size: f32) -> f32 {
        font_size * 19.0 / 16.0
    }

    fn metrics(&self, _request: noonoo_text::FontRequest, font_size: f32) -> (f32, f32) {
        (font_size * 15.0 / 16.0, font_size * 4.0 / 16.0)
    }

    fn text_metrics(
        &self,
        text: &str,
        request: noonoo_text::FontRequest,
        font_size: f32,
    ) -> (f32, f32) {
        if text.is_ascii() {
            self.metrics(request, font_size)
        } else {
            (font_size * 17.0 / 16.0, font_size * 7.0 / 16.0)
        }
    }
}

/// A `normal`-height line grows to hold text from a taller fallback face,
/// and its baseline drops to fit the face's ascent, as Chromium lays out
/// Thai, Bengali or CJK beside DejaVu Sans; a fixed line height doesn't.
#[test]
fn a_normal_line_holds_its_fallback_faces() {
    let doc = noonoo_html::parse(
        r#"<!doctype html><div id=a>abc</div><div id=t>ไทย</div><div id=m>Abc <span>ไทย</span> def</div><div id=f style="line-height: 19px">ไทย</div>"#,
    );
    let sheet =
        noonoo_css::Stylesheet::parse("body { margin: 0 } div { font-size: 16px }").unwrap();
    let styles = noonoo_style::compute(&doc, &[sheet]);
    let tree = layout(&doc, &styles, Size::new(400.0, 600.0), &FallbackFont);
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    assert_eq!(rect("a").height, 19.0);
    assert_eq!(rect("t").height, 24.0);
    assert_eq!(rect("m").height, 24.0);
    assert_eq!(rect("f").height, 19.0);
    // The text box keeps the primary face's height, 2px below the line top.
    assert!(
        near(text_y(&tree, "ไทย") - rect("t").y, 2.0),
        "{}",
        text_y(&tree, "ไทย")
    );
    assert!(near(text_y(&tree, "Abc") - rect("m").y, 2.0));
}

/// An empty inline element still gets a zero-width box where it sits on
/// its line, as Chromium reports Hacker News's empty `unv_` spans, but it
/// never makes a line of its own or changes one's height.
#[test]
fn an_empty_inline_element_has_a_box_but_no_line() {
    let (doc, tree) = page(
        r#"<!doctype html><div id=d>ab <span id=e></span>cd</div><div id=q>ab<br><span id=t></span></div><div id=s style="font-size: 8px">ab <span id=u style="font-size: 30px"></span></div>"#,
        "div { font-size: 16px }",
        400.0,
    );
    let rect = |name: &str| tree.get(id(&doc, name)).map(|b| b.rect);
    let e = rect("e").expect("the empty span has a box");
    assert!(near(e.x, 24.0) && e.width == 0.0, "{e:?}");
    assert!(near(rect("d").unwrap().height, 19.2));
    // After a break, it starts no new line.
    assert!(near(rect("q").unwrap().height, 19.2), "{:?}", rect("q"));
    // Nor does a larger font of its own raise its line.
    assert!(
        near(rect("s").unwrap().height, 9.6 * 16.0 / 16.0),
        "{:?}",
        rect("s")
    );
}

/// Clearance puts a box's border edge at the float's bottom when the
/// margins before it wouldn't get it there, and those margins are spent in
/// it (CSS 2 §9.5.2): gnu.org's `<div style="clear: both">` after its
/// floated button sat a paragraph margin too low.
#[test]
fn clearance_absorbs_the_margins_above_it() {
    let float =
        r#"<div style="float: left; width: 10px; height: 50px; margin-bottom: 32px"></div>"#;
    let (doc, tree) = page(
        &format!(
            r#"<!doctype html><div><p style="margin: 0 0 16px">a</p>{float}<div id=c style="clear: both"></div><div id=n>b</div></div><div><p style="margin: 0 0 16px">a</p>{float}<div id=t style="clear: both; margin-top: 10px">c</div></div>"#
        ),
        "div, p { font-size: 16px }",
        400.0,
    );
    let y = |name: &str| tree.get(id(&doc, name)).unwrap().rect.y;
    // The float sits at 19.2 + 16; its margin box ends 82px lower.
    assert!(near(y("c"), 117.2), "{}", y("c"));
    assert!(near(y("n"), 117.2), "{}", y("n"));
    let second = 117.2 + 19.2;
    assert!(
        near(y("t"), second + 117.2),
        "{} vs {}",
        y("t"),
        second + 117.2
    );
}

/// A text control counts whole-pixel average characters, as Chromium rounds
/// the font's average width (Hacker News's 13.33px search field was 4px
/// narrow).
#[test]
fn a_text_controls_average_character_is_whole_pixels() {
    let (doc, tree) = page(
        r#"<p><input id="a"><textarea id="t" cols="10"></textarea></p>"#,
        // MetricShaper: a 13.4px "0" is 6.7px wide, counted as 7.
        "input, textarea { padding: 0; border: 0; font-size: 13.4px }",
        400.0,
    );
    let width = |name: &str| tree.get(id(&doc, name)).unwrap().content_box().width;
    assert!(near(width("a"), 140.0), "{}", width("a"));
    assert!(near(width("t"), 85.0), "{}", width("t"));
}

/// `justify-content` keeps aligning when the items overflow (its default
/// alignment is unsafe): `flex-end` pushes the overflow out the start,
/// `center` out both sides, and the `space-*` values fall back to `start`
/// or `center`, as in Chromium (rust-lang.org's nav list).
#[test]
fn justify_content_aligns_overflowing_items() {
    let row = |justify: &str| {
        format!(
            r#"<div style="display: flex; width: 100px; justify-content: {justify}"><div id="{justify}" style="flex: none; width: 150px; height: 10px"></div></div>"#
        )
    };
    let html: String = ["flex-end", "center", "space-between", "space-around"]
        .iter()
        .map(|j| row(j))
        .collect::<String>()
        + r#"<div style="display: flex; flex-direction: column; height: 100px; justify-content: flex-end"><div id="col" style="flex: none; height: 150px"></div></div>"#;
    let (doc, tree) = page(&format!("<!doctype html>{html}"), "", 400.0);
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    assert_eq!(rect("flex-end").x, -50.0);
    assert_eq!(rect("center").x, -25.0);
    assert_eq!(rect("space-between").x, 0.0);
    assert_eq!(rect("space-around").x, -25.0);
    let column_top = rect("col").y - (40.0);
    assert_eq!(column_top, -50.0, "{:?}", rect("col"));
}

/// A closed `<select>` is a control, not a box of options: as wide as its
/// widest option's label (whole pixels) plus Chromium's 20px for the inner
/// padding and arrow, a line tall plus a pixel above and below, showing the
/// selected option's label; the options themselves make no boxes
/// (rust-lang.org's language picker spread its options across the page).
#[test]
fn a_dropdown_select_shows_its_selected_label() {
    let (doc, tree) = page(
        r#"<!doctype html><div><select id="s"><option id="o">ab</option><option selected>abcd</option></select><select id="e"></select></div>"#,
        "select { font-size: 16px }",
        400.0,
    );
    let select = tree.get(id(&doc, "s")).unwrap();
    let content = select.content_box();
    assert!(near(content.width, 52.0), "{content:?}");
    assert!(near(content.height, 21.2), "{content:?}");
    assert_eq!(select.border.left, 1.0, "the UA's 1px border");
    assert!(tree.get(id(&doc, "o")).is_none(), "options make no boxes");
    let label = tree
        .boxes()
        .iter()
        .find(|b| b.text.as_deref() == Some("abcd"))
        .expect("the selected label is drawn");
    assert!(
        near(label.rect.x, content.x + 4.0) && near(label.rect.y, content.y + 1.0),
        "{:?}",
        label.rect
    );
    // An empty select is just the padding and arrow.
    assert!(near(
        tree.get(id(&doc, "e")).unwrap().content_box().width,
        20.0
    ));
}

#[test]
fn a_select_without_appearance_drops_its_arrow_and_inner_padding() {
    let (doc, tree) = page(
        r#"<!doctype html><div><select id="s"><option>abcd</option></select><select id="w"><option>abcd</option></select></div>"#,
        "select { font-size: 16px } #s { -webkit-appearance: none; appearance: none; padding: 2px 6px }",
        400.0,
    );
    let select = tree.get(id(&doc, "s")).unwrap();
    let content = select.content_box();
    // Just the label: no 20px of arrow, no 1px of inner padding.
    assert!(near(content.width, 32.0), "{content:?}");
    assert!(near(content.height, 19.2), "{content:?}");
    assert_eq!(select.padding.left, 6.0);
    let label = tree
        .boxes()
        .iter()
        .find(|b| b.text.as_deref() == Some("abcd") && b.style_node == id(&doc, "s"))
        .expect("the label is still drawn");
    assert!(
        near(label.rect.x, content.x) && near(label.rect.y, content.y),
        "{:?}",
        label.rect
    );
    // `auto` keeps the drop-down's chrome.
    assert!(near(
        tree.get(id(&doc, "w")).unwrap().content_box().width,
        52.0
    ));
}

/// A select with `multiple` or a `size` over 1 is a list box, as Chromium
/// draws one: `size` rows (4 for `multiple`), each option a block 1.2em
/// tall plus 1px, as wide as the widest option and 2px of padding a side.
/// Options past the last row scroll out of sight.
#[test]
fn a_list_box_select_stacks_its_options_in_rows() {
    let (doc, tree) = page(
        r#"<!doctype html><div><select id="s" size="3"><option id="a">ab</option><option id="b">abcd</option><option>x</option><option>y</option><option>z</option></select></div><div><select id="m" multiple><option>ab</option></select></div><div><select id="e" size="3"></select></div>"#,
        "select { font-size: 16px }",
        400.0,
    );
    let select = tree.get(id(&doc, "s")).unwrap();
    let content = select.content_box();
    assert!(near(content.width, 36.0), "{content:?}");
    assert!(near(content.height, 3.0 * 20.2), "{content:?}");
    assert!(select.clips, "rows past the last scroll");
    let a = tree.get(id(&doc, "a")).unwrap();
    let b = tree.get(id(&doc, "b")).unwrap();
    assert!(
        near(a.rect.x, content.x) && near(a.rect.y, content.y),
        "{:?}",
        a.rect
    );
    assert!(
        near(a.rect.width, 36.0) && near(a.rect.height, 20.2),
        "{:?}",
        a.rect
    );
    assert_eq!(a.padding.left, 2.0);
    assert!(near(b.rect.y, content.y + 20.2), "{:?}", b.rect);
    // `multiple` shows four rows.
    let m = tree.get(id(&doc, "m")).unwrap().content_box();
    assert!(near(m.height, 4.0 * 20.2), "{m:?}");
    // An empty list box keeps its rows, 1.2em each.
    let e = tree.get(id(&doc, "e")).unwrap().content_box();
    assert!(near(e.height, 3.0 * 19.2) && near(e.width, 0.0), "{e:?}");
}

/// A list box sits on its line by `vertical-align: text-bottom`, as
/// Chromium's sheet sets: its bottom at the bottom of the line's text, so
/// it adds no descent below itself.
#[test]
fn a_list_box_sits_on_the_bottom_of_its_lines_text() {
    let (doc, tree) = page(
        r#"<!doctype html><div id="d">x<select id="s" size="3"><option>ab</option></select></div>"#,
        "",
        400.0,
    );
    let select = tree.get(id(&doc, "s")).unwrap();
    let line = tree.get(id(&doc, "d")).unwrap().rect;
    assert!(
        near(line.height, select.rect.height),
        "{line:?} {:?}",
        select.rect
    );
}

/// In a list box an `optgroup` is a bold row of its `label` above its
/// options, which indent 20px.
#[test]
fn a_list_box_optgroup_is_a_label_row_over_indented_options() {
    let (doc, tree) = page(
        r#"<!doctype html><select id="s" size="3"><optgroup id="g" label="Grp"><option id="o">ab</option></optgroup></select>"#,
        "select { font-size: 16px }",
        400.0,
    );
    let group = tree.get(id(&doc, "g")).unwrap().rect;
    let option = tree.get(id(&doc, "o")).unwrap();
    assert!(near(group.height, 2.0 * 20.2), "{group:?}");
    assert!(near(option.rect.y, group.y + 20.2), "{:?}", option.rect);
    assert_eq!(option.padding.left, 20.0);
    let label = tree
        .boxes()
        .iter()
        .find(|b| b.text.as_deref() == Some("Grp"))
        .expect("the label is drawn");
    assert!(near(label.rect.x, group.x + 2.0), "{:?}", label.rect);
    // As wide as the indented option: 16 + 20 + 2.
    let content = tree.get(id(&doc, "s")).unwrap().content_box();
    assert!(near(content.width, 38.0), "{content:?}");
    // A group's label is a row too: a list box of just a group isn't empty.
    let (doc, tree) = page(
        r#"<!doctype html><select id="s" size="3"><optgroup label="Grp"></optgroup></select>"#,
        "select { font-size: 16px }",
        400.0,
    );
    let content = tree.get(id(&doc, "s")).unwrap().content_box();
    assert!(near(content.height, 3.0 * 20.2), "{content:?}");
}

/// A flex line is as tall as its items' margin boxes, and `center` and
/// `end` align those margin boxes (CSS Flexbox 9.4 step 8, 9.6):
/// rust-lang.org's nav lost a list's 16px margins between its lines.
#[test]
fn flex_lines_hold_their_items_margins() {
    let (doc, tree) = page(
        r#"<!doctype html><div id="f"><div id="a"></div><div id="b"></div><div id="c"></div><div id="d"></div></div>"#,
        "body { margin: 0 } #f { display: flex; flex-wrap: wrap; align-items: center; width: 100px }
         #f > div { width: 100px; height: 10px }
         #a { margin-top: 3px } #b { margin: 16px 0 } #c { align-self: flex-end; margin-bottom: 5px }
         #f > #d { align-self: stretch; height: auto; margin: 4px 0 }",
        400.0,
    );
    let y = |name: &str| tree.get(id(&doc, name)).unwrap().rect.y;
    assert!(near(y("a"), 3.0), "{}", y("a"));
    // Line 2 starts at 13 and is 42 tall, `b` centred in it.
    assert!(near(y("b"), 13.0 + 16.0), "{}", y("b"));
    // Line 3 starts at 55; `c` ends 5px above its end.
    assert!(near(y("c"), 55.0), "{}", y("c"));
    // Line 4 starts at 70: `d` stretches to nothing between its margins.
    let d = tree.get(id(&doc, "d")).unwrap().rect;
    assert!(near(d.y, 74.0) && near(d.height, 0.0), "{d:?}");
    assert!(near(tree.get(id(&doc, "f")).unwrap().rect.height, 78.0));
}

/// Internal table elements have no margins (CSS 2.1 §17.5): a cell's
/// margin neither widens its column nor deepens its row (noonoo#9).
#[test]
fn table_cells_have_no_margins() {
    let (doc, tree) = page(
        r#"<!doctype html><table id="t"><tr><td id="a">a</td><td id="b">b</td></tr></table><table id="u"><tr><td>a</td><td>b</td></tr></table>"#,
        "td { margin: 3px 5px 6px 7px } #u td { margin: 0 }",
        400.0,
    );
    let t = tree.get(id(&doc, "t")).unwrap().rect;
    let u = tree.get(id(&doc, "u")).unwrap().rect;
    assert!(
        near(t.width, u.width) && near(t.height, u.height),
        "{t:?} {u:?}"
    );
    let a = tree.get(id(&doc, "a")).unwrap().rect;
    let b = tree.get(id(&doc, "b")).unwrap().rect;
    // Cells sit at the border spacing, edge to edge with their columns.
    assert!(near(a.y, t.y + 2.0) && near(a.x, t.x + 2.0), "{a:?}");
    assert!(near(b.x, a.x + a.width + 2.0), "{b:?}");
}

/// An outer `<svg>` is replaced content sized as Chromium sizes it
/// (noonoo#7): its `width`/`height` attributes, the `viewBox` ratio for a
/// missing axis, the available width for a ratio alone, and 300x150 with
/// nothing to go on. A `<canvas>` is 300x150 unless its attributes say.
#[test]
fn an_inline_svg_is_sized_from_its_attributes_and_view_box() {
    let (doc, tree) = page(
        r#"<!doctype html><div style="width: 500px">
        <p>trend <svg id="a" width="80" height="20" viewBox="0 0 80 20"><polyline id="pl" points="0,20 40,0 80,10"/></svg> end</p>
        <p><svg id="b"><text>hi</text></svg></p>
        <p><svg id="c" viewBox="0 0 100 50"></svg></p>
        <p><svg id="d" width="100" viewBox="0 0 100 50"></svg></p>
        <p><svg id="e" width="50%" height="10"></svg></p>
        <p><svg id="f" style="width: 40px" viewBox="0 0 20 10"></svg></p>
        <p><canvas id="g"></canvas><canvas id="h" width="30" height="10"></canvas></p></div>"#,
        "",
        800.0,
    );
    let size = |name: &str| {
        let rect = tree.get(id(&doc, name)).unwrap().rect;
        (rect.width, rect.height)
    };
    for (name, expected) in [
        ("a", (80.0, 20.0)),
        ("b", (300.0, 150.0)),
        ("c", (500.0, 250.0)),
        ("d", (100.0, 50.0)),
        ("e", (250.0, 10.0)),
        ("f", (40.0, 20.0)),
        ("g", (300.0, 150.0)),
        ("h", (30.0, 10.0)),
    ] {
        let (w, h) = size(name);
        assert!(
            near(w, expected.0) && near(h, expected.1),
            "{name}: {w}x{h}"
        );
    }
    // Its children are SVG content, not CSS boxes.
    assert!(tree.get(id(&doc, "pl")).is_none());
}

/// The collapsing border model (CSS 2.1 §17.6.2), as Chromium lays it
/// out (noonoo#8): a shared edge is one border, the wider (or `hidden`)
/// winning, the table's own competing on the outside; a column or row
/// line is its widest edge, and each cell's box (and the table's) takes
/// half of the lines around it.
#[test]
fn collapsed_borders_are_shared_and_split_in_half() {
    let (doc, tree) = page(
        r#"<!doctype html>
        <table id="t1"><tr><td id="a1">a</td><td id="b1">b</td><td>c</td></tr></table>
        <table id="t2"><tr><td id="a2">a</td><td id="b2">b</td></tr></table>
        <table id="t4"><tr><td id="a4">a</td><td id="b4">b</td></tr></table>
        <table id="t6"><tr><td id="a6">a</td><td id="b6">b</td></tr><tr><td id="c6">c</td><td>d</td></tr></table>"#,
        "body { margin: 0 } table { border-collapse: collapse; margin-bottom: 10px; padding: 7px }
         td { padding: 0; width: 20px; border: 2px solid black }
         #t2 { border: 6px solid red }
         #t4 td { border-width: 4px } #b4 { border-left-style: hidden }
         #t6 { border: 1px solid red } #t6 td { border-width: 4px } #t6 #a6 { border-top-width: 10px }",
        800.0,
    );
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    let near_rect = |name: &str, x: f32, y: f32, w: f32, h: f32| {
        let r = rect(name);
        assert!(
            near(r.x, x) && near(r.y, y) && near(r.width, w) && near(r.height, h),
            "{name}: {r:?} vs {x},{y} {w}x{h}"
        );
    };
    // Three 2px cells: 20 + 1 + 1 each, the frame's halves outside them.
    near_rect("t1", 0.0, 0.0, 68.0, 23.2);
    near_rect("a1", 1.0, 1.0, 22.0, 19.2 + 2.0);
    near_rect("b1", 23.0, 1.0, 22.0, 21.2);
    // The table's 6px wins the outside: half of it on each side.
    let t2 = rect("t2");
    near_rect("t2", 0.0, t2.y, 54.0, 31.2);
    near_rect("a2", 3.0, t2.y + 3.0, 24.0, 25.2);
    near_rect("b2", 27.0, t2.y + 3.0, 24.0, 25.2);
    // `hidden` wins its edge: nothing between the cells.
    let t4 = rect("t4");
    near_rect("t4", 0.0, t4.y, 48.0, 27.2);
    near_rect("b4", 24.0, t4.y + 2.0, 22.0, 23.2);
    // A row's line is its widest edge: both cells take its 5px half.
    let t6 = rect("t6");
    near_rect("t6", 0.0, t6.y, 52.0, 5.0 + 26.2 + 23.2 + 2.0);
    near_rect("a6", 2.0, t6.y + 5.0, 24.0, 26.2);
    near_rect("b6", 26.0, t6.y + 5.0, 24.0, 26.2);
    near_rect("c6", 2.0, t6.y + 31.2, 24.0, 23.2);
}

/// A cell's `height` is the least it is (CSS 2.1 §17.5.3): taller content
/// makes the cell, and its row, taller, as in Chromium.
#[test]
fn a_cells_height_is_a_minimum() {
    let (doc, tree) = page(
        r#"<!doctype html><table><tr><td id="a">a</td><td id="b">b</td></tr></table><table><tr><td id="c">c</td></tr></table>"#,
        "td { padding: 0; height: 10px } #c { height: 40px }",
        400.0,
    );
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    assert!(near(rect("a").height, 19.2), "{:?}", rect("a"));
    assert!(near(rect("b").height, 19.2), "{:?}", rect("b"));
    assert!(near(rect("c").height, 40.0), "{:?}", rect("c"));
}

/// An empty inline belongs to the white space around it, as in Chromium:
/// white space after only empty inlines at a line's start is dropped (as
/// Wikipedia's icon spans before link text), white space before only empty
/// inlines at its end hangs, and a line breaks after an empty inline that
/// took a space, not before it.
#[test]
fn white_space_around_empty_inlines_hangs_at_line_edges() {
    let (doc, tree) = page(
        r#"<!doctype html><div><a id="a"><span id="i"></span>
<span id="t">Donate</span></a></div><div><span id="u">x</span> <span id="k"></span> </div><div>a <span id="m"></span> b</div><div style="width: 30px">aaaa <span id="n"></span> bbbb</div>"#,
        "body { margin: 0 }",
        400.0,
    );
    let rect = |name: &str| tree.get(id(&doc, name)).unwrap().rect;
    // A line's leading white space goes, empty span or not.
    assert!(near(rect("t").x, 0.0), "{:?}", rect("t"));
    assert!(near(rect("a").width, rect("t").width), "{:?}", rect("a"));
    // Trailing white space hangs: `k` sits right after the `x`.
    assert!(near(rect("k").x, rect("u").right()), "{:?}", rect("k"));
    // Mid-line, the empty span sits after the space it took.
    let m = rect("m");
    assert!(near(m.x, 16.0), "{m:?}");
    // At a wrap it ends the first line, right after `aaaa`.
    let n = rect("n");
    assert!(near(n.x, 32.0) && near(n.y, 3.0 * 19.2), "{n:?}");
}
