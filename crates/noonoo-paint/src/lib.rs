//! # noonoo-paint
//!
//! Turn a [`LayoutTree`] into a backend-neutral **command stream**: fill a box
//! (flat or gradient), draw a border side, draw a text run and its decoration
//! bars, and clip a subtree — walking each box's children in stacking order.
//! The stream is the artifact — it can be diffed (as JSON), fed to a GPU
//! rasterizer, or recorded by a test without a screen.
//!
//! Gradients are resolved here into window-space geometry ([`GradientGeometry`])
//! with CSS stop fix-up ([`GradientStop`]), so a backend receives concrete
//! points, radii, and offsets rather than CSS syntax.
//!
//! Text sits behind the [`Glyphs`] seam: [`paint`] draws runs with [`TextRuns`],
//! while [`paint_with`] takes a renderer — e.g. `noonoo-ttf`'s `OutlineGlyphs`,
//! which emits one `FillContours` per glyph (holes stay open), so a real font renders as
//! vector paths with no GPU text backend.
//!
//! This is Phase D of the dependency-exit plan (`docs/DEPENDENCY-EXIT.md`). The
//! GPU backend (`anyrender`/`vello`) is deliberately **not** wired yet: this
//! crate owns the translation, which is the part that is ours. See
//! `docs/SHELL.md` for how overlay textures / scene effects land later.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use noonoo_css::Color;
use noonoo_geometry::{Point, Rect};
use noonoo_json::Json;
use noonoo_layout::{BoxKind, LayoutBox, LayoutTree};
use noonoo_style::{Gradient, LinearDirection, RadialShape, RadialSize, Styles, TextDecoration};
/// Re-exported for implementers of [`Glyphs`], whose `draw` takes one.
pub use noonoo_text::FontRequest;

/// One colour stop of a gradient fill.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GradientStop {
    /// The offset along the gradient line (`0` = start, `1` = end).
    pub offset: f32,
    /// The colour at the offset.
    pub color: Color,
}

/// The geometry of a gradient fill, resolved in window space.
#[derive(Clone, PartialEq, Debug)]
pub enum GradientGeometry {
    /// A linear gradient from `from` to `to`.
    Linear {
        /// The start of the gradient line.
        from: Point,
        /// The end of the gradient line.
        to: Point,
    },
    /// A radial gradient; the two radii are equal for a circle.
    Radial {
        /// The centre.
        center: Point,
        /// The `(x, y)` radii of the ending shape.
        radius: (f32, f32),
    },
}

pub use noonoo_layout::image::{EncodedImage, ImageFormat};

/// A single drawing instruction.
#[derive(Clone, PartialEq, Debug)]
pub enum Command {
    /// Fill an axis-aligned rectangle, optionally with rounded corners
    /// (`top-left`, `top-right`, `bottom-right`, `bottom-left` radii; all
    /// zero for a sharp rectangle).
    FillRect {
        /// The rectangle.
        rect: Rect,
        /// The fill colour.
        color: Color,
        /// The corner radii, clamped to the rectangle.
        radius: (f32, f32, f32, f32),
    },
    /// Fill a rectangle with a gradient.
    FillGradient {
        /// The filled rectangle.
        rect: Rect,
        /// The resolved geometry.
        geometry: GradientGeometry,
        /// The colour stops, sorted by non-decreasing offset.
        stops: Vec<GradientStop>,
        /// Whether the stops repeat past the end of the line.
        repeating: bool,
    },
    /// Fill a polygon (at least three points, in order).
    FillPolygon {
        /// The vertices.
        points: Vec<Point>,
        /// The fill colour.
        color: Color,
    },
    /// Fill several closed contours as one shape, with the non-zero winding
    /// rule: a contour wound the other way cuts a hole, as in a glyph's `o`.
    FillContours {
        /// The contours, each at least three points, in order.
        contours: Vec<Vec<Point>>,
        /// The fill colour.
        color: Color,
    },
    /// Draw an encoded image scaled to fill `rect` (an `<img>`, or a colour
    /// emoji glyph).
    DrawImage {
        /// Where the image goes.
        rect: Rect,
        /// The image.
        image: EncodedImage,
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
    /// Begin a compositing group: subsequent commands composite with `alpha`
    /// (1.0 = fully opaque) and, when `clip` is given, clipped to it — until
    /// the matching [`Command::PopLayer`]. Groups nest.
    PushLayer {
        /// The clip rectangle, if any.
        clip: Option<Rect>,
        /// The group's alpha.
        alpha: f32,
    },
    /// End the most recent [`Command::PushLayer`].
    PopLayer,
    /// Draw subsequent commands through `matrix` — `[a, b, c, d, e, f]`,
    /// mapping `(x, y)` to `(a·x + c·y + e, b·x + d·y + f)` — composed with
    /// any enclosing transform, until the matching
    /// [`Command::PopTransform`]. CSS `transform`.
    PushTransform {
        /// The affine matrix.
        matrix: [f32; 6],
    },
    /// End the most recent [`Command::PushTransform`].
    PopTransform,
    /// A rounded border: `outer` minus `inner`, each a rounded rectangle,
    /// filled with `color`.
    FillRing {
        /// The border box.
        outer: Rect,
        /// Its corner radii (top-left, top-right, bottom-right, bottom-left).
        outer_radius: (f32, f32, f32, f32),
        /// The padding box (the border box less the border widths).
        inner: Rect,
        /// Its corner radii.
        inner_radius: (f32, f32, f32, f32),
        /// The border colour.
        color: Color,
    },
    /// An outer CSS `box-shadow`: the rounded `rect` filled with `color`,
    /// blurred by `blur` (the CSS blur radius; `0` is a hard edge), and
    /// never drawn inside the element's own border box (`element`, with
    /// `element_radius`), as in CSS.
    BoxShadow {
        /// The shadow box: the border box offset and grown by the spread.
        rect: Rect,
        /// Its corner radii (top-left, top-right, bottom-right, bottom-left).
        radius: (f32, f32, f32, f32),
        /// The blur radius.
        blur: f32,
        /// The shadow colour.
        color: Color,
        /// The border box the shadow is cut out under.
        element: Rect,
        /// The border box's corner radii.
        element_radius: (f32, f32, f32, f32),
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

/// Draws a text run — the seam between layout and the glyph shapes.
///
/// The default [`TextRuns`] emits a [`Command::DrawText`]. A font-backed renderer
/// (e.g. `noonoo-ttf`'s `OutlineGlyphs`) emits glyph polygons instead, selecting
/// the face from a font set by `request` — the element's `font-family`,
/// `font-weight`, and `font-style`, exactly what layout measured against.
/// `origin` is `(x, baseline_y)` in window space.
pub trait Glyphs {
    /// Emit the commands that draw `text`.
    fn draw(
        &self,
        text: &str,
        origin: (f32, f32),
        font_size: f32,
        color: Color,
        request: FontRequest,
        out: &mut CommandList,
    );

    /// Emit the commands that draw `text` with `letter_spacing` pixels
    /// added after each character (CSS `letter-spacing`).
    ///
    /// The default ignores the spacing and calls [`Glyphs::draw`]: a run
    /// renderer like [`TextRuns`] has no per-glyph positions to spread.
    #[allow(clippy::too_many_arguments)]
    fn draw_spaced(
        &self,
        text: &str,
        origin: (f32, f32),
        font_size: f32,
        letter_spacing: f32,
        color: Color,
        request: FontRequest,
        out: &mut CommandList,
    ) {
        let _ = letter_spacing;
        self.draw(text, origin, font_size, color, request, out);
    }
}

/// The default [`Glyphs`]: one [`Command::DrawText`] per run.
#[derive(Debug, Default, Clone, Copy)]
pub struct TextRuns;

impl Glyphs for TextRuns {
    fn draw(
        &self,
        text: &str,
        (x, y): (f32, f32),
        font_size: f32,
        color: Color,
        _request: FontRequest,
        out: &mut CommandList,
    ) {
        out.0.push(Command::DrawText {
            text: text.to_owned(),
            x,
            y,
            font_size,
            color,
        });
    }
}

/// Paint a laid-out document into a command list, drawing text as runs.
pub fn paint(styles: &Styles, layout: &LayoutTree) -> CommandList {
    paint_with(styles, layout, &TextRuns)
}

/// Paint a laid-out document, using `glyphs` to draw every text run.
pub fn paint_with(styles: &Styles, layout: &LayoutTree, glyphs: &dyn Glyphs) -> CommandList {
    let mut list = CommandList(Vec::new());
    let boxes = layout.boxes();
    let mut roots = layout.root_indices().to_vec();
    roots.sort_by_key(|&root| (boxes[root].z_tier, boxes[root].z_index));
    for root in roots {
        paint_box(root, layout, styles, glyphs, 0.0, 0.0, &mut list);
    }
    list
}

/// The matrix for a `transform` list on a box with border box `rect`:
/// the functions compose left to right, about the box's centre
/// (`transform-origin` is always `50% 50%`).
fn transform_matrix(list: &[noonoo_style::Transform], rect: Rect) -> [f32; 6] {
    use noonoo_style::Transform;
    let (ox, oy) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let mut m = [1.0, 0.0, 0.0, 1.0, ox, oy];
    for function in list {
        let step = match function {
            Transform::Translate(x, y) => [
                1.0,
                0.0,
                0.0,
                1.0,
                x.resolve(rect.width).unwrap_or(0.0),
                y.resolve(rect.height).unwrap_or(0.0),
            ],
            Transform::Scale(x, y) => [*x, 0.0, 0.0, *y, 0.0, 0.0],
            Transform::Rotate(degrees) => {
                let (sin, cos) = degrees.to_radians().sin_cos();
                [cos, sin, -sin, cos, 0.0, 0.0]
            }
        };
        m = multiply(m, step);
    }
    multiply(m, [1.0, 0.0, 0.0, 1.0, -ox, -oy])
}

/// `a · b`: apply `b`, then `a`.
fn multiply(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

/// `tx`/`ty` are the accumulated horizontal/vertical scroll of enclosing scroll
/// containers: a box's content is drawn shifted by them.
fn paint_box(
    index: usize,
    layout: &LayoutTree,
    styles: &Styles,
    glyphs: &dyn Glyphs,
    tx: f32,
    ty: f32,
    out: &mut CommandList,
) {
    let boxes = layout.boxes();
    let b = &boxes[index];
    let style = styles.get(b.style_node);
    let rect = b.rect.translate(-tx, -ty);
    // An `opacity` below 1 composites the box's whole subtree as one group.
    let opacity = style.map(|style| style.opacity).unwrap_or(1.0);
    if opacity < 1.0 {
        out.0.push(Command::PushLayer {
            clip: None,
            alpha: opacity,
        });
    }
    // A `transform` draws the box and its subtree through one matrix; it
    // never moves the layout.
    let transformed = match style {
        Some(style) if b.kind != BoxKind::Text && !style.transform.is_empty() => {
            out.0.push(Command::PushTransform {
                matrix: transform_matrix(&style.transform, rect),
            });
            true
        }
        _ => false,
    };

    // Background and borders belong to element boxes; a text run draws only text
    // (it shares its element's style, which was already painted).
    if b.kind != BoxKind::Text {
        if let Some(style) = style {
            // Outer shadows sit under the background; the first listed is
            // on top, so it paints last.
            let element_radius = corner_radii(style, rect);
            for shadow in style.box_shadow.iter().rev().filter(|s| !s.inset) {
                let spread = shadow.spread;
                let grow = |r: f32| if r > 0.0 { (r + spread).max(0.0) } else { 0.0 };
                out.0.push(Command::BoxShadow {
                    rect: Rect::new(
                        rect.x + shadow.x - spread,
                        rect.y + shadow.y - spread,
                        (rect.width + 2.0 * spread).max(0.0),
                        (rect.height + 2.0 * spread).max(0.0),
                    ),
                    radius: (
                        grow(element_radius.0),
                        grow(element_radius.1),
                        grow(element_radius.2),
                        grow(element_radius.3),
                    ),
                    blur: shadow.blur,
                    color: shadow.color.unwrap_or(style.color),
                    element: rect,
                    element_radius,
                });
            }
            if style.background_color.a > 0 {
                out.0.push(Command::FillRect {
                    rect,
                    color: style.background_color,
                    radius: corner_radii(style, rect),
                });
            }
            // The background image paints over the background colour.
            if let Some(gradient) = &style.background_image {
                paint_gradient(gradient, rect, out);
            }
        }

        // A rounded border in one colour is one ring; otherwise each side
        // is a band (and with mixed colours, the corners stay square).
        if let Some(ring) = rounded_border(rect, b, style) {
            out.0.push(ring);
        } else {
            for (band, width, color) in border_sides(rect, b, style) {
                if width > 0.0 {
                    out.0.push(Command::FillRect {
                        rect: band,
                        color,
                        radius: (0.0, 0.0, 0.0, 0.0),
                    });
                }
            }
        }

        // A loaded `<img>` fills its content box, over its background.
        if let Some(image) = b.node.and_then(|node| layout.image(node)) {
            out.0.push(Command::DrawImage {
                rect: b.content_box().translate(-tx, -ty),
                image: image.clone(),
            });
        }
    }

    // Empty runs are dropped: an element with nothing to show (a `<br>`, a
    // placeholder-less `<img>`) must not emit a `draw-text` for nothing.
    if let Some(text) = &b.text
        && !text.is_empty()
    {
        let (font_size, color, letter_spacing, line_height) = style
            .map(|s| (s.font_size, s.color, s.letter_spacing, s.line_height))
            .unwrap_or((16.0, Color::BLACK, 0.0, None));
        // `normal` is the font's own line height: layout sized the run's box
        // with it.
        let line_height = line_height.unwrap_or(rect.height);
        // Half the leading beyond the normal line height (1.2em) goes above
        // the text, so a tall line centres it (and a short one raises it).
        let half_leading = (line_height - font_size * 1.2) / 2.0;
        let baseline = rect.y + half_leading + font_size * 0.8;
        let request = style
            .map(|style| style.font_request())
            .unwrap_or_else(FontRequest::any);
        let shown = match style {
            Some(style) => style.transformed_text(text),
            None => std::borrow::Cow::Borrowed(text.as_str()),
        };
        if letter_spacing == 0.0 {
            glyphs.draw(&shown, (rect.x, baseline), font_size, color, request, out);
        } else {
            glyphs.draw_spaced(
                &shown,
                (rect.x, baseline),
                font_size,
                letter_spacing,
                color,
                request,
                out,
            );
        }

        // Text decorations are bars across the run, in the text colour.
        if let Some(style) = style {
            let decoration = style.text_decoration;
            if !decoration.is_empty() {
                let thickness = (font_size * 0.07).max(1.0);
                for (flag, y) in [
                    (TextDecoration::UNDERLINE, baseline + font_size * 0.08),
                    (TextDecoration::OVERLINE, rect.y),
                    (TextDecoration::LINE_THROUGH, baseline - font_size * 0.32),
                ] {
                    if decoration.contains(flag) {
                        out.0.push(Command::FillRect {
                            rect: Rect::new(rect.x, y, rect.width, thickness),
                            color,
                            radius: (0.0, 0.0, 0.0, 0.0),
                        });
                    }
                }
            }
        }
    }

    // A clipping box clips its content; a scrolling box additionally shifts its
    // content up by its offset.
    if b.clips {
        out.0.push(Command::PushLayer {
            clip: Some(b.padding_box().translate(-tx, -ty)),
            alpha: 1.0,
        });
    }
    let (child_tx, child_ty) = (tx + b.scroll_x, ty + b.scroll_y);
    // Paint children in stacking order: negative `z-index` first, then in-flow
    // content, then positioned boxes by `z-index`.
    let mut children = b.children.clone();
    children.sort_by_key(|&child| (boxes[child].z_tier, boxes[child].z_index));
    for child in children {
        paint_box(child, layout, styles, glyphs, child_tx, child_ty, out);
    }
    if b.clips {
        out.0.push(Command::PopLayer);
    }
    if transformed {
        out.0.push(Command::PopTransform);
    }
    if opacity < 1.0 {
        out.0.push(Command::PopLayer);
    }
}

/// The four border side bands `(rect, width, colour)`, positioned at `rect`.
/// The border as one [`Command::FillRing`], when the box has a rounded
/// corner and every side with a width has the same colour.
fn rounded_border(
    rect: Rect,
    b: &LayoutBox,
    style: Option<&noonoo_style::ComputedStyle>,
) -> Option<Command> {
    let style = style?;
    let radius = corner_radii(style, rect);
    if radius.0 <= 0.0 && radius.1 <= 0.0 && radius.2 <= 0.0 && radius.3 <= 0.0 {
        return None;
    }
    let w = &b.border;
    let colors = &style.border_color;
    let mut visible = [
        (w.top, colors.top),
        (w.right, colors.right),
        (w.bottom, colors.bottom),
        (w.left, colors.left),
    ]
    .into_iter()
    .filter(|(width, _)| *width > 0.0)
    .map(|(_, color)| color);
    let color = visible.next()?;
    if visible.any(|other| other != color) {
        return None;
    }
    let inner = Rect::new(
        rect.x + w.left,
        rect.y + w.top,
        (rect.width - w.left - w.right).max(0.0),
        (rect.height - w.top - w.bottom).max(0.0),
    );
    // Circular corners: each inner corner gives up the wider of the two
    // borders that meet there.
    let shrink = |r: f32, a: f32, b: f32| (r - a.max(b)).max(0.0);
    Some(Command::FillRing {
        outer: rect,
        outer_radius: radius,
        inner,
        inner_radius: (
            shrink(radius.0, w.top, w.left),
            shrink(radius.1, w.top, w.right),
            shrink(radius.2, w.bottom, w.right),
            shrink(radius.3, w.bottom, w.left),
        ),
        color,
    })
}

fn border_sides(
    rect: Rect,
    b: &LayoutBox,
    style: Option<&noonoo_style::ComputedStyle>,
) -> [(Rect, f32, Color); 4] {
    let Some(style) = style else {
        return [(Rect::default(), 0.0, Color::BLACK); 4];
    };
    let r = rect;
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
        Command::FillRect {
            rect,
            color,
            radius,
        } => {
            let mut pairs = vec![
                ("op", Json::String("fill-rect".into())),
                ("x", Json::Number(rect.x as f64)),
                ("y", Json::Number(rect.y as f64)),
                ("width", Json::Number(rect.width as f64)),
                ("height", Json::Number(rect.height as f64)),
                ("color", Json::String(color_hex(*color))),
            ];
            if radius.0 != 0.0 || radius.1 != 0.0 || radius.2 != 0.0 || radius.3 != 0.0 {
                pairs.push((
                    "radius",
                    Json::Array(
                        [radius.0, radius.1, radius.2, radius.3]
                            .iter()
                            .map(|r| Json::Number(*r as f64))
                            .collect(),
                    ),
                ));
            }
            object(pairs)
        }
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
        Command::FillContours { contours, color } => object(vec![
            ("op", Json::String("fill-contours".into())),
            (
                "contours",
                Json::Array(
                    contours
                        .iter()
                        .map(|contour| {
                            Json::Array(contour.iter().copied().map(point_json).collect())
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
        Command::FillGradient {
            rect,
            geometry,
            stops,
            repeating,
        } => {
            let mut pairs = vec![
                ("op", Json::String("fill-gradient".into())),
                ("x", Json::Number(rect.x as f64)),
                ("y", Json::Number(rect.y as f64)),
                ("width", Json::Number(rect.width as f64)),
                ("height", Json::Number(rect.height as f64)),
            ];
            match geometry {
                GradientGeometry::Linear { from, to } => {
                    pairs.push(("kind", Json::String("linear".into())));
                    pairs.push(("from", point_json(*from)));
                    pairs.push(("to", point_json(*to)));
                }
                GradientGeometry::Radial { center, radius } => {
                    pairs.push(("kind", Json::String("radial".into())));
                    pairs.push(("center", point_json(*center)));
                    pairs.push((
                        "radius",
                        Json::Object(vec![
                            ("x".to_owned(), Json::Number(radius.0 as f64)),
                            ("y".to_owned(), Json::Number(radius.1 as f64)),
                        ]),
                    ));
                }
            }
            pairs.push((
                "stops",
                Json::Array(
                    stops
                        .iter()
                        .map(|stop| {
                            Json::Object(vec![
                                ("offset".to_owned(), Json::Number(stop.offset as f64)),
                                ("color".to_owned(), Json::String(color_hex(stop.color))),
                            ])
                        })
                        .collect(),
                ),
            ));
            pairs.push(("repeating", Json::Bool(*repeating)));
            object(pairs)
        }
        Command::PushLayer { clip, alpha } => {
            let pairs = vec![
                ("op", Json::String("push-layer".into())),
                (
                    "clip",
                    match clip {
                        Some(rect) => Json::Object(vec![
                            ("x".to_owned(), Json::Number(rect.x as f64)),
                            ("y".to_owned(), Json::Number(rect.y as f64)),
                            ("width".to_owned(), Json::Number(rect.width as f64)),
                            ("height".to_owned(), Json::Number(rect.height as f64)),
                        ]),
                        None => Json::Null,
                    },
                ),
                ("alpha", Json::Number(*alpha as f64)),
            ];
            object(pairs)
        }
        Command::PopLayer => object(vec![("op", Json::String("pop-layer".into()))]),
        Command::PushTransform { matrix } => object(vec![
            ("op", Json::String("push-transform".into())),
            (
                "matrix",
                Json::Array(matrix.iter().map(|&v| Json::Number(v as f64)).collect()),
            ),
        ]),
        Command::PopTransform => object(vec![("op", Json::String("pop-transform".into()))]),
        Command::FillRing {
            outer,
            outer_radius,
            inner,
            inner_radius,
            color,
        } => object(vec![
            ("op", Json::String("fill-ring".into())),
            ("outer", rect_json(*outer)),
            ("outer-radius", radius_json(*outer_radius)),
            ("inner", rect_json(*inner)),
            ("inner-radius", radius_json(*inner_radius)),
            ("color", Json::String(color_hex(*color))),
        ]),
        Command::DrawImage { rect, image } => object(vec![
            ("op", Json::String("draw-image".into())),
            ("rect", rect_json(*rect)),
            ("format", Json::String(image.format.name().into())),
            ("bytes", Json::Number(image.data.len() as f64)),
        ]),
        Command::BoxShadow {
            rect,
            radius,
            blur,
            color,
            element,
            element_radius,
        } => object(vec![
            ("op", Json::String("box-shadow".into())),
            ("rect", rect_json(*rect)),
            ("radius", radius_json(*radius)),
            ("blur", Json::Number(*blur as f64)),
            ("color", Json::String(color_hex(*color))),
            ("element", rect_json(*element)),
            ("element-radius", radius_json(*element_radius)),
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

fn rect_json(rect: Rect) -> Json {
    Json::Object(vec![
        ("x".to_owned(), Json::Number(rect.x as f64)),
        ("y".to_owned(), Json::Number(rect.y as f64)),
        ("width".to_owned(), Json::Number(rect.width as f64)),
        ("height".to_owned(), Json::Number(rect.height as f64)),
    ])
}

fn radius_json(radius: (f32, f32, f32, f32)) -> Json {
    Json::Array(
        [radius.0, radius.1, radius.2, radius.3]
            .into_iter()
            .map(|r| Json::Number(r as f64))
            .collect(),
    )
}

fn point_json(point: Point) -> Json {
    Json::Object(vec![
        ("x".to_owned(), Json::Number(point.x as f64)),
        ("y".to_owned(), Json::Number(point.y as f64)),
    ])
}

/// Translate a computed gradient into window-space geometry and stops.
///
/// Percentages resolve against the gradient line's length (the gradient line
/// for a linear gradient, the horizontal radius for a radial one); lengths are
/// distances along the same line. The CSS implicit-position fix-up (first stop
/// at `0`, last at `100%`, even spacing for unstated positions, and clamping to
/// be non-decreasing) happens here, once the reference length is known.
fn paint_gradient(gradient: &Gradient, rect: Rect, out: &mut CommandList) {
    let (geometry, reference) = match &gradient.kind {
        noonoo_style::GradientKind::Linear { direction } => {
            let angle = linear_angle(*direction, rect.width, rect.height).to_radians();
            let (sin, cos) = angle.sin_cos();
            // The line spans the box's diagonal band along the direction, so
            // the opposite corners sit exactly at 0% and 100%.
            let length = (rect.width * sin.abs() + rect.height * cos.abs()).max(1.0);
            let (dx, dy) = (sin, -cos);
            let center = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
            let half = length / 2.0;
            (
                GradientGeometry::Linear {
                    from: Point::new(center.x - dx * half, center.y - dy * half),
                    to: Point::new(center.x + dx * half, center.y + dy * half),
                },
                length,
            )
        }
        noonoo_style::GradientKind::Radial {
            shape,
            size,
            center,
        } => {
            let center = Point::new(
                rect.x + center.0.resolve(rect.width).unwrap_or(rect.width / 2.0),
                rect.y + center.1.resolve(rect.height).unwrap_or(rect.height / 2.0),
            );
            let radius = radial_radii(*shape, size, center, rect);
            (
                GradientGeometry::Radial { center, radius },
                radius.0.max(1.0),
            )
        }
    };
    out.0.push(Command::FillGradient {
        rect,
        geometry,
        stops: resolve_stops(&gradient.stops, reference),
        repeating: gradient.repeating,
    });
}

/// The CSS angle (degrees; `0` points up, `90` points right) of a direction.
///
/// Corner directions are perpendicular to the diagonal between their two
/// neighbouring corners, so a corner keyword depends on the box's aspect ratio.
fn linear_angle(direction: LinearDirection, width: f32, height: f32) -> f32 {
    match direction {
        LinearDirection::Angle(angle) => angle,
        LinearDirection::ToTop => 0.0,
        LinearDirection::ToRight => 90.0,
        LinearDirection::ToBottom => 180.0,
        LinearDirection::ToLeft => 270.0,
        LinearDirection::ToTopRight => height.atan2(width).to_degrees(),
        LinearDirection::ToBottomRight => 180.0 - height.atan2(width).to_degrees(),
        LinearDirection::ToBottomLeft => 180.0 + height.atan2(width).to_degrees(),
        LinearDirection::ToTopLeft => 360.0 - height.atan2(width).to_degrees(),
    }
}

/// The ending shape's `(x, y)` radii for a radial gradient.
fn radial_radii(shape: RadialShape, size: &RadialSize, center: Point, rect: Rect) -> (f32, f32) {
    let cx = center.x - rect.x;
    let cy = center.y - rect.y;
    let (width, height) = (rect.width.max(0.0), rect.height.max(0.0));
    let circle = shape == RadialShape::Circle;
    match size {
        RadialSize::ClosestSide => {
            let (dx, dy) = (cx.min(width - cx).max(0.0), cy.min(height - cy).max(0.0));
            if circle {
                let radius = dx.min(dy);
                (radius, radius)
            } else {
                (dx, dy)
            }
        }
        RadialSize::FarthestSide => {
            let (dx, dy) = (cx.max(width - cx).max(0.0), cy.max(height - cy).max(0.0));
            if circle {
                let radius = dx.max(dy);
                (radius, radius)
            } else {
                (dx, dy)
            }
        }
        RadialSize::ClosestCorner => {
            let (dx, dy) = (cx.min(width - cx).max(0.0), cy.min(height - cy).max(0.0));
            if circle {
                let radius = (dx * dx + dy * dy).sqrt();
                (radius, radius)
            } else {
                (dx, dy)
            }
        }
        RadialSize::FarthestCorner => {
            let (dx, dy) = (cx.max(width - cx).max(0.0), cy.max(height - cy).max(0.0));
            if circle {
                let radius = (dx * dx + dy * dy).sqrt();
                (radius, radius)
            } else {
                (dx, dy)
            }
        }
        RadialSize::Explicit { x, y } => {
            let radius_x = x.resolve(width).unwrap_or(width).max(0.0);
            let radius_y = y
                .as_ref()
                .map(|radius| radius.resolve(height).unwrap_or(height).max(0.0))
                .unwrap_or(radius_x);
            if circle {
                (radius_x, radius_x)
            } else {
                (radius_x, radius_y)
            }
        }
    }
}

/// Resolve stop positions against `reference` and apply the CSS fix-up:
/// clamp into `[0, 1]`, keep offsets non-decreasing, default the first to `0`
/// and the last to `1`, and space unstated stops evenly between their stated
/// neighbours.
fn resolve_stops(stops: &[noonoo_style::GradientStop], reference: f32) -> Vec<GradientStop> {
    let reference = if reference > 0.0 { reference } else { 1.0 };
    let mut offsets: Vec<Option<f32>> = stops
        .iter()
        .map(|stop| {
            stop.position.as_ref().map(|position| {
                (position.resolve(reference).unwrap_or(0.0) / reference).clamp(0.0, 1.0)
            })
        })
        .collect();
    if offsets.len() < 2 {
        return Vec::new();
    }
    if offsets[0].is_none() {
        offsets[0] = Some(0.0);
    }
    let last = offsets.len() - 1;
    if offsets[last].is_none() {
        offsets[last] = Some(1.0);
    }

    // Keep offsets non-decreasing.
    let mut previous = 0.0;
    for offset in offsets.iter_mut().flatten() {
        *offset = offset.max(previous);
        previous = *offset;
    }

    // Distribute runs of unstated positions evenly.
    let mut index = 0;
    while index < offsets.len() {
        if offsets[index].is_none() {
            let start = index;
            while index < offsets.len() && offsets[index].is_none() {
                index += 1;
            }
            let (left, right) = (
                offsets[start - 1].unwrap_or(0.0),
                offsets[index.min(last)].unwrap_or(1.0),
            );
            let count = (index - start + 1) as f32;
            for (step, slot) in (start..index).enumerate() {
                offsets[slot] = Some(left + (right - left) * (step + 1) as f32 / count);
            }
        } else {
            index += 1;
        }
    }

    stops
        .iter()
        .zip(offsets)
        .map(|(stop, offset)| GradientStop {
            offset: offset.unwrap_or(0.0),
            color: stop.color,
        })
        .collect()
}

/// The box's `border-radius`, resolved against `rect` and clamped per CSS:
/// when the two radii on one side overlap, that side's pair scales down so
/// adjacent curves never exceed the side.
fn corner_radii(style: &noonoo_style::ComputedStyle, rect: Rect) -> (f32, f32, f32, f32) {
    let resolve = |length: &noonoo_style::Length| length.resolve(rect.width).max(0.0);
    let radii = &style.border_radius;
    let (tl, tr, br, bl) = (
        resolve(&radii.top_left),
        resolve(&radii.top_right),
        resolve(&radii.bottom_right),
        resolve(&radii.bottom_left),
    );
    let scale = |a: f32, b: f32, side: f32| -> (f32, f32) {
        let sum = a + b;
        if sum > side && sum > 0.0 {
            (a * side / sum, b * side / sum)
        } else {
            (a, b)
        }
    };
    let (tl, tr) = scale(tl, tr, rect.width);
    let (bl, br) = scale(bl, br, rect.width);
    let (tl, bl) = scale(tl, bl, rect.height);
    let (tr, br) = scale(tr, br, rect.height);
    (tl, tr, br, bl)
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

    /// `css` after a reset of the UA's `body` margin: the parser gives every
    /// snippet a `<body>`, and a test's boxes should start at the origin.
    fn test_sheet(css: &str) -> Result<noonoo_css::Stylesheet, impl std::fmt::Debug> {
        noonoo_css::Stylesheet::parse(&format!("body {{ margin: 0 }} {css}"))
    }

    fn render(html: &str, css: &str) -> CommandList {
        let doc = noonoo_html::parse(html);
        let sheet = test_sheet(css).unwrap();
        let styles = noonoo_style::compute(&doc, &[sheet]);
        let layout = noonoo_layout::layout(&doc, &styles, Size::new(400.0, 300.0), &MetricShaper);
        paint(&styles, &layout)
    }

    /// A PNG signature and IHDR chunk for a `width` × `height` image.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut data = b"\x89PNG\r\n\x1a\n".to_vec();
        data.extend(13u32.to_be_bytes());
        data.extend(b"IHDR");
        data.extend(width.to_be_bytes());
        data.extend(height.to_be_bytes());
        data.extend([8, 6, 0, 0, 0]);
        data
    }

    #[test]
    fn a_loaded_image_draws_over_its_content_box() {
        let doc = noonoo_html::parse(r#"<img id="a" src="a.png"><img src="missing.png" alt="x">"#);
        let sheet = test_sheet(
            "img { padding: 2px; border: 1px solid #000; background-color: #ff0000; \
             margin: 0 }",
        )
        .unwrap();
        let styles = noonoo_style::compute(&doc, &[sheet]);
        let mut images = noonoo_layout::Images::new();
        let image = noonoo_layout::Image::from_bytes(png(40, 30)).unwrap();
        images.insert("a.png", image.clone());
        let layout = noonoo_layout::layout_with_images(
            &doc,
            &styles,
            Size::new(400.0, 300.0),
            &MetricShaper,
            &images,
        );
        let list = paint(&styles, &layout);
        let drawn: Vec<(Rect, &EncodedImage)> = list
            .commands()
            .iter()
            .filter_map(|c| match c {
                Command::DrawImage { rect, image } => Some((*rect, image)),
                _ => None,
            })
            .collect();
        assert_eq!(
            drawn,
            vec![(Rect::new(3.0, 3.0, 40.0, 30.0), &image.encoded)]
        );
        // Over the background and border, not under them.
        let position =
            |pred: &dyn Fn(&Command) -> bool| list.commands().iter().position(pred).unwrap();
        let image_at = position(&|c| matches!(c, Command::DrawImage { .. }));
        let background_at = position(
            &|c| matches!(c, Command::FillRect { color, .. } if *color == Color::rgb(255, 0, 0)),
        );
        assert!(background_at < image_at);
    }

    #[test]
    fn a_scrolled_image_moves_with_its_container() {
        let doc = noonoo_html::parse(r#"<div id="s"><img src="a.png"><div id="tall"></div></div>"#);
        let sheet = test_sheet(
            "#s { height: 50px; overflow: auto } #tall { height: 200px } img { display: block }",
        )
        .unwrap();
        let styles = noonoo_style::compute(&doc, &[sheet]);
        let mut images = noonoo_layout::Images::new();
        images.insert(
            "a.png",
            noonoo_layout::Image::from_bytes(png(40, 30)).unwrap(),
        );
        let mut layout = noonoo_layout::layout_with_images(
            &doc,
            &styles,
            Size::new(400.0, 300.0),
            &MetricShaper,
            &images,
        );
        layout.scroll_by(doc.get_element_by_id("s").unwrap(), 10.0);
        let list = paint(&styles, &layout);
        let rect = list.commands().iter().find_map(|c| match c {
            Command::DrawImage { rect, .. } => Some(*rect),
            _ => None,
        });
        assert_eq!(rect, Some(Rect::new(0.0, -10.0, 40.0, 30.0)));
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
    fn paints_text_transformed() {
        let list = render(
            r#"<div id="a">hi there</div>"#,
            "#a { text-transform: uppercase }",
        );
        let texts: Vec<&str> = list
            .commands()
            .iter()
            .filter_map(|c| match c {
                Command::DrawText { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["HI", "THERE"]);
    }

    /// Records the letter spacing each run is drawn with.
    struct Spacing(std::cell::RefCell<Vec<f32>>);

    impl Glyphs for Spacing {
        fn draw(
            &self,
            _: &str,
            _: (f32, f32),
            _: f32,
            _: Color,
            _: FontRequest,
            _: &mut CommandList,
        ) {
            self.0.borrow_mut().push(0.0);
        }

        fn draw_spaced(
            &self,
            _: &str,
            _: (f32, f32),
            _: f32,
            letter_spacing: f32,
            _: Color,
            _: FontRequest,
            _: &mut CommandList,
        ) {
            self.0.borrow_mut().push(letter_spacing);
        }
    }

    #[test]
    fn text_is_drawn_with_its_letter_spacing() {
        let doc = noonoo_html::parse(r#"<div id="a">hi</div>"#);
        let sheet = test_sheet("#a { letter-spacing: 3px }").unwrap();
        let styles = noonoo_style::compute(&doc, &[sheet]);
        let layout = noonoo_layout::layout(&doc, &styles, Size::new(400.0, 300.0), &MetricShaper);
        let glyphs = Spacing(Default::default());
        paint_with(&styles, &layout, &glyphs);
        assert_eq!(*glyphs.0.borrow(), [3.0]);
    }

    fn transforms(list: &CommandList) -> Vec<[f32; 6]> {
        list.commands()
            .iter()
            .filter_map(|c| match c {
                Command::PushTransform { matrix } => Some(*matrix),
                _ => None,
            })
            .collect()
    }

    fn close(a: [f32; 6], b: [f32; 6]) -> bool {
        a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn a_transform_wraps_the_box_and_its_children() {
        let list = render(
            r#"<div id="a"><p id="b">x</p></div><div id="c"></div>"#,
            "div { height: 50px } #a { width: 100px; background: red; transform: translateX(-50%) }
             #c { background: blue }",
        );
        assert_eq!(transforms(&list), [[1.0, 0.0, 0.0, 1.0, -50.0, 0.0]]);
        let commands = list.commands();
        let push = commands
            .iter()
            .position(|c| matches!(c, Command::PushTransform { .. }))
            .unwrap();
        let pop = commands
            .iter()
            .position(|c| matches!(c, Command::PopTransform))
            .unwrap();
        let red = commands
            .iter()
            .position(
                |c| matches!(c, Command::FillRect { color, .. } if *color == Color::rgb(255, 0, 0)),
            )
            .unwrap();
        let text = commands
            .iter()
            .position(|c| matches!(c, Command::DrawText { .. }))
            .unwrap();
        let blue = commands
            .iter()
            .position(
                |c| matches!(c, Command::FillRect { color, .. } if *color == Color::rgb(0, 0, 255)),
            )
            .unwrap();
        assert!(push < red && red < text && text < pop && pop < blue);
    }

    /// Scale and rotate act about the border box's centre.
    #[test]
    fn scale_and_rotate_act_about_the_box_centre() {
        let list = render(
            r#"<div id="a"></div>"#,
            "#a { width: 100px; height: 50px; transform: scale(2) }",
        );
        assert!(close(
            transforms(&list)[0],
            [2.0, 0.0, 0.0, 2.0, -50.0, -25.0]
        ));

        let list = render(
            r#"<div id="a"></div>"#,
            "#a { width: 100px; height: 50px; transform: rotate(90deg) }",
        );
        // About (50, 25): x' = -y + 75, y' = x - 25.
        assert!(close(
            transforms(&list)[0],
            [0.0, 1.0, -1.0, 0.0, 75.0, -25.0]
        ));
    }

    #[test]
    fn transforms_serialize() {
        let list = CommandList(vec![
            Command::PushTransform {
                matrix: [1.0, 0.0, 0.0, 1.0, 5.0, 6.0],
            },
            Command::PopTransform,
        ]);
        let json = list.to_json().to_string();
        assert!(json.contains(r#""op":"push-transform""#), "{json}");
        assert!(
            json.contains(r#""matrix":[1.0,0.0,0.0,1.0,5.0,6.0]"#),
            "{json}"
        );
        assert!(json.contains(r#""op":"pop-transform""#), "{json}");
    }

    #[test]
    fn outer_box_shadows_paint_under_the_background_last_first() {
        let list = render(
            r#"<div id="a"></div>"#,
            "#a { width: 100px; height: 50px; background: white; border-radius: 4px;
                  color: red; box-shadow: 2px 3px 4px 1px, 0 0 0 6px blue, inset 0 0 2px green }",
        );
        let commands = list.commands();
        let shadows: Vec<_> = commands
            .iter()
            .enumerate()
            .filter_map(|(i, c)| match c {
                Command::BoxShadow {
                    rect,
                    radius,
                    blur,
                    color,
                    element,
                    element_radius,
                } => Some((i, *rect, *radius, *blur, *color, *element, *element_radius)),
                _ => None,
            })
            .collect();
        assert_eq!(shadows.len(), 2, "inset shadows are not painted yet");
        let element = Rect::new(0.0, 0.0, 100.0, 50.0);
        // The first shadow listed is on top, so it paints last.
        assert_eq!(shadows[0].1, Rect::new(-6.0, -6.0, 112.0, 62.0));
        assert_eq!(shadows[0].2, (10.0, 10.0, 10.0, 10.0));
        assert_eq!(shadows[0].3, 0.0);
        assert_eq!(shadows[0].4, Color::rgb(0, 0, 255));
        assert_eq!(shadows[1].1, Rect::new(1.0, 2.0, 102.0, 52.0));
        assert_eq!(shadows[1].2, (5.0, 5.0, 5.0, 5.0));
        assert_eq!(shadows[1].3, 4.0);
        assert_eq!(shadows[1].4, Color::rgb(255, 0, 0));
        for shadow in &shadows {
            assert_eq!(shadow.5, element);
            assert_eq!(shadow.6, (4.0, 4.0, 4.0, 4.0));
        }
        let background = commands
            .iter()
            .position(|c| matches!(c, Command::FillRect { color, .. } if *color == Color::WHITE))
            .unwrap();
        assert!(shadows[1].0 < background);
        let json = list.to_json().to_string();
        assert!(json.contains(r#""op":"box-shadow""#), "{json}");
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
    fn paints_a_linear_gradient() {
        let list = render(
            r#"<div id="a"></div>"#,
            "#a { width: 400px; height: 100px; background: linear-gradient(to right, red, blue) }",
        );
        let gradients: Vec<_> = list
            .commands()
            .iter()
            .filter_map(|c| match c {
                Command::FillGradient {
                    rect,
                    geometry,
                    stops,
                    ..
                } => Some((*rect, geometry.clone(), stops.clone())),
                _ => None,
            })
            .collect();
        let [(rect, geometry, stops)] = gradients.as_slice() else {
            panic!("expected one gradient");
        };
        // A 400x100 box: `to right` runs left edge to right edge at mid-height.
        assert_eq!((rect.width, rect.height), (400.0, 100.0));
        let GradientGeometry::Linear { from, to } = geometry else {
            panic!("expected a linear geometry");
        };
        assert_eq!((from.x, to.x), (0.0, 400.0));
        assert!(
            (from.y - to.y).abs() < 1e-3,
            "horizontal line: {from:?} vs {to:?}"
        );
        assert_eq!(
            stops.as_slice(),
            &[
                GradientStop {
                    offset: 0.0,
                    color: Color::rgb(255, 0, 0)
                },
                GradientStop {
                    offset: 1.0,
                    color: Color::rgb(0, 0, 255)
                },
            ]
        );
    }

    #[test]
    fn a_corner_direction_depends_on_the_aspect_ratio() {
        let list = render(
            r#"<div id="a"></div>"#,
            "#a { width: 200px; height: 100px; background: linear-gradient(to top right, red, blue) }",
        );
        let Some(Command::FillGradient { geometry, .. }) = list
            .commands()
            .iter()
            .find(|c| matches!(c, Command::FillGradient { .. }))
        else {
            panic!("expected a gradient");
        };
        let GradientGeometry::Linear { from, to } = geometry else {
            panic!("expected a linear geometry");
        };
        // CSS angle atan2(h, w) = 26.57 degrees from *up*, i.e. 63.43 degrees
        // above the horizontal: on a wide box, `to top right` is steep, because
        // the line perpendicular to the gradient must pass through the two
        // neighbouring corners (the box's main diagonal).
        let angle = (to.y - from.y).atan2(to.x - from.x).to_degrees();
        assert!((angle + 63.43).abs() < 0.1, "got {angle}");
    }

    #[test]
    fn stop_positions_fix_up_like_css() {
        let cases = [
            ("linear-gradient(red, blue)", vec![(0.0, 255), (1.0, 0)]),
            (
                "linear-gradient(red 25%, blue)",
                vec![(0.25, 255), (1.0, 0)],
            ),
            (
                "linear-gradient(red, blue 75%)",
                vec![(0.0, 255), (0.75, 0)],
            ),
            // Clamped: an out-of-order pair becomes equal.
            (
                "linear-gradient(red 50%, blue 25%)",
                vec![(0.5, 255), (0.5, 0)],
            ),
            // Even spacing between stated neighbours.
            (
                "linear-gradient(red, blue, green 80%)",
                vec![(0.0, 255), (0.4, 0), (0.8, 0)],
            ),
        ];
        for (css, expected) in cases {
            let list = render(
                r#"<div id="a"></div>"#,
                &format!("#a {{ background: {css} }}"),
            );
            let Some(Command::FillGradient { stops, .. }) = list
                .commands()
                .iter()
                .find(|c| matches!(c, Command::FillGradient { .. }))
            else {
                panic!("expected a gradient for {css}");
            };
            let offsets: Vec<(f64, u8)> =
                stops.iter().map(|s| (s.offset as f64, s.color.r)).collect();
            assert_eq!(offsets.len(), expected.len(), "for {css}");
            for ((offset, red), (expected_offset, expected_red)) in offsets.iter().zip(&expected) {
                assert_eq!(red, expected_red, "for {css}");
                assert!(
                    (offset - expected_offset).abs() < 1e-6,
                    "offset {offset} != {expected_offset} for {css}"
                );
            }
        }
    }

    #[test]
    fn paints_a_radial_gradient() {
        let list = render(
            r#"<div id="a"></div>"#,
            "#a { width: 200px; height: 100px; background: radial-gradient(circle 50px at 10px 10px, red, blue) }",
        );
        let Some(Command::FillGradient {
            geometry,
            repeating,
            ..
        }) = list
            .commands()
            .iter()
            .find(|c| matches!(c, Command::FillGradient { .. }))
        else {
            panic!("expected a gradient");
        };
        assert!(!repeating);
        let GradientGeometry::Radial { center, radius } = geometry else {
            panic!("expected a radial geometry");
        };
        assert_eq!((center.x, center.y), (10.0, 10.0));
        assert_eq!(*radius, (50.0, 50.0));
    }

    #[test]
    fn radial_sizes_resolve_against_the_box() {
        let list = render(
            r#"<div id="a"></div>"#,
            "#a { width: 200px; height: 100px; background: radial-gradient(ellipse closest-side, red, blue) }",
        );
        let Some(Command::FillGradient { geometry, .. }) = list
            .commands()
            .iter()
            .find(|c| matches!(c, Command::FillGradient { .. }))
        else {
            panic!("expected a gradient");
        };
        let GradientGeometry::Radial { center, radius } = geometry else {
            panic!("expected a radial geometry");
        };
        // Centered: closest side is half the width / half the height.
        assert_eq!((center.x, center.y), (100.0, 50.0));
        assert_eq!(*radius, (100.0, 50.0));
    }

    #[test]
    fn a_background_gradient_paints_over_the_colour() {
        let list = render(
            r#"<div id="a"></div>"#,
            "#a { background: linear-gradient(red, blue) #123456 }",
        );
        let commands = list.commands();
        let fill_index = commands
            .iter()
            .position(|c| matches!(c, Command::FillRect { color, .. } if *color == Color::rgb(0x12, 0x34, 0x56)))
            .expect("background colour behind the gradient");
        assert!(matches!(
            commands.get(fill_index + 1),
            Some(Command::FillGradient { .. })
        ));
    }

    #[test]
    fn opacity_composites_a_subtree_as_one_group() {
        let list = render(
            r#"<div id="a"><div id="b"></div></div>"#,
            "#a { opacity: 0.5; background: red } #b { width: 10px; height: 10px; background: blue }",
        );
        let commands = list.commands();
        let push = commands
            .iter()
            .position(|c| matches!(c, Command::PushLayer { clip: None, alpha } if (*alpha - 0.5).abs() < 1e-6))
            .expect("an alpha group opens before the box");
        let fill = commands
            .iter()
            .position(
                |c| matches!(c, Command::FillRect { color, .. } if *color == Color::rgb(255, 0, 0)),
            )
            .expect("the box paints inside the group");
        let pop = commands
            .iter()
            .skip(fill)
            .position(|c| matches!(c, Command::PopLayer))
            .expect("the group closes after the subtree");
        assert!(push < fill);
        assert!(pop > fill);
    }

    #[test]
    fn border_radius_rounds_and_clamps() {
        let list = render(
            r#"<div id="a"></div>"#,
            "#a { width: 100px; height: 50px; background: red; border-radius: 100px }",
        );
        let Some(Command::FillRect { radius, .. }) = list.commands().iter().find(
            |c| matches!(c, Command::FillRect { color, .. } if *color == Color::rgb(255, 0, 0)),
        ) else {
            panic!("expected the background fill");
        };
        // CSS 5.5: adjacent radii may not exceed a side, so 100px scales to 50
        // on the width and 25 on the height.
        assert_eq!(*radius, (25.0, 25.0, 25.0, 25.0));
    }

    /// A ring's outer rect and radii, inner rect and radii, and colour.
    type Ring = (
        Rect,
        (f32, f32, f32, f32),
        Rect,
        (f32, f32, f32, f32),
        Color,
    );

    fn rings(list: &CommandList) -> Vec<Ring> {
        list.commands()
            .iter()
            .filter_map(|c| match c {
                Command::FillRing {
                    outer,
                    outer_radius,
                    inner,
                    inner_radius,
                    color,
                } => Some((*outer, *outer_radius, *inner, *inner_radius, *color)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_rounded_border_paints_as_one_ring() {
        let list = render(
            r#"<div id="a"></div>"#,
            "#a { width: 96px; height: 46px; border: 2px solid blue; border-left-width: 6px;
                  border-radius: 10px }",
        );
        let blue = Color::rgb(0, 0, 255);
        assert_eq!(
            rings(&list),
            [(
                Rect::new(0.0, 0.0, 104.0, 50.0),
                (10.0, 10.0, 10.0, 10.0),
                Rect::new(6.0, 2.0, 96.0, 46.0),
                // Each inner corner gives up the wider of its two borders.
                (4.0, 8.0, 8.0, 4.0),
                blue,
            )]
        );
        // No square bands as well.
        assert!(
            !list
                .commands()
                .iter()
                .any(|c| matches!(c, Command::FillRect { color, .. } if *color == blue))
        );
        let json = list.to_json().to_string();
        assert!(json.contains(r#""op":"fill-ring""#), "{json}");
    }

    #[test]
    fn square_or_multicoloured_borders_keep_their_bands() {
        let square = render(
            r#"<div id="a"></div>"#,
            "#a { height: 10px; border: 2px solid blue }",
        );
        assert!(rings(&square).is_empty());
        let mixed = render(
            r#"<div id="a"></div>"#,
            "#a { height: 10px; border: 2px solid blue; border-top-color: red; border-radius: 4px }",
        );
        assert!(rings(&mixed).is_empty());
        // A side with no width doesn't count against "one colour".
        let partial = render(
            r#"<div id="a"></div>"#,
            "#a { height: 10px; border-bottom: 2px solid blue; border-radius: 4px }",
        );
        assert_eq!(rings(&partial).len(), 1);
    }

    #[test]
    fn json_carries_a_radius_only_when_rounded() {
        let sharp = render(r#"<div id="a"></div>"#, "#a { background: red }");
        assert!(!sharp.to_json().to_string().contains("radius"));

        let round = render(
            r#"<div id="a"></div>"#,
            "#a { background: red; border-radius: 8px; height: 20px }",
        );
        let json = round.to_json().to_string();
        assert!(json.contains("radius"));
    }

    /// An image records where it goes, its format and its size, not its
    /// bytes: the JSON stays a readable diff.
    #[test]
    fn json_records_an_image_by_format_and_size() {
        let list = CommandList(vec![Command::DrawImage {
            rect: Rect::new(1.0, 2.0, 3.0, 4.0),
            image: EncodedImage::png(vec![0x89, b'P', b'N', b'G']),
        }]);
        let json = list.to_json().to_string();
        assert!(json.contains(r#""op":"draw-image""#), "{json}");
        assert!(json.contains(r#""format":"png""#), "{json}");
        assert!(json.contains(r#""bytes":4"#), "{json}");
        assert!(json.contains(r#""width":3"#), "{json}");
    }

    #[test]
    fn json_records_each_contour_of_a_shape() {
        let list = CommandList(vec![Command::FillContours {
            contours: vec![
                vec![
                    Point::new(0.0, 0.0),
                    Point::new(4.0, 0.0),
                    Point::new(4.0, 4.0),
                ],
                vec![
                    Point::new(1.0, 1.0),
                    Point::new(2.0, 1.0),
                    Point::new(2.0, 2.0),
                ],
            ],
            color: Color::BLACK,
        }]);
        let json = list.to_json().to_string();
        assert!(json.contains(r#""op":"fill-contours""#), "{json}");
        assert!(json.contains(r#""contours":[[{"x":0.0,"y":0.0}"#), "{json}");
        assert_eq!(json.matches(r#""x":"#).count(), 6, "{json}");
    }

    #[test]
    fn json_is_stable() {
        let list = render(r#"<div>x</div>"#, "#a { }");
        let json = list.to_json().to_string();
        assert!(json.starts_with('['));
        assert!(json.contains("draw-text"));
    }

    #[test]
    fn paint_with_uses_the_glyph_renderer() {
        struct Marks;
        impl Glyphs for Marks {
            fn draw(
                &self,
                _text: &str,
                _origin: (f32, f32),
                _font_size: f32,
                color: Color,
                _request: noonoo_text::FontRequest,
                out: &mut CommandList,
            ) {
                out.0.push(Command::FillRect {
                    rect: Rect::new(0.0, 0.0, 1.0, 1.0),
                    color,
                    radius: (0.0, 0.0, 0.0, 0.0),
                });
            }
        }

        let doc = noonoo_html::parse(r#"<div style="color: #ff0000">hi</div>"#);
        let styles = noonoo_style::compute(&doc, &[]);
        let layout = noonoo_layout::layout(&doc, &styles, Size::new(100.0, 100.0), &MetricShaper);
        let list = paint_with(&styles, &layout, &Marks);

        // The renderer replaced the default `DrawText` with its own command.
        assert!(list.commands().iter().any(
            |c| matches!(c, Command::FillRect { color, .. } if *color == Color::rgb(255, 0, 0))
        ));
        assert!(
            !list
                .commands()
                .iter()
                .any(|c| matches!(c, Command::DrawText { .. }))
        );
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

    #[test]
    fn overflow_hidden_emits_a_clip() {
        let list = render(
            r#"<div id="clip"><div id="inner">x</div></div>"#,
            "#clip { width: 50px; height: 20px; overflow: hidden }",
        );
        assert!(
            list.commands()
                .iter()
                .any(|c| matches!(c, Command::PushLayer { clip: Some(_), .. }))
        );
        assert!(
            list.commands()
                .iter()
                .any(|c| matches!(c, Command::PopLayer))
        );
        assert!(list.to_json().to_string().contains("push-layer"));
    }

    #[test]
    fn scrolling_shifts_the_content_in_the_command_stream() {
        let doc = noonoo_html::parse(r#"<div id="list"><div id="row"></div></div>"#);
        let sheet = test_sheet(
            "#list { width: 100px; height: 50px; overflow-y: auto } \
             #row { height: 200px; background-color: #ff0000 }",
        )
        .unwrap();
        let styles = noonoo_style::compute(&doc, &[sheet]);
        let mut layout =
            noonoo_layout::layout(&doc, &styles, Size::new(400.0, 300.0), &MetricShaper);
        let list = doc.get_element_by_id("list").unwrap();

        assert_eq!(red_rect_y(&paint(&styles, &layout)), 0.0);
        layout.scroll_by(list, 30.0);
        // The row's background moved up by the scroll offset.
        assert_eq!(red_rect_y(&paint(&styles, &layout)), -30.0);
    }

    fn red_rect_y(list: &CommandList) -> f32 {
        list.commands()
            .iter()
            .find_map(|c| match c {
                Command::FillRect { rect, color, .. } if *color == Color::rgb(255, 0, 0) => {
                    Some(rect.y)
                }
                _ => None,
            })
            .expect("a red rect")
    }

    #[test]
    fn horizontal_scroll_shifts_content_in_the_command_stream() {
        let doc = noonoo_html::parse(r#"<div id="strip"><div id="wide"></div></div>"#);
        let sheet = test_sheet(
            "#strip { width: 50px; height: 20px; overflow-x: auto } \
             #wide { width: 200px; height: 10px; background-color: #ff0000 }",
        )
        .unwrap();
        let styles = noonoo_style::compute(&doc, &[sheet]);
        let mut layout =
            noonoo_layout::layout(&doc, &styles, Size::new(400.0, 300.0), &MetricShaper);
        let strip = doc.get_element_by_id("strip").unwrap();

        assert_eq!(red_rect_x(&paint(&styles, &layout)), 0.0);
        layout.scroll_by_x(strip, 30.0);
        assert_eq!(red_rect_x(&paint(&styles, &layout)), -30.0);
    }

    fn red_rect_x(list: &CommandList) -> f32 {
        list.commands()
            .iter()
            .find_map(|c| match c {
                Command::FillRect { rect, color, .. } if *color == Color::rgb(255, 0, 0) => {
                    Some(rect.x)
                }
                _ => None,
            })
            .expect("a red rect")
    }

    #[test]
    fn z_index_orders_positioned_siblings() {
        let list = render(
            r#"<div id="a"></div><div id="b"></div>"#,
            "#a { position: absolute; top: 0; left: 0; width: 10px; height: 10px; \
                  background-color: #ff0000; z-index: 2 } \
             #b { position: absolute; top: 0; left: 0; width: 10px; height: 10px; \
                  background-color: #00ff00; z-index: 1 }",
        );
        // #a has the higher z-index, so it paints last (on top).
        assert!(
            index_of_color(&list, Color::rgb(255, 0, 0))
                > index_of_color(&list, Color::rgb(0, 255, 0))
        );
    }

    #[test]
    fn a_positioned_box_paints_above_in_flow_content() {
        let list = render(
            r#"<div id="flow"></div><div id="over"></div>"#,
            "#flow { height: 10px; background-color: #00ff00 } \
             #over { position: absolute; top: 0; left: 0; width: 10px; height: 10px; \
                     background-color: #ff0000 }",
        );
        assert!(
            index_of_color(&list, Color::rgb(255, 0, 0))
                > index_of_color(&list, Color::rgb(0, 255, 0))
        );
    }

    #[test]
    fn a_negative_z_index_paints_below_in_flow_content() {
        let list = render(
            r#"<div id="under"></div><div id="flow"></div>"#,
            "#under { position: absolute; top: 0; left: 0; width: 10px; height: 10px; \
                      background-color: #ff0000; z-index: -1 } \
             #flow { height: 10px; background-color: #00ff00 }",
        );
        assert!(
            index_of_color(&list, Color::rgb(255, 0, 0))
                < index_of_color(&list, Color::rgb(0, 255, 0))
        );
    }

    fn index_of_color(list: &CommandList, color: Color) -> usize {
        list.commands()
            .iter()
            .position(|c| matches!(c, Command::FillRect { color: c, .. } if *c == color))
            .expect("a rect of that colour")
    }

    #[test]
    fn a_realistic_page_survives_the_whole_pipeline() {
        // The review's closing suggestion (`docs/REVIEW-2026-11-07.md`):
        // test realistic content — media rules, flex buttons, entities,
        // tables — not only per-feature fixtures.
        let list = render(
            r#"<nav class="bar"><button>Save &amp; exit</button></nav>
    <h1>H<br>i</h1>
    <table><tr><td>a<td>b</tr></table>
    <p class="greeting">&copy; 2026 &mdash; hello</p>"#,
            "nav { display: flex; gap: 8px }
                 button { padding: 8px 16px; white-space: nowrap }
                 h1 { font-size: 2em; margin: 0 }
                 table { border-collapse: collapse } td { border: 1px solid #000 }
                 @media (min-width: 100px) { .greeting { color: blue } }
                 @media (min-width: 100000px) { .greeting { color: red } }",
        );
        let texts: Vec<&str> = list
            .commands()
            .iter()
            .filter_map(|command| match command {
                Command::DrawText { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        // Entities decode, `<br>` separates the heading, cells separate, and
        // no empty run appears anywhere. Inline runs paint one word per box,
        // so sentences assert word-wise.
        assert_eq!(texts.len(), 11, "{texts:?}");
        assert!(texts.contains(&"&"), "the `&amp;` decodes");
        let a = texts.contains(&"a") && texts.contains(&"b");
        assert!(a, "table cells stay separate: {texts:?}");
        assert!(texts.iter().all(|t| !t.is_empty()));
        // The matching media rule wins; the impossible one contributes
        // nothing.
        assert!(
            list.commands().iter().any(|command| matches!(
                command,
                Command::DrawText { text, color, .. }
                    if *text == "©" && *color == Color::rgb(0, 0, 255)
            )),
            "{texts:?}"
        );
    }

    #[test]
    fn empty_text_runs_are_dropped() {
        // Finding 7 of docs/REVIEW-2026-11-07.md: a `<br>` used to emit a
        // `draw-text` with empty text; an `<img>` without `alt`, too.
        let list = render(r#"<p>H<br><img src="x" alt=""></p>"#, "p { margin: 0 }");
        assert!(
            list.commands()
                .iter()
                .any(|c| matches!(c, Command::DrawText { text, .. } if text == "H"))
        );
        assert!(
            list.commands()
                .iter()
                .all(|c| !matches!(c, Command::DrawText { text, .. } if text.is_empty())),
            "no empty text runs: {:?}",
            list.commands()
        );
    }
}
