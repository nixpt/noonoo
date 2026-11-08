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
//! | `PushLayer` / `PopLayer` | `Scene::push_layer` (alpha + clip) / `pop_layer` |
//! | `DrawText` | *not drawn* — Noonoo renders text as glyph outlines (`noonoo-ttf`'s `OutlineGlyphs`), so a bare text run has no paths for Vello |
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

use noonoo_geometry::Point;
use noonoo_paint::{Command, CommandList, GradientGeometry};
use vello::Scene;
use vello::kurbo::{self, Affine, BezPath, Rect as KRect};
use vello::peniko::{BlendMode, Color, ColorStop, Extend, Fill, Gradient};

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
            // A plain text run carries no outlines; use `OutlineGlyphs` so Vello
            // receives filled glyph contours instead.
            Command::DrawText { .. } => {}
            Command::PushLayer { clip, alpha } => {
                // No clip: an effectively infinite shape (Vello wants a shape
                // per layer), so the group covers the whole frame.
                let shape = match clip {
                    Some(rect) => kurbo::Shape::into_path(kurbo_rect(*rect), 0.1),
                    None => {
                        let mut shape = BezPath::new();
                        shape.move_to((-1.0e6, -1.0e6));
                        shape.line_to((2.0e6, -1.0e6));
                        shape.line_to((2.0e6, 2.0e6));
                        shape.line_to((-1.0e6, 2.0e6));
                        shape.close_path();
                        shape
                    }
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

    #[test]
    fn a_text_run_is_not_drawn() {
        let (paths, draws) = encodings(vec![Command::DrawText {
            text: "hi".into(),
            x: 0.0,
            y: 0.0,
            font_size: 16.0,
            color: Color::BLACK,
        }]);
        assert_eq!((paths, draws), (0, 0));
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
