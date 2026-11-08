//! Dependency-free benchmarks for the parser and rendering pipeline.
//!
//! Run with:
//!
//! ```sh
//! cargo run --release --manifest-path tools/bench/Cargo.toml
//! cargo run --release --manifest-path tools/bench/Cargo.toml -- --bytes 1048576 --iters 3
//! ```
//!
//! The harness uses `std::time::Instant` and `std::hint::black_box` only, so
//! the tool itself stays dependency-free. Numbers are wall-clock medians; treat
//! them as relative signals, not a regression gate.

use std::hint::black_box;
use std::time::{Duration, Instant};

use noonoo_geometry::Size;
use noonoo_layout::MetricShaper;

const DEFAULT_SIZES: [usize; 3] = [1024, 64 * 1024, 1024 * 1024];

fn main() {
    let mut sizes = DEFAULT_SIZES.to_vec();
    let mut iters = 5usize;
    let mut only: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print_help();
                return;
            }
            "--bytes" => {
                let value = args.next().expect("--bytes needs a number");
                let bytes: usize = value.parse().expect("--bytes needs a number");
                sizes = vec![bytes.max(1)];
            }
            "--iters" => {
                let value = args.next().expect("--iters needs a number");
                iters = value
                    .parse::<usize>()
                    .expect("--iters needs a number")
                    .max(1);
            }
            "--only" => {
                only = Some(args.next().expect("--only needs a name"));
            }
            other => {
                eprintln!("unknown argument: {other}");
                print_help();
                std::process::exit(2);
            }
        }
    }

    let selected = |name: &str| match &only {
        None => true,
        Some(filter) => name.contains(filter.as_str()),
    };

    println!(
        "{:<20} {:>10} {:>6} {:>12} {:>12}",
        "benchmark", "input", "iters", "median", "throughput"
    );
    println!("{}", "-".repeat(65));

    for &bytes in &sizes {
        let iterations = iters_for(bytes, iters);

        if selected("html") {
            let input = html_document(bytes);
            report("html/parse", input.len(), iterations, || {
                let doc = noonoo_html::parse(black_box(&input));
                let _ = doc.elements().len();
                doc
            });
        }

        if selected("css") {
            let input = css_sheet(bytes);
            report("css/parse", input.len(), iterations, || {
                noonoo_css::Stylesheet::parse(black_box(&input))
            });
        }

        if selected("json") {
            let input = json_document(bytes);
            report("json/parse", input.len(), iterations, || {
                noonoo_json::Json::parse(black_box(&input))
            });
        }

        if selected("toml") {
            let input = toml_document(bytes);
            report("toml/parse", input.len(), iterations, || {
                noonoo_toml::to_json(black_box(&input))
            });
        }

        if selected("pipeline") {
            let input = html_document(bytes.min(256 * 1024));
            let engine = noonoo_engine::Engine::from_html(&input);
            let viewport = Size::new(1024.0, 768.0);
            report("pipeline/paint", input.len(), iterations, || {
                engine.paint(black_box(viewport), &MetricShaper)
            });
        }
    }

    if selected("ui") {
        ui_benches(iters);
    }

    if selected("nested") {
        nested_benches(iters);
    }

    if selected("a11y") {
        let input = html_document(64 * 1024);
        let engine = noonoo_engine::Engine::from_html(&input);
        let viewport = Size::new(1024.0, 768.0);
        let styles = engine.compute_styles_for(viewport);
        let layout = noonoo_layout::layout(engine.dom(), &styles, viewport, &MetricShaper);
        report("a11y/build", input.len(), iters.max(3), || {
            noonoo_a11y::build_in(engine.dom(), &styles, Some(&layout), None)
        });
    }
}

/// The arniko component stylesheet, as the engine's coverage test uses it.
const ARNIKO_CSS: &str =
    include_str!("../../../crates/noonoo-engine/tests/fixtures/arniko_styles.css");

/// How many cards the UI benchmarks lay out: about 4,800 boxes.
const UI_CARDS: usize = 320;

/// A page of `cards` arniko cards under the arniko stylesheet.
fn ui_page(cards: usize) -> String {
    let mut html = format!("<html><head><style>{ARNIKO_CSS}</style></head><body>");
    for i in 0..cards {
        html.push_str(&format!(
            r#"<div class="arniko-card"><div class="arniko-card-header"><h3 class="arniko-card-title">Card {i}</h3><span class="arniko-badge arniko-badge-info">new</span></div><div class="arniko-card-body"><p>Body text for card {i}, a few words long.</p><ul><li>one</li><li>two</li><li>three</li></ul></div><div class="arniko-card-footer"><button class="arniko-btn arniko-btn-default arniko-btn-sm">Open</button><button class="arniko-btn arniko-btn-ghost arniko-btn-sm">Dismiss</button></div></div>"#
        ));
    }
    html.push_str("</body></html>");
    html
}

/// Style, layout and paint of an arniko-sized UI, and the cost of bringing
/// it up to date after one element changes.
fn ui_benches(iters: usize) {
    let viewport = Size::new(1280.0, 800.0);
    let mut engine = noonoo_engine::Engine::from_html(&ui_page(UI_CARDS));
    let elements = engine.dom().elements().len();
    let styles = engine.compute_styles_for(viewport);
    let layout = noonoo_layout::layout(engine.dom(), &styles, viewport, &MetricShaper);
    let iters = iters.max(5);

    report_items("ui/style", elements, iters, || {
        engine.compute_styles_for(black_box(viewport))
    });
    report_items("ui/layout", elements, iters, || {
        noonoo_layout::layout(engine.dom(), &styles, black_box(viewport), &MetricShaper)
    });
    report_items("ui/paint", elements, iters, || {
        noonoo_paint::paint(&styles, black_box(&layout))
    });

    // One badge changes class; the styles are then brought up to date.
    let badge = engine.dom().get_elements_by_tag_name("span")[UI_CARDS / 2];
    let interaction = noonoo_style::Interaction::default();
    let mut styles = noonoo_style::Styles::default();
    engine.restyle(&mut styles, viewport, interaction);
    let mut flip = false;
    report_items("ui/restyle-one", 1, iters, || {
        flip = !flip;
        let class = if flip {
            "arniko-badge arniko-badge-error"
        } else {
            "arniko-badge arniko-badge-info"
        };
        let _ = engine
            .dom_mut()
            .mutate()
            .set_attribute(badge, "class", class);
        engine.restyle(&mut styles, black_box(viewport), interaction)
    });
}

/// Layout of grids nested `depth` deep, each holding an item that spans
/// 1,000 rows: intrinsic sizing measures each level's contents again, so
/// the cost grows with depth faster than the box count does.
fn nested_benches(iters: usize) {
    let viewport = Size::new(1024.0, 768.0);
    let shapes: &[(&str, &str)] = &[
        (
            "grid",
            "div { display: grid } p { margin: 0; grid-row: span 1000 }",
        ),
        ("flex", "div { display: flex }"),
        ("flex-col", "div { display: flex; flex-direction: column }"),
        ("inline-block", "div { display: inline-block }"),
        ("float", "div { float: left }"),
        ("table", "div { display: table-cell }"),
        ("abs", "div { position: absolute }"),
    ];
    let depths: Vec<usize> = std::env::var("NESTED_DEPTHS")
        .ok()
        .map(|v| v.split(',').filter_map(|d| d.parse().ok()).collect())
        .unwrap_or_else(|| vec![4, 8, 16, 32]);
    for (shape, css) in shapes {
        for &depth in &depths {
            let html = format!(
                "<style>{css}</style>{}<p>x</p>{}",
                "<div>".repeat(depth),
                "</div>".repeat(depth)
            );
            let engine = noonoo_engine::Engine::from_html(&html);
            let styles = engine.compute_styles_for(viewport);
            report_items(
                &format!("nested/{shape}-{depth}"),
                depth,
                iters.max(3),
                || noonoo_layout::layout(engine.dom(), &styles, black_box(viewport), &MetricShaper),
            );
        }
    }
}

/// Like [`report`], for work measured in elements rather than bytes.
fn report_items<T>(name: &str, items: usize, iters: usize, mut f: impl FnMut() -> T) {
    black_box(f());
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let start = Instant::now();
        black_box(f());
        samples.push(start.elapsed());
    }
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    println!(
        "{name:<20} {:>10} {iters:>6} {:>12}",
        format!("{items} el"),
        human_time(median),
    );
}

fn iters_for(bytes: usize, cap: usize) -> usize {
    match bytes {
        0..=4096 => cap.max(20),
        4097..=131_072 => cap.max(7),
        _ => cap.max(3),
    }
}

fn report<T>(name: &str, bytes: usize, iters: usize, mut f: impl FnMut() -> T) {
    // One warm-up so lazy allocation and caches settle.
    black_box(f());
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let start = Instant::now();
        black_box(f());
        samples.push(start.elapsed());
    }
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    let throughput = bytes as f64 / median.as_secs_f64() / (1024.0 * 1024.0);
    println!(
        "{name:<20} {:>10} {iters:>6} {:>12} {:>10.1} MiB/s",
        human_bytes(bytes),
        human_time(median),
        throughput,
    );
}

fn human_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn human_time(time: Duration) -> String {
    let nanos = time.as_nanos();
    if nanos >= 1_000_000_000 {
        format!("{:.2} s", time.as_secs_f64())
    } else if nanos >= 1_000_000 {
        format!("{:.2} ms", time.as_secs_f64() * 1_000.0)
    } else if nanos >= 1_000 {
        format!("{:.2} us", time.as_secs_f64() * 1_000_000.0)
    } else {
        format!("{nanos} ns")
    }
}

fn html_document(target: usize) -> String {
    let mut html = String::with_capacity(target + 256);
    html.push_str("<!doctype html><html><head><title>bench</title></head><body>");
    let mut i = 0;
    while html.len() < target {
        html.push_str("<div class=\"row\" style=\"width:");
        html.push_str(&(i % 640 + 100).to_string());
        html.push_str("px;margin:2px\"><span>Item ");
        html.push_str(&i.to_string());
        html.push_str("</span></div>");
        i += 1;
    }
    html.push_str("</body></html>");
    html
}

fn css_sheet(target: usize) -> String {
    let mut css = String::with_capacity(target + 128);
    let mut i = 0;
    while css.len() < target {
        css.push_str(".c");
        css.push_str(&i.to_string());
        css.push_str(
            " { color: #112233; margin: 1px 2px; display: flex; gap: 4px; padding: 2px }\n",
        );
        i += 1;
    }
    css
}

fn json_document(target: usize) -> String {
    let mut json = String::with_capacity(target + 128);
    json.push('[');
    let mut i = 0;
    while json.len() < target {
        if i > 0 {
            json.push(',');
        }
        json.push_str("{\"id\":");
        json.push_str(&i.to_string());
        json.push_str(",\"name\":\"item ");
        json.push_str(&i.to_string());
        json.push_str("\",\"ok\":true,\"score\":1.5}");
        i += 1;
    }
    json.push(']');
    json
}

fn toml_document(target: usize) -> String {
    let mut toml = String::with_capacity(target + 128);
    let mut i = 0;
    while toml.len() < target {
        toml.push_str("\n[table");
        toml.push_str(&i.to_string());
        toml.push_str("]\nname = \"item ");
        toml.push_str(&i.to_string());
        toml.push_str("\"\nvalue = ");
        toml.push_str(&i.to_string());
        toml.push_str("\n");
        i += 1;
    }
    toml
}

fn print_help() {
    println!(
        "\
noonoo-bench — parser and pipeline benchmarks

USAGE:
    noonoo-bench [--bytes N] [--iters N] [--only NAME]

OPTIONS:
    --bytes N     Run one input size instead of 1 KiB / 64 KiB / 1 MiB
    --iters N     Minimum sample count (default 5; small inputs get more)
    --only NAME   Run benchmarks whose name contains NAME
                  (html, css, json, toml, pipeline, ui, nested, a11y)
    -h, --help    Show this help"
    );
}
