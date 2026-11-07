//! # noonoo-paint
//!
//! Turn a [`LayoutTree`] into a backend-neutral **command stream**: fill a box,
//! draw a border side, draw a text run and its decoration bars. The stream is the
//! artifact — it can be diffed (as JSON), fed to a GPU rasterizer, or recorded
//! by a test without a screen.
//!
//! This is Phase D of the dependency-exit plan (`docs/DEPENDENCY-EXIT.md`). The
//! GPU backend (`anyrender`/`vello`) is deliberately **not** wired yet: this
//! crate owns the translation, which is the part that is ours. See
//! `docs/SHELL.md` for how overlay textures / scene effects land later.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use noonoo_css::Color;
use noonoo_geometry::{Point, Rect};
use noonoo_json::Json;
use noonoo_layout::{BoxKind, LayoutBox, LayoutTree};
use noonoo_style::{Styles, TextDecoration};

/// A single drawing instruction.
#[derive(Clone, PartialEq, Debug)]
pub enum Command {
    /// Fill an axis-aligned rectangle.
    FillRect {
        /// The rectangle.
        rect: Rect,
        /// The fill colour.
        color: Color,
    },
    /// Fill a polygon (at least three points, in order).
    FillPolygon {
        /// The vertices.
        points: Vec<Point>,
        /// The fill colour.
        color: Color,
    },
    /// Draw a text run, with `y` on the baseline.
    DrawText {
        /// The text.
        text: String,
        /// Baseline x.
        x: f32,
        /// Baseline y.
        y: f32,
        /// Font size.
        font_size: f32,
        /// Text colour.
        color: Color,
    },
}

/// An ordered list of [`Command`]s.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct CommandList(pub Vec<Command>);

impl CommandList {
    /// The commands.
    pub fn commands(&self) -> &[Command] {
        &self.0
    }

    /// Number of commands.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no commands.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Serialize to JSON — the diffable artifact.
    pub fn to_json(&self) -> Json {
        Json::Array(self.0.iter().map(command_json).collect())
    }
}

/// Paint a laid-out document into a command list.
pub fn paint(styles: &Styles, layout: &LayoutTree) -> CommandList {
    let mut list = CommandList(Vec::new());
    let boxes = layout.boxes();
    for &root in layout.root_indices() {
        paint_box(root, boxes, styles, &mut list);
    }
    list
}

fn paint_box(index: usize, boxes: &[LayoutBox], styles: &Styles, out: &mut CommandList) {
    let b = &boxes[index];
    let style = styles.get(b.style_node);

    // Background and borders belong to element boxes; a text run draws only text
    // (it shares its element's style, which was already painted).
    if b.kind != BoxKind::Text {
        if let Some(style) = style
            && style.background_color.a > 0
        {
            out.0.push(Command::FillRect {
                rect: b.rect,
                color: style.background_color,
            });
        }

        for (rect, width, color) in border_sides(b, style) {
            if width > 0.0 {
                out.0.push(Command::FillRect { rect, color });
            }
        }
    }

    if let Some(text) = &b.text {
        let (font_size, color) = style
            .map(|s| (s.font_size, s.color))
            .unwrap_or((16.0, Color::BLACK));
        out.0.push(Command::DrawText {
            text: text.clone(),
            x: b.rect.x,
            y: b.rect.y + font_size * 0.8,
            font_size,
            color,
        });

        // Text decorations are bars across the run, in the text colour.
        if let Some(style) = style {
            let decoration = style.text_decoration;
            if !decoration.is_empty() {
                let thickness = (font_size * 0.07).max(1.0);
                let baseline = b.rect.y + font_size * 0.8;
                for (flag, y) in [
                    (TextDecoration::UNDERLINE, baseline + font_size * 0.08),
                    (TextDecoration::OVERLINE, b.rect.y),
                    (TextDecoration::LINE_THROUGH, baseline - font_size * 0.32),
                ] {
                    if decoration.contains(flag) {
                        out.0.push(Command::FillRect {
                            rect: Rect::new(b.rect.x, y, b.rect.width, thickness),
                            color,
                        });
                    }
                }
            }
        }
    }

    for &child in &b.children {
        paint_box(child, boxes, styles, out);
    }
}

/// The four border side bands: `(rect, width, colour)`.
fn border_sides(
    b: &LayoutBox,
    style: Option<&noonoo_style::ComputedStyle>,
) -> [(Rect, f32, Color); 4] {
    let Some(style) = style else {
        return [(Rect::default(), 0.0, Color::BLACK); 4];
    };
    let r = b.rect;
    let w = &b.border;
    [
        (
            Rect::new(r.x, r.y, r.width, w.top),
            w.top,
            style.border_color.top,
        ),
        (
            Rect::new(r.right() - w.right, r.y, w.right, r.height),
            w.right,
            style.border_color.right,
        ),
        (
            Rect::new(r.x, r.bottom() - w.bottom, r.width, w.bottom),
            w.bottom,
            style.border_color.bottom,
        ),
        (
            Rect::new(r.x, r.y, w.left, r.height),
            w.left,
            style.border_color.left,
        ),
    ]
}

fn command_json(command: &Command) -> Json {
    match command {
        Command::FillRect { rect, color } => object(vec![
            ("op", Json::String("fill-rect".into())),
            ("x", Json::Number(rect.x as f64)),
            ("y", Json::Number(rect.y as f64)),
            ("width", Json::Number(rect.width as f64)),
            ("height", Json::Number(rect.height as f64)),
            ("color", Json::String(color_hex(*color))),
        ]),
        Command::FillPolygon { points, color } => object(vec![
            ("op", Json::String("fill-polygon".into())),
            (
                "points",
                Json::Array(
                    points
                        .iter()
                        .map(|p| {
                            Json::Object(vec![
                                ("x".to_owned(), Json::Number(p.x as f64)),
                                ("y".to_owned(), Json::Number(p.y as f64)),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("color", Json::String(color_hex(*color))),
        ]),
        Command::DrawText {
            text,
            x,
            y,
            font_size,
            color,
        } => object(vec![
            ("op", Json::String("draw-text".into())),
            ("text", Json::String(text.clone())),
            ("x", Json::Number(*x as f64)),
            ("y", Json::Number(*y as f64)),
            ("font_size", Json::Number(*font_size as f64)),
            ("color", Json::String(color_hex(*color))),
        ]),
    }
}

fn object(pairs: Vec<(&str, Json)>) -> Json {
    Json::Object(
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn color_hex(color: Color) -> String {
    if color.a == 255 {
        format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            color.r, color.g, color.b, color.a
        )
    }
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;
    use noonoo_geometry::Size;
    use noonoo_layout::MetricShaper;

    fn render(html: &str, css: &str) -> CommandList {
        let doc = noonoo_html::parse(html);
        let sheet = noonoo_css::Stylesheet::parse(css).unwrap();
        let styles = noonoo_style::compute(&doc, &[sheet]);
        let layout = noonoo_layout::layout(&doc, &styles, Size::new(400.0, 300.0), &MetricShaper);
        paint(&styles, &layout)
    }

    #[test]
    fn paints_background_colour() {
        let list = render(r#"<div id="a">x</div>"#, "#a { background-color: #3366cc }");
        assert!(list.commands().iter().any(|c| matches!(
            c,
            Command::FillRect { color, .. } if *color == Color::rgb(0x33, 0x66, 0xcc)
        )));
    }

    #[test]
    fn paints_text_with_its_colour() {
        let list = render(r#"<div id="a">hi</div>"#, "#a { color: red }");
        assert!(list.commands().iter().any(|c| matches!(
            c,
            Command::DrawText { text, color, .. } if text == "hi" && *color == Color::rgb(255, 0, 0)
        )));
    }

    #[test]
    fn paints_borders() {
        let list = render(
            r#"<div id="a">x</div>"#,
            "#a { border: 3px solid black; background-color: white }",
        );
        // 4 border sides + 1 background fill.
        let fills = list
            .commands()
            .iter()
            .filter(|c| matches!(c, Command::FillRect { .. }))
            .count();
        assert_eq!(fills, 5);
    }

    #[test]
    fn json_is_stable() {
        let list = render(r#"<div>x</div>"#, "#a { }");
        let json = list.to_json().to_string();
        assert!(json.starts_with('['));
        assert!(json.contains("draw-text"));
    }

    #[test]
    fn paints_a_text_decoration_bar() {
        let list = render(
            r#"<div id="a">hi</div>"#,
            "#a { color: red; text-decoration: underline }",
        );
        // An underline bar, in the text colour, alongside the run.
        assert!(list.commands().iter().any(|c| matches!(
            c,
            Command::FillRect { color, .. } if *color == Color::rgb(255, 0, 0)
        )));
    }

    #[test]
    fn links_are_underlined_by_the_user_agent_sheet() {
        let list = render(r##"<a href="#">go</a>"##, "");
        assert!(
            list.commands()
                .iter()
                .any(|c| matches!(c, Command::FillRect { .. }))
        );
    }
}
