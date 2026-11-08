//! # noonoo-vello
//!
//! The opt-in **GPU pass**: translate a [`noonoo_paint::CommandList`] into a
//! `vello::Scene`, ready for Vello's wgpu renderer (and for `mustang`'s effect
//! compositor, which consumes a `vello::Scene`).
//!
//! This is the "one module per borrowed engine" boundary (`docs/DEPENDENCIES.md`):
//! Vello is named *here* and nowhere in the engine. It lives outside the workspace
//! so the engine's lockfile stays free of the GPU stack.
//!
//! ## Mapping
//!
//! | Noonoo command | Vello |
//! |---|---|
//! | `FillRect` | `Scene::fill` a `kurbo::Rect` (rounded corners become a path) |
//! | `FillGradient` | `Scene::fill` a `kurbo::Rect` with a `peniko::Gradient` (elliptical radial gradients use a brush transform) |
//! | `FillPolygon` | `Scene::fill` a closed `kurbo::BezPath` |
//! | `FillContours` | one `Scene::fill` (non-zero) of a `BezPath` with a subpath per contour, so glyph holes stay open |
//! | `DrawImage` | the PNG decoded (once per distinct image) to RGBA8, then `Scene::draw_image` scaled into its rect; bytes that don't decode draw nothing |
//! | `PushLayer` / `PopLayer` | `Scene::push_layer` (alpha + clip) / `pop_layer` |
//! | `DrawText` | a placeholder box per character: text should arrive as glyph outlines (`noonoo-ttf`'s `OutlineGlyphs`, with the faces from [`fonts`]), so a bare run means it was painted without a font |
//!
//! ## Text
//!
//! Vello draws paths, so text has to arrive as glyph outlines. Paint with a
//! font-backed shaper and glyph renderer over one font set, so text is drawn
//! with the faces it was measured against. [`fonts::system`] finds the usual
//! system faces:
//!
//! ```ignore
//! let set = noonoo_vello::fonts::system().expect("a known font family");
//! let commands = engine.paint_with(
//!     viewport,
//!     &noonoo_ttf::FontSetShaper::new(set),
//!     &noonoo_ttf::OutlineGlyphs::new(set),
//! );
//! let scene = noonoo_vello::scene(&commands);
//! ```
//!
//! `Engine::paint` uses the default `TextRuns`, which emits bare `DrawText`
//! runs; those draw as placeholder boxes here.
//!
//! ```
//! let commands = noonoo_paint::CommandList(vec![noonoo_paint::Command::FillRect {
//!     rect: noonoo_geometry::Rect::new(0.0, 0.0, 10.0, 10.0),
//!     color: noonoo_paint::Color::rgb(255, 0, 0),
//!     radius: (0.0, 0.0, 0.0, 0.0),
//! }]);
//! let scene = noonoo_vello::scene(&commands);
//! assert!(!scene.encoding().path_tags.is_empty());
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod fonts;

use noonoo_geometry::Point;
use noonoo_paint::{Command, CommandList, GradientGeometry};
use vello::Scene;
use vello::kurbo::{self, Affine, BezPath, Rect as KRect};
use vello::peniko::{
    BlendMode, Blob, Color, ColorStop, Extend, Fill, Gradient, ImageAlphaType, ImageBrush,
    ImageData, ImageFormat,
};

/// Build a fresh Vello scene from a Noonoo command stream.
pub fn scene(commands: &CommandList) -> Scene {
    let mut scene = Scene::new();
    draw(&mut scene, commands);
    scene
}

/// Draw a Noonoo command stream into an existing Vello scene.
pub fn draw(scene: &mut Scene, commands: &CommandList) {
    // `PushTransform`s compose; every fill and layer uses the innermost.
    let mut transforms = vec![Affine::IDENTITY];
    for command in commands.commands() {
        let transform = *transforms.last().expect("the identity is never popped");
        match command {
            Command::FillRect {
                rect,
                color,
                radius,
            } => {
                let shape =
                    if radius.0 == 0.0 && radius.1 == 0.0 && radius.2 == 0.0 && radius.3 == 0.0 {
                        kurbo::Shape::into_path(kurbo_rect(*rect), 0.1)
                    } else {
                        rounded_rect(*rect, *radius)
                    };
                scene.fill(Fill::NonZero, transform, brush(*color), None, &shape);
            }
            Command::FillGradient {
                rect,
                geometry,
                stops,
                repeating,
            } => {
                if stops.is_empty() {
                    continue;
                }
                let brush = gradient(geometry, *repeating, stops);
                // Vello's radial brush is a circle; an ellipse scales it about
                // its centre through the brush transform.
                let brush_transform = match geometry {
                    GradientGeometry::Radial {
                        center,
                        radius: (rx, ry),
                    } if *rx > 0.0 && (*ry - *rx).abs() > f32::EPSILON => {
                        let (cx, cy) = (center.x as f64, center.y as f64);
                        Some(
                            Affine::translate((cx, cy))
                                * Affine::scale_non_uniform(1.0, (*ry / *rx) as f64)
                                * Affine::translate((-cx, -cy)),
                        )
                    }
                    _ => None,
                };
                let shape = kurbo_rect(*rect);
                scene.fill(Fill::NonZero, transform, &brush, brush_transform, &shape);
            }
            Command::FillPolygon { points, color } => {
                if points.len() >= 3 {
                    let path = bez_path(points);
                    scene.fill(Fill::NonZero, transform, brush(*color), None, &path);
                }
            }
            Command::FillContours { contours, color } => {
                // One path of several subpaths, one fill: the non-zero rule
                // cuts the holes a glyph winds the other way.
                let mut path = BezPath::new();
                for contour in contours.iter().filter(|contour| contour.len() >= 3) {
                    path.extend(bez_path(contour));
                }
                if !path.elements().is_empty() {
                    scene.fill(Fill::NonZero, transform, brush(*color), None, &path);
                }
            }
            // A plain text run carries no outlines: text reaches Vello as glyph
            // contours from `OutlineGlyphs` (see [`fonts`]). A run that gets
            // here was painted without a font, so it draws placeholder boxes
            // rather than vanish without a trace.
            Command::DrawText {
                text,
                x,
                y,
                font_size,
                color,
            } => placeholder_boxes(scene, transform, text, (*x, *y), *font_size, *color),
            Command::DrawImage { rect, image } => {
                // Undecodable bytes draw nothing, as a broken image would.
                if let Some(decoded) = decode(image) {
                    let to_rect = Affine::translate((f64::from(rect.x), f64::from(rect.y)))
                        * Affine::scale_non_uniform(
                            f64::from(rect.width) / f64::from(decoded.width),
                            f64::from(rect.height) / f64::from(decoded.height),
                        );
                    scene.draw_image(&ImageBrush::new(decoded), transform * to_rect);
                }
            }
            Command::PushLayer { clip, alpha } => {
                // No clip: an effectively infinite shape (Vello wants a shape
                // per layer), so the group covers the whole frame.
                let shape = match clip {
                    Some(rect) => kurbo::Shape::into_path(kurbo_rect(*rect), 0.1),
                    None => frame(),
                };
                scene.push_layer(
                    Fill::NonZero,
                    BlendMode::default(),
                    *alpha,
                    transform,
                    &shape,
                );
            }
            Command::PopLayer => scene.pop_layer(),
            Command::FillRing {
                outer,
                outer_radius,
                inner,
                inner_radius,
                color,
            } => {
                // Even-odd: the inner outline cuts the hole.
                let mut ring = rounded_rect(*outer, *outer_radius);
                ring.extend(rounded_rect(*inner, *inner_radius));
                scene.fill(Fill::EvenOdd, transform, brush(*color), None, &ring);
            }
            Command::BoxShadow {
                rect,
                radius,
                blur,
                color,
                element,
                element_radius,
            } => {
                // CSS never draws an outer shadow under its own box: clip to
                // everything but the element (even-odd: frame minus box).
                let mut outside = frame();
                outside.extend(rounded_rect(*element, *element_radius));
                scene.push_clip_layer(Fill::EvenOdd, transform, &outside);
                if *blur > 0.0 {
                    let corner = radius.0.max(radius.1).max(radius.2).max(radius.3);
                    scene.draw_blurred_rounded_rect(
                        transform,
                        kurbo_rect(*rect),
                        brush(*color),
                        corner as f64,
                        // The CSS blur radius is twice the Gaussian's
                        // standard deviation.
                        (*blur / 2.0) as f64,
                    );
                } else {
                    scene.fill(
                        Fill::NonZero,
                        transform,
                        brush(*color),
                        None,
                        &rounded_rect(*rect, *radius),
                    );
                }
                scene.pop_layer();
            }
            Command::PushTransform { matrix } => {
                let [a, b, c, d, e, f] = matrix.map(f64::from);
                transforms.push(transform * Affine::new([a, b, c, d, e, f]));
            }
            Command::PopTransform => {
                if transforms.len() > 1 {
                    transforms.pop();
                }
            }
        }
    }
}

/// Decoded images by their encoded bytes, so a glyph repainted every frame
/// decodes once and keeps one blob, which Vello's atlas keys its upload by.
static DECODED: std::sync::LazyLock<std::sync::Mutex<DecodedImages>> =
    std::sync::LazyLock::new(Default::default);

/// Encoded bytes to their decode; `None` remembers bytes that don't decode.
type DecodedImages = std::collections::HashMap<std::sync::Arc<[u8]>, Option<ImageData>>;

/// How many images [`DECODED`] holds before it starts over.
const DECODED_LIMIT: usize = 1024;

/// `image` as straight-alpha RGBA8 pixels, or `None` when it doesn't decode.
fn decode(image: &noonoo_paint::EncodedImage) -> Option<ImageData> {
    let mut cache = DECODED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(decoded) = cache.get(&image.data) {
        return decoded.clone();
    }
    if cache.len() >= DECODED_LIMIT {
        cache.clear();
    }
    let decoded = match image.format {
        noonoo_paint::ImageFormat::Png => decode_png(&image.data),
        noonoo_paint::ImageFormat::Jpeg => decode_jpeg(&image.data),
        noonoo_paint::ImageFormat::Gif => decode_gif(&image.data),
        noonoo_paint::ImageFormat::Webp => decode_webp(&image.data),
    };
    cache.insert(image.data.clone(), decoded.clone());
    decoded
}

/// The most pixels an image may decode to (256 MiB of RGBA): a few bytes of
/// header can claim any size, and the decode allocates it.
const MAX_PIXELS: usize = 1 << 26;

/// Whether a `width` × `height` image is non-empty and within
/// [`MAX_PIXELS`].
fn fits(width: usize, height: usize) -> bool {
    width > 0 && height > 0 && width.checked_mul(height).is_some_and(|n| n <= MAX_PIXELS)
}

/// Straight-alpha RGBA8 `pixels` as Vello image data, if they fill
/// `width` × `height`.
fn rgba_image(width: usize, height: usize, pixels: Vec<u8>) -> Option<ImageData> {
    if !fits(width, height) || pixels.len() != width * height * 4 {
        return None;
    }
    Some(ImageData {
        data: Blob::new(std::sync::Arc::new(pixels)),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: u32::try_from(width).ok()?,
        height: u32::try_from(height).ok()?,
    })
}

fn decode_jpeg(bytes: &[u8]) -> Option<ImageData> {
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(bytes, options);
    decoder.decode_headers().ok()?;
    let (width, height) = decoder.dimensions()?;
    if !fits(width, height) {
        return None;
    }
    let pixels = decoder.decode().ok()?;
    rgba_image(width, height, pixels)
}

/// The first frame, placed on the logical screen (transparent elsewhere).
fn decode_gif(bytes: &[u8]) -> Option<ImageData> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(bytes).ok()?;
    let (width, height) = (usize::from(decoder.width()), usize::from(decoder.height()));
    if !fits(width, height) {
        return None;
    }
    let frame = decoder.read_next_frame().ok()??;
    let mut canvas = vec![0; width * height * 4];
    let (left, top) = (usize::from(frame.left), usize::from(frame.top));
    let frame_width = usize::from(frame.width);
    for (row, line) in frame.buffer.chunks_exact(frame_width * 4).enumerate() {
        let y = top + row;
        if y >= height || left >= width {
            break;
        }
        let span = frame_width.min(width - left) * 4;
        let at = (y * width + left) * 4;
        canvas[at..at + span].copy_from_slice(&line[..span]);
    }
    rgba_image(width, height, canvas)
}

fn decode_webp(bytes: &[u8]) -> Option<ImageData> {
    let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let (width, height) = decoder.dimensions();
    let (width, height) = (width as usize, height as usize);
    if !fits(width, height) {
        return None;
    }
    let mut pixels = vec![0; decoder.output_buffer_size()?];
    decoder.read_image(&mut pixels).ok()?;
    let rgba = if decoder.has_alpha() {
        pixels
    } else {
        pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect()
    };
    rgba_image(width, height, rgba)
}

fn decode_png(bytes: &[u8]) -> Option<ImageData> {
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    if !fits(info.width as usize, info.height as usize) {
        return None;
    }
    let mut buffer = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buffer).ok()?;
    let pixels = &buffer[..frame.buffer_size()];
    let rgba: Vec<u8> = match frame.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        // `normalize_to_color8` expands palettes, so none reach here.
        png::ColorType::Indexed => return None,
    };
    rgba_image(frame.width as usize, frame.height as usize, rgba)
}

/// A Noonoo gradient into a Vello/peniko brush.
fn gradient(
    geometry: &GradientGeometry,
    repeating: bool,
    stops: &[noonoo_paint::GradientStop],
) -> Gradient {
    let mut gradient = match geometry {
        GradientGeometry::Linear { from, to } => {
            Gradient::new_linear((from.x as f64, from.y as f64), (to.x as f64, to.y as f64))
        }
        GradientGeometry::Radial { center, radius } => {
            // A zero radius draws nothing; keep it a hair above zero so the
            // encoder sees a usable extent.
            Gradient::new_radial(
                (center.x as f64, center.y as f64),
                radius.0.max(1.0 / 256.0),
            )
        }
    };
    gradient.extend = if repeating {
        Extend::Repeat
    } else {
        Extend::Pad
    };
    gradient.stops = stops
        .iter()
        .map(|stop| ColorStop {
            offset: stop.offset,
            color: brush(stop.color).into(),
        })
        .collect::<Vec<ColorStop>>()
        .as_slice()
        .into();
    gradient
}

/// A rectangle with per-corner radii (`top-left`, `top-right`, `bottom-right`,
/// `bottom-left`) as a Bézier path; sharp corners are straight lines and each
/// rounded corner is a cubic approximation of a quarter arc.
/// An effectively infinite shape: the whole frame.
fn frame() -> BezPath {
    let mut shape = BezPath::new();
    shape.move_to((-1.0e6, -1.0e6));
    shape.line_to((2.0e6, -1.0e6));
    shape.line_to((2.0e6, 2.0e6));
    shape.line_to((-1.0e6, 2.0e6));
    shape.close_path();
    shape
}

fn rounded_rect(rect: noonoo_geometry::Rect, radius: (f32, f32, f32, f32)) -> BezPath {
    // The circular-arc cubic constant.
    const K: f64 = 0.552_284_749_8;
    let (r_tl, r_tr, r_br, r_bl) = (
        f64::from(radius.0),
        f64::from(radius.1),
        f64::from(radius.2),
        f64::from(radius.3),
    );
    let (x, y, w, h) = (
        f64::from(rect.x),
        f64::from(rect.y),
        f64::from(rect.width),
        f64::from(rect.height),
    );
    let (right, bottom) = (x + w, y + h);
    let mut path = BezPath::new();
    path.move_to((x + r_tl, y));
    // top edge to the top-right corner
    path.line_to((right - r_tr, y));
    if r_tr > 0.0 {
        path.curve_to(
            (right - r_tr * (1.0 - K), y),
            (right, y + r_tr - r_tr * K),
            (right, y + r_tr),
        );
    }
    // right edge to the bottom-right corner
    path.line_to((right, bottom - r_br));
    if r_br > 0.0 {
        path.curve_to(
            (right, bottom - r_br * (1.0 - K)),
            (right - r_br + r_br * K, bottom),
            (right - r_br, bottom),
        );
    }
    // bottom edge to the bottom-left corner
    path.line_to((x + r_bl, bottom));
    if r_bl > 0.0 {
        path.curve_to(
            (x + r_bl - r_bl * K, bottom),
            (x, bottom - r_bl + r_bl * K),
            (x, bottom - r_bl),
        );
    }
    // left edge to the top-left corner
    path.line_to((x, y + r_tl));
    if r_tl > 0.0 {
        path.curve_to(
            (x, y + r_tl - r_tl * K),
            (x + r_tl - r_tl * K, y),
            (x + r_tl, y),
        );
    }
    path.close_path();
    path
}

/// A rectangle with per-corner radii (`top-left`, `top-right`, `bottom-right`,
/// `bottom-left`) as a Bézier path; sharp corners are straight lines and each
/// rounded corner is a cubic approximation of a quarter arc.
/// Noonoo's 8-bit RGBA into a Vello/peniko colour.
/// One outlined box per visible character of a run drawn without a font, in
/// the run's colour, sitting on its baseline: the "missing glyph" box a
/// browser draws, so text with no font is visible rather than gone.
fn placeholder_boxes(
    scene: &mut Scene,
    transform: Affine,
    text: &str,
    (x, baseline): (f32, f32),
    font_size: f32,
    color: noonoo_paint::Color,
) {
    let em = f64::from(font_size.max(0.0));
    if em == 0.0 {
        return;
    }
    let stroke = kurbo::Stroke::new((em * 0.06).max(1.0));
    let (advance, width, height) = (em * 0.6, em * 0.5, em * 0.7);
    let baseline = f64::from(baseline);
    let mut pen = f64::from(x);
    for character in text.chars() {
        if !character.is_whitespace() {
            let left = pen + (advance - width) / 2.0;
            let tofu = KRect::new(left, baseline - height, left + width, baseline);
            scene.stroke(&stroke, transform, brush(color), None, &tofu);
        }
        pen += advance;
    }
}

fn brush(color: noonoo_paint::Color) -> Color {
    Color::from_rgba8(color.r, color.g, color.b, color.a)
}

/// Noonoo's `(x, y, width, height)` into kurbo's `(x0, y0, x1, y1)`.
fn kurbo_rect(rect: noonoo_geometry::Rect) -> KRect {
    KRect::new(
        rect.x as f64,
        rect.y as f64,
        rect.right() as f64,
        rect.bottom() as f64,
    )
}

/// A closed polygon path from the points.
fn bez_path(points: &[Point]) -> BezPath {
    let mut path = BezPath::new();
    path.move_to((points[0].x as f64, points[0].y as f64));
    for point in &points[1..] {
        path.line_to((point.x as f64, point.y as f64));
    }
    path.close_path();
    path
}

/// The version of this crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;
    use noonoo_geometry::Rect as NRect;
    use noonoo_paint::Color;

    fn encodings(commands: Vec<Command>) -> (usize, usize) {
        let scene = scene(&CommandList(commands));
        (
            scene.encoding().path_tags.len(),
            scene.encoding().draw_tags.len(),
        )
    }

    fn red_square() -> Command {
        Command::FillRect {
            rect: NRect::new(0.0, 0.0, 10.0, 10.0),
            color: Color::rgb(255, 0, 0),
            radius: (0.0, 0.0, 0.0, 0.0),
        }
    }

    /// Each push composes with the enclosing transform; a pop restores it.
    #[test]
    fn transforms_stack_over_the_fills_inside_them() {
        let scene = scene(&CommandList(vec![
            Command::PushTransform {
                matrix: [1.0, 0.0, 0.0, 1.0, 5.0, 6.0],
            },
            Command::PushTransform {
                matrix: [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
            },
            red_square(),
            Command::PopTransform,
            red_square(),
            Command::PopTransform,
            red_square(),
        ]));
        let transforms: Vec<([f32; 4], [f32; 2])> = scene
            .encoding()
            .transforms
            .iter()
            .map(|t| (t.matrix, t.translation))
            .collect();
        assert!(
            transforms.contains(&([2.0, 0.0, 0.0, 2.0], [5.0, 6.0])),
            "{transforms:?}"
        );
        assert!(
            transforms.contains(&([1.0, 0.0, 0.0, 1.0], [5.0, 6.0])),
            "{transforms:?}"
        );
        assert_eq!(
            transforms.last(),
            Some(&([1.0, 0.0, 0.0, 1.0], [0.0, 0.0])),
            "{transforms:?}"
        );
    }

    fn shadow(blur: f32) -> Command {
        Command::BoxShadow {
            rect: NRect::new(-2.0, -2.0, 14.0, 14.0),
            radius: (3.0, 3.0, 3.0, 3.0),
            blur,
            color: Color::rgb(0, 0, 0),
            element: NRect::new(0.0, 0.0, 10.0, 10.0),
            element_radius: (1.0, 1.0, 1.0, 1.0),
        }
    }

    /// `DrawTag::BLUR_RECT` and `BEGIN_CLIP` in vello_encoding 0.7.
    const BLUR_RECT: u32 = 0x2d4;
    const BEGIN_CLIP: u32 = 0x49;

    /// A shadow is clipped to outside the element, blurred when it has a
    /// blur radius and a plain fill when it doesn't.
    #[test]
    fn box_shadows_clip_out_the_element_and_blur() {
        let tags = |command| {
            scene(&CommandList(vec![command]))
                .encoding()
                .draw_tags
                .iter()
                .map(|tag| tag.0)
                .collect::<Vec<u32>>()
        };
        let blurred = tags(shadow(4.0));
        assert!(blurred.contains(&BEGIN_CLIP), "{blurred:x?}");
        assert!(blurred.contains(&BLUR_RECT), "{blurred:x?}");
        let hard = tags(shadow(0.0));
        assert!(hard.contains(&BEGIN_CLIP), "{hard:x?}");
        assert!(!hard.contains(&BLUR_RECT), "{hard:x?}");
    }

    /// A ring is one even-odd fill of its outer and inner rounded paths.
    #[test]
    fn a_ring_fills_once_with_both_outlines() {
        let ring = scene(&CommandList(vec![Command::FillRing {
            outer: NRect::new(0.0, 0.0, 20.0, 20.0),
            outer_radius: (5.0, 5.0, 5.0, 5.0),
            inner: NRect::new(2.0, 2.0, 16.0, 16.0),
            inner_radius: (3.0, 3.0, 3.0, 3.0),
            color: Color::rgb(0, 0, 255),
        }]));
        let single = scene(&CommandList(vec![Command::FillRect {
            rect: NRect::new(0.0, 0.0, 20.0, 20.0),
            color: Color::rgb(0, 0, 255),
            radius: (5.0, 5.0, 5.0, 5.0),
        }]));
        assert_eq!(
            ring.encoding().draw_tags.len(),
            single.encoding().draw_tags.len()
        );
        // Two closed outlines instead of one.
        assert!(ring.encoding().path_tags.len() > single.encoding().path_tags.len());
        assert_eq!(ring.encoding().styles.len(), 1);
    }

    #[test]
    fn a_fill_rect_becomes_a_path_and_a_draw() {
        let (paths, draws) = encodings(vec![Command::FillRect {
            rect: NRect::new(0.0, 0.0, 10.0, 10.0),
            color: Color::rgb(255, 0, 0),
            radius: (0.0, 0.0, 0.0, 0.0),
        }]);
        assert!(paths > 0);
        assert!(draws >= 1);
    }

    #[test]
    fn a_polygon_becomes_a_closed_path() {
        let (paths, draws) = encodings(vec![Command::FillPolygon {
            points: vec![
                Point::new(0.0, 0.0),
                Point::new(5.0, 0.0),
                Point::new(0.0, 5.0),
            ],
            color: Color::rgb(0, 255, 0),
        }]);
        assert!(paths > 0);
        assert!(draws >= 1);
    }

    /// A glyph's contours are one fill (one draw), so the non-zero rule can
    /// cut its holes; two draws would paint the hole solid.
    #[test]
    fn contours_fill_as_one_shape() {
        let square = |x: f32, y: f32, size: f32| {
            vec![
                Point::new(x, y),
                Point::new(x + size, y),
                Point::new(x + size, y + size),
                Point::new(x, y + size),
            ]
        };
        let (paths, draws) = encodings(vec![Command::FillContours {
            contours: vec![square(0.0, 0.0, 10.0), square(3.0, 3.0, 4.0)],
            color: Color::BLACK,
        }]);
        assert!(paths > 0);
        assert_eq!(draws, 1);
    }

    /// A `width`×`height` RGBA PNG, every pixel `rgba`.
    fn png_bytes(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&rgba.repeat((width * height) as usize))
            .unwrap();
        writer.finish().unwrap();
        out
    }

    fn image_patches(scene: &Scene) -> Vec<vello::peniko::ImageData> {
        scene
            .encoding()
            .resources
            .patches
            .iter()
            .filter_map(|patch| match patch {
                vello_encoding::Patch::Image { image, .. } => Some(image.clone()),
                _ => None,
            })
            .collect()
    }

    fn draw_image(rect: NRect, data: Vec<u8>) -> Command {
        Command::DrawImage {
            rect,
            image: noonoo_paint::EncodedImage::png(data),
        }
    }

    #[test]
    fn a_png_image_draws_its_decoded_pixels() {
        let scene = scene(&CommandList(vec![draw_image(
            NRect::new(0.0, 0.0, 4.0, 2.0),
            png_bytes(2, 1, [10, 20, 30, 128]),
        )]));
        let images = image_patches(&scene);
        assert_eq!(images.len(), 1);
        let image = &images[0];
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.format, vello::peniko::ImageFormat::Rgba8);
        assert_eq!(image.alpha_type, vello::peniko::ImageAlphaType::Alpha);
        assert_eq!(image.data.data(), &[10, 20, 30, 128, 10, 20, 30, 128]);
    }

    /// The one image a `DrawImage` of `bytes` hands Vello, decoded.
    fn decoded(bytes: &[u8]) -> vello::peniko::ImageData {
        let image = noonoo_paint::EncodedImage::sniff(bytes.to_vec()).expect("a known format");
        let scene = scene(&CommandList(vec![Command::DrawImage {
            rect: NRect::new(0.0, 0.0, 8.0, 8.0),
            image,
        }]));
        let mut images = image_patches(&scene);
        assert_eq!(images.len(), 1, "one image drawn");
        images.remove(0)
    }

    /// Whether each channel of `pixel` is within `slack` of `want`.
    fn near(pixel: &[u8], want: [u8; 4], slack: u8) -> bool {
        pixel
            .iter()
            .zip(want)
            .all(|(&got, want)| got.abs_diff(want) <= slack)
    }

    #[test]
    fn a_jpeg_image_decodes_to_opaque_pixels() {
        let image = decoded(include_bytes!("../tests/fixtures/solid.jpg"));
        assert_eq!((image.width, image.height), (8, 8));
        let pixels = image.data.data();
        assert_eq!(pixels.len(), 8 * 8 * 4);
        assert!(
            pixels.chunks(4).all(|p| near(p, [200, 40, 40, 255], 6)),
            "{:?}",
            &pixels[..4]
        );
    }

    #[test]
    fn a_gif_image_decodes_its_first_frame_with_transparency() {
        let image = decoded(include_bytes!("../tests/fixtures/tiny.gif"));
        assert_eq!((image.width, image.height), (3, 2));
        let pixels = image.data.data();
        assert_eq!(pixels[3], 0, "the transparent index");
        assert_eq!(&pixels[4..8], &[0, 200, 0, 255]);
    }

    #[test]
    fn a_gif_frame_is_placed_on_its_logical_screen() {
        // A 2 × 2 screen holding one 1 × 1 green frame at (1, 1).
        let mut gif = b"GIF89a\x02\x00\x02\x00\x80\x00\x00".to_vec();
        gif.extend([0, 0, 0, 0, 200, 0]);
        gif.extend(b",\x01\x00\x01\x00\x01\x00\x01\x00\x00");
        gif.extend([2, 2, 0x4c, 0x01, 0, b';']);
        let image = decoded(&gif);
        assert_eq!((image.width, image.height), (2, 2));
        let pixels = image.data.data();
        assert_eq!(&pixels[12..16], &[0, 200, 0, 255]);
        assert!(pixels[..12].chunks(4).all(|p| p[3] == 0));
    }

    #[test]
    fn a_gif_frame_past_the_screen_edge_is_clipped() {
        // A 2 × 1 screen; a 2 × 1 green frame at x = 1 hangs off the right.
        let mut gif = b"GIF89a\x02\x00\x01\x00\x80\x00\x00".to_vec();
        gif.extend([0, 0, 0, 0, 200, 0]);
        gif.extend(b",\x01\x00\x00\x00\x02\x00\x01\x00\x00");
        gif.extend([2, 2, 0x4c, 0x0a, 0, b';']);
        let image = decoded(&gif);
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.data.data(), &[0, 0, 0, 0, 0, 200, 0, 255]);
    }

    #[test]
    fn webp_images_decode_lossless_with_alpha_and_lossy_without() {
        let image = decoded(include_bytes!("../tests/fixtures/alpha.webp"));
        assert_eq!((image.width, image.height), (3, 2));
        let pixels = image.data.data();
        assert_eq!(pixels[3], 0);
        assert_eq!(&pixels[4..8], &[10, 20, 30, 255]);
        let image = decoded(include_bytes!("../tests/fixtures/lossy.webp"));
        assert_eq!((image.width, image.height), (16, 8));
        let pixels = image.data.data();
        assert_eq!(pixels.len(), 16 * 8 * 4);
        assert!(
            pixels.chunks(4).all(|p| near(p, [40, 40, 200, 255], 8)),
            "{:?}",
            &pixels[..4]
        );
    }

    /// A header can claim any size; decoding stops at 64 Mpx.
    #[test]
    fn images_too_large_to_decode_are_refused_up_front() {
        assert!(fits(8192, 8192));
        assert!(!fits(8193, 8192));
        assert!(!fits(0, 4) && !fits(4, 0));
        assert!(!fits(usize::MAX, 2));
        // A GIF claiming 65 535 × 65 535 draws nothing (and allocates nothing).
        let mut bomb = b"GIF89a\xff\xff\xff\xff\x00\x00\x00".to_vec();
        bomb.extend(b",\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00;");
        let image = noonoo_paint::EncodedImage::sniff(bomb).unwrap();
        let scene = scene(&CommandList(vec![Command::DrawImage {
            rect: NRect::new(0.0, 0.0, 8.0, 8.0),
            image,
        }]));
        assert!(image_patches(&scene).is_empty());
    }

    /// Repainting the same glyph must not hand Vello a new image each frame:
    /// its atlas keys images by blob, so equal bytes share one decode.
    #[test]
    fn equal_png_bytes_share_one_decoded_image() {
        let bytes = png_bytes(3, 3, [1, 2, 3, 255]);
        let rect = NRect::new(0.0, 0.0, 3.0, 3.0);
        let first = image_patches(&scene(&CommandList(vec![draw_image(rect, bytes.clone())])));
        let second = image_patches(&scene(&CommandList(vec![draw_image(rect, bytes)])));
        assert_eq!(first[0].data.id(), second[0].data.id());
    }

    #[test]
    fn an_undecodable_image_draws_nothing() {
        let (paths, draws) = encodings(vec![draw_image(
            NRect::new(0.0, 0.0, 4.0, 4.0),
            b"not a png".to_vec(),
        )]);
        assert_eq!((paths, draws), (0, 0));
    }

    fn text_run(text: &str) -> Command {
        Command::DrawText {
            text: text.into(),
            x: 0.0,
            y: 16.0,
            font_size: 16.0,
            color: Color::BLACK,
        }
    }

    /// A text run has no outlines for Vello. Rather than vanish, each visible
    /// character draws a placeholder box, so a missing font shows on screen.
    #[test]
    fn a_text_run_draws_a_placeholder_box_per_character() {
        let (_, draws) = encodings(vec![text_run("hi")]);
        assert_eq!(draws, 2);
        // Spaces take room but draw nothing.
        let (_, draws) = encodings(vec![text_run("a b")]);
        assert_eq!(draws, 2);
        assert_eq!(encodings(vec![text_run("  ")]), (0, 0));
    }

    #[test]
    fn a_clip_wraps_the_content_as_a_layer() {
        let (_, with_clip) = encodings(vec![
            Command::PushLayer {
                clip: Some(NRect::new(0.0, 0.0, 10.0, 10.0)),
                alpha: 1.0,
            },
            Command::FillRect {
                rect: NRect::new(0.0, 0.0, 10.0, 10.0),
                color: Color::rgb(0, 0, 255),
                radius: (0.0, 0.0, 0.0, 0.0),
            },
            Command::PopLayer,
        ]);
        let (_, without_clip) = encodings(vec![Command::FillRect {
            rect: NRect::new(0.0, 0.0, 10.0, 10.0),
            color: Color::rgb(0, 0, 255),
            radius: (0.0, 0.0, 0.0, 0.0),
        }]);
        // The layer's begin/end are extra draw entries.
        assert!(with_clip > without_clip);
    }

    #[test]
    fn a_rounded_fill_and_an_alpha_layer_add_paths() {
        let (paths, draws) = encodings(vec![
            Command::PushLayer {
                clip: None,
                alpha: 0.5,
            },
            Command::FillRect {
                rect: NRect::new(0.0, 0.0, 10.0, 10.0),
                color: Color::rgb(255, 0, 0),
                radius: (2.0, 2.0, 2.0, 2.0),
            },
            Command::PopLayer,
        ]);
        assert!(paths > 0);
        assert!(draws >= 3);
    }
}
