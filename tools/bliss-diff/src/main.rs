//! Lay pages out with noonoo and with bliss and compare every element's
//! border box.
//!
//! Both engines get the same viewport and the same fonts (DejaVu Sans,
//! forced in bliss through a user-agent `!important` rule, registered as
//! the match-anything face in noonoo), and neither loads subresources, so
//! what differs is layout. Elements from `<body>` down are aligned by
//! their tag sequence (a longest common subsequence, so a parser
//! disagreement shifts the alignment locally instead of mismatching
//! everything after it), then compared box by box.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// An element and its border box in page coordinates, if it has one.
#[derive(Clone, Debug)]
struct Item {
    tag: String,
    label: String,
    rect: Option<[f32; 4]>,
    /// The engine has no box of its own for it to compare (bliss lays an
    /// inline element out inside its parent's text).
    unmeasured: bool,
}

/// Elements that never draw a box of their own.
fn skipped(tag: &str) -> bool {
    matches!(
        tag,
        "head" | "script" | "style" | "template" | "meta" | "link" | "title" | "noscript"
    )
}

fn label(tag: &str, id: Option<&str>, class: Option<&str>) -> String {
    let mut out = tag.to_owned();
    if let Some(id) = id.filter(|id| !id.is_empty()) {
        let _ = write!(out, "#{id}");
    } else if let Some(class) = class.and_then(|c| c.split_whitespace().next()) {
        let _ = write!(out, ".{class}");
    }
    out
}

struct Config {
    width: u32,
    height: u32,
    tolerance: f32,
    worst: usize,
    fonts: PathBuf,
}

const FACES: [(&str, u16, bool); 4] = [
    ("DejaVuSans.ttf", 400, false),
    ("DejaVuSans-Bold.ttf", 700, false),
    ("DejaVuSans-Oblique.ttf", 400, true),
    ("DejaVuSans-BoldOblique.ttf", 700, true),
];

fn noonoo_items(html: &str, config: &Config) -> Vec<Item> {
    let engine = noonoo_engine::Engine::from_html(html);
    let data: Vec<Vec<u8>> = FACES
        .iter()
        .map(|(file, _, _)| std::fs::read(config.fonts.join(file)).expect("a DejaVu face"))
        .collect();
    let fonts: Vec<_> = data
        .iter()
        .map(|bytes| noonoo_ttf::Font::parse(bytes).expect("a parsable face"))
        .collect();
    let mut set = noonoo_ttf::FontSet::new();
    for (font, (_, weight, italic)) in fonts.iter().zip(FACES) {
        set.add("", weight, italic, font);
    }
    let shaper = noonoo_ttf::FontSetShaper::new(&set);
    let viewport = noonoo_geometry::Size::new(config.width as f32, config.height as f32);
    let tree = engine.layout(viewport, &shaper);
    let dom = engine.dom();
    let mut items = Vec::new();
    let Some(body) = dom.get_elements_by_tag_name("body").first().copied() else {
        return items;
    };
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        let Some(element) = dom.element(node) else {
            continue;
        };
        let tag = element.name.to_ascii_lowercase();
        if skipped(&tag) {
            continue;
        }
        let rect = tree
            .get(node)
            .map(|b| [b.rect.x, b.rect.y, b.rect.width, b.rect.height]);
        items.push(Item {
            label: label(&tag, element.attr("id"), element.attr("class")),
            tag,
            rect,
            unmeasured: false,
        });
        stack.extend(dom.tree().children(node).iter().rev().copied());
    }
    items
}

fn bliss_items(html: &str, config: &Config) -> Vec<Item> {
    use bliss_traits::shell::{ColorScheme, Viewport};
    let doc_config = bliss_dom::DocumentConfig {
        viewport: Some(Viewport::new(
            config.width,
            config.height,
            1.0,
            ColorScheme::Light,
        )),
        ua_stylesheets: Some(vec![
            "* { font-family: 'DejaVu Sans' !important }".to_owned(),
        ]),
        ..Default::default()
    };
    let mut doc = bliss_html::HtmlDocument::from_html(html, doc_config).into_inner();
    doc.resolve(0.0);
    let mut items = Vec::new();
    let body = doc.root_element().and_then(|html| {
        html.children.iter().copied().find(|&child| {
            doc.get_node(child)
                .and_then(|node| node.element_data())
                .is_some_and(|element| &*element.name.local == "body")
        })
    });
    let Some(body) = body else {
        return items;
    };
    let mut stack = vec![body];
    while let Some(id) = stack.pop() {
        let Some(node) = doc.get_node(id) else {
            continue;
        };
        let Some(element) = node.element_data() else {
            continue;
        };
        let tag = element.name.local.to_string();
        if skipped(&tag) {
            continue;
        }
        let display = node.primary_styles().map(|style| style.get_box().display);
        let shown = display.is_some_and(|display| !display.is_none());
        let unmeasured = display.is_some_and(|display| display.is_inline_flow());
        let rect = shown.then(|| {
            let at = node.absolute_position(0.0, 0.0);
            let size = node.final_layout.size;
            [at.x, at.y, size.width, size.height]
        });
        items.push(Item {
            label: label(
                &tag,
                node.attr(bliss_dom::local_name!("id")),
                node.attr(bliss_dom::local_name!("class")),
            ),
            tag,
            rect,
            unmeasured,
        });
        stack.extend(node.children.iter().rev().copied());
    }
    items
}

/// Index pairs of the longest common subsequence of the two tag sequences.
fn align(a: &[Item], b: &[Item]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut table = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[at(i, j)] = if a[i].tag == b[j].tag {
                table[at(i + 1, j + 1)] + 1
            } else {
                table[at(i + 1, j)].max(table[at(i, j + 1)])
            };
        }
    }
    let (mut i, mut j, mut pairs) = (0, 0, Vec::new());
    while i < n && j < m {
        if a[i].tag == b[j].tag {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if table[at(i + 1, j)] >= table[at(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

fn report(path: &Path, config: &Config) -> (usize, usize, usize) {
    let html = match std::fs::read_to_string(path) {
        Ok(html) => html,
        Err(error) => {
            println!("{}: {error}", path.display());
            return (0, 0, 0);
        }
    };
    let ours = noonoo_items(&html, config);
    let theirs = bliss_items(&html, config);
    let pairs = align(&ours, &theirs);
    let mut compared = Vec::new();
    let mut presence = 0;
    let mut unmeasured = 0;
    for &(i, j) in &pairs {
        if theirs[j].unmeasured {
            unmeasured += 1;
            continue;
        }
        match (ours[i].rect, theirs[j].rect) {
            (Some(a), Some(b)) => {
                let delta = (0..4).map(|k| (a[k] - b[k]).abs()).fold(0.0f32, f32::max);
                compared.push((delta, i, j));
            }
            (None, None) => {}
            _ => presence += 1,
        }
    }
    let close = compared
        .iter()
        .filter(|(d, ..)| *d <= config.tolerance)
        .count();
    // A box that's the right size in the wrong place usually sits below an
    // earlier difference: count sizes on their own too.
    let sized = compared
        .iter()
        .filter(|&&(_, i, j)| {
            let (a, b) = (ours[i].rect.unwrap(), theirs[j].rect.unwrap());
            (a[2] - b[2]).abs() <= config.tolerance && (a[3] - b[3]).abs() <= config.tolerance
        })
        .count();
    println!(
        "{}: {} noonoo / {} bliss elements, {} aligned; {close}/{} boxes within {}px ({:.0}%), {sized} sizes ({:.0}%), {presence} shown by one engine only, {unmeasured} inline (not compared)",
        path.display(),
        ours.len(),
        theirs.len(),
        pairs.len(),
        compared.len(),
        config.tolerance,
        100.0 * close as f32 / compared.len().max(1) as f32,
        100.0 * sized as f32 / compared.len().max(1) as f32,
    );
    compared.sort_by(|a, b| b.0.total_cmp(&a.0));
    for &(delta, i, j) in compared
        .iter()
        .take(config.worst)
        .filter(|(d, ..)| *d > config.tolerance)
    {
        let (a, b) = (ours[i].rect.unwrap(), theirs[j].rect.unwrap());
        println!(
            "  {:>7.1}px  {:<32} noonoo {:>7.1} {:>7.1} {:>7.1}x{:<7.1} bliss {:>7.1} {:>7.1} {:>7.1}x{:<7.1}",
            delta, ours[i].label, a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3]
        );
    }
    (close, sized, compared.len())
}

const USAGE: &str = "usage: noonoo-bliss-diff [--width W] [--height H] [--tolerance PX] [--worst N] [--fonts DIR] PAGE.html...";

fn main() {
    let mut config = Config {
        width: 1024,
        height: 768,
        tolerance: 1.0,
        worst: 8,
        fonts: PathBuf::from("/usr/share/fonts/TTF"),
    };
    let mut pages = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .unwrap_or_else(|| panic!("{arg} needs a value\n{USAGE}"))
        };
        match arg.as_str() {
            "--width" => config.width = value().parse().expect("a width"),
            "--height" => config.height = value().parse().expect("a height"),
            "--tolerance" => config.tolerance = value().parse().expect("a tolerance"),
            "--worst" => config.worst = value().parse().expect("a count"),
            "--fonts" => config.fonts = PathBuf::from(value()),
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            _ => pages.push(PathBuf::from(arg)),
        }
    }
    if pages.is_empty() {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
    let (mut close, mut sized, mut total) = (0, 0, 0);
    for page in &pages {
        let (c, z, t) = report(page, &config);
        close += c;
        sized += z;
        total += t;
    }
    println!(
        "overall: {close}/{total} boxes within {}px ({:.0}%), {sized} sizes ({:.0}%)",
        config.tolerance,
        100.0 * close as f32 / total.max(1) as f32,
        100.0 * sized as f32 / total.max(1) as f32,
    );
}
