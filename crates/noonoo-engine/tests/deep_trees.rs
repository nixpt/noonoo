//! Markup nested far deeper than any real page goes through the whole
//! pipeline — parse, style, layout, paint — without overflowing the stack.
//! The parser caps the tree at `noonoo_html::MAX_TREE_DEPTH`, so this holds
//! on a test thread's 2 MiB stack (block and inline nesting even in debug).

use noonoo_engine::Engine;
use noonoo_geometry::Size;
use noonoo_layout::MetricShaper;

fn nested(tag: &str, depth: usize) -> String {
    format!(
        "{}x{}",
        format!("<{tag}>").repeat(depth),
        format!("</{tag}>").repeat(depth)
    )
}

#[test]
fn ten_thousand_nested_blocks_paint() {
    let engine = Engine::from_html(&nested("div", 10_000));
    let _ = engine.paint(Size::new(800.0, 600.0), &MetricShaper);
}

#[test]
fn ten_thousand_nested_inlines_and_cells_paint() {
    for tag in ["span", "li", "td", "b"] {
        let engine = Engine::from_html(&nested(tag, 10_000));
        let _ = engine.paint(Size::new(800.0, 600.0), &MetricShaper);
    }
}

/// Flex, grid and inline-block layout recurse through more frames per
/// level, and debug frames are several times larger, so in debug this runs
/// on a 16 MiB stack. In release it holds on a spawned thread's default
/// 2 MiB, which is what a tab worker gets.
#[test]
fn deep_flex_and_grid_paint() {
    let stack = if cfg!(debug_assertions) {
        16 << 20
    } else {
        2 << 20
    };
    std::thread::Builder::new()
        .stack_size(stack)
        .spawn(|| {
            let html = format!(
                "<style>.f {{ display: flex }} .g {{ display: grid }} .i {{ display: inline-block }}</style>{}x{}",
                r#"<div class="f"><div class="g"><span class="i">"#.repeat(300),
                "</span></div></div>".repeat(300)
            );
            let engine = Engine::from_html(&html);
            let _ = engine.paint(Size::new(800.0, 600.0), &MetricShaper);
        })
        .unwrap()
        .join()
        .unwrap();
}
