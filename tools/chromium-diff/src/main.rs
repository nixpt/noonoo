//! noonoo's block geometry of a page, for the Chromium diff (`cmp.py`).
//!
//! Lays `PAGE` out at `WIDTH` x 600 with noonoo's font-set shaper over the
//! four DejaVu Sans faces (`FONT_DIR`, default `/usr/share/fonts/TTF`),
//! registered under every family name a page may ask for, so it matches
//! Chromium with DejaVu forced. `FALLBACK_FONTS`, a `:`-separated list of
//! font files, adds faces for scripts DejaVu lacks, tried in order. Prints a
//! TSV in DOM order from `<body>`: `E idx tag id x y w h n_boxes` per element
//! (`-` when it has no box) and `T x y w h "text"` per text box.
//!
//!     cargo run --release -- PAGE [WIDTH]

const FACES: [(&str, u16, bool); 4] = [
    ("DejaVuSans.ttf", 400, false),
    ("DejaVuSans-Bold.ttf", 700, false),
    ("DejaVuSans-Oblique.ttf", 400, true),
    ("DejaVuSans-BoldOblique.ttf", 700, true),
];

const FAMILIES: [&str; 11] = [
    "",
    "monospace",
    "serif",
    "sans-serif",
    "DejaVu Sans",
    "arial",
    "verdana",
    "helvetica",
    "times new roman",
    "courier new",
    "courier",
];

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("usage: noonoo-chromium-diff PAGE [WIDTH]");
    let width: f32 = args.next().map_or(1024.0, |s| s.parse().expect("WIDTH"));
    let dir = std::env::var("FONT_DIR").unwrap_or_else(|_| "/usr/share/fonts/TTF".into());
    let html = std::fs::read_to_string(&path).expect("PAGE");
    let engine = noonoo_engine::Engine::from_html(&html);
    let data: Vec<Vec<u8>> = FACES
        .iter()
        .map(|(f, _, _)| std::fs::read(format!("{dir}/{f}")).expect("a DejaVu Sans face"))
        .collect();
    let fonts: Vec<_> = data
        .iter()
        .map(|b| noonoo_ttf::Font::parse(b).expect("font"))
        .collect();
    let mut set = noonoo_ttf::FontSet::new();
    for fam in FAMILIES {
        for (font, (_, weight, italic)) in fonts.iter().zip(FACES) {
            set.add(fam, weight, italic, font);
        }
    }
    let fallback_data: Vec<Vec<u8>> = std::env::var("FALLBACK_FONTS")
        .unwrap_or_default()
        .split(':')
        .filter(|file| !file.is_empty())
        .map(|file| std::fs::read(file).expect("a fallback font"))
        .collect();
    let fallbacks: Vec<_> = fallback_data
        .iter()
        .map(|b| noonoo_ttf::Font::parse(b).expect("fallback font"))
        .collect();
    for font in &fallbacks {
        set.add_fallback(font);
    }
    let shaper = noonoo_ttf::FontSetShaper::new(&set);
    let tree = engine.layout(noonoo_geometry::Size::new(width, 600.0), &shaper);
    let dom = engine.dom();
    let Some(body) = dom.get_elements_by_tag_name("body").first().copied() else {
        return;
    };
    let mut stack = vec![body];
    let mut idx = 0;
    while let Some(node) = stack.pop() {
        if let Some(el) = dom.element(node) {
            let tag = el.name.to_ascii_lowercase();
            if matches!(tag.as_str(), "script" | "style" | "head") {
                continue;
            }
            let n = tree.boxes().iter().filter(|b| b.node == Some(node)).count();
            let id = el.attr("id").unwrap_or("");
            match tree.bounding_client_rect(node) {
                Some(r) => println!(
                    "E\t{idx}\t{tag}\t{id}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{n}",
                    r.x, r.y, r.width, r.height
                ),
                None => println!("E\t{idx}\t{tag}\t{id}\t-\t-\t-\t-\t0"),
            }
            idx += 1;
            stack.extend(dom.tree().children(node).iter().rev().copied());
        }
    }
    for b in tree.boxes() {
        if let (Some(t), noonoo_layout::BoxKind::Text) = (&b.text, b.kind) {
            println!(
                "T\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{t:?}",
                b.rect.x, b.rect.y, b.rect.width, b.rect.height
            );
        }
    }
}
