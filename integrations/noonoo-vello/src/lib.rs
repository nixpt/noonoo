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
//! | `FillRect` | `Scene::fill` a `kurbo::Rect` |
//! | `FillGradient` | `Scene::fill` a `kurbo::Rect` with a `peniko::Gradient` (elliptical radial gradients use a brush transform) |
//! | `FillPolygon` | `Scene::fill` a closed `kurbo::BezPath` |
//! | `PushClip` / `PopClip` | `Scene::push_layer` / `pop_layer` |
//! | `DrawText` | *not drawn* — Noonoo renders text as glyph outlines (`noonoo-ttf`'s `OutlineGlyphs`), so a bare text run has no paths for Vello |
//!
//! ```
//! let commands = noonoo_paint::CommandList(vec![noonoo_paint::Command::FillRect {
//!     rect: noonoo_geometry::Rect::new(0.0, 0.0, 10.0, 10.0),
//!     color: noonoo_paint::Color::rgb(255, 0, 0),
//! }]);
//! let scene = noonoo_vello::scene(&commands);
//! assert!(!scene.encoding().path_tags.is_empty());
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use noonoo_geometry::Point;
use noonoo_paint::{Command, CommandList, GradientGeometry};
use vello::Scene;
use vello::kurbo::{Affine, BezPath, Rect};
use vello::peniko::{BlendMode, Color, ColorStop, Extend, Fill, Gradient};

/// Build a fresh Vello scene from a Noonoo command stream.
pub fn scene(commands: &CommandList) -> Scene {
    let mut scene = Scene::new();
    draw(&mut scene, commands);
    scene
}

/// Draw a Noonoo command stream into an existing Vello scene.
pub fn draw(scene: &mut Scene, commands: &CommandList) {
    for command in commands.commands() {
        match command {
            Command::FillRect { rect, color } => {
                let shape = kurbo_rect(*rect);
                scene.fill(Fill::NonZero, Affine::IDENTITY, brush(*color), None, &shape);
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
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    &brush,
                    brush_transform,
                    &shape,
                );
            }
            Command::FillPolygon { points, color } => {
                if points.len() >= 3 {
                    let path = bez_path(points);
                    scene.fill(Fill::NonZero, Affine::IDENTITY, brush(*color), None, &path);
                }
            }
            // A plain text run carries no outlines; use `OutlineGlyphs` so Vello
            // receives filled glyph polygons instead.
            Command::DrawText { .. } => {}
            Command::PushClip { rect } => {
                let shape = kurbo_rect(*rect);
                scene.push_layer(
                    Fill::NonZero,
                    BlendMode::default(),
                    1.0,
                    Affine::IDENTITY,
                    &shape,
                );
            }
            Command::PopClip => scene.pop_layer(),
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

/// Noonoo's 8-bit RGBA into a Vello/peniko colour.
fn brush(color: noonoo_paint::Color) -> Color {
    Color::from_rgba8(color.r, color.g, color.b, color.a)
}

/// Noonoo's `(x, y, width, height)` into kurbo's `(x0, y0, x1, y1)`.
fn kurbo_rect(rect: noonoo_geometry::Rect) -> Rect {
    Rect::new(
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

    #[test]
    fn a_fill_rect_becomes_a_path_and_a_draw() {
        let (paths, draws) = encodings(vec![Command::FillRect {
            rect: NRect::new(0.0, 0.0, 10.0, 10.0),
            color: Color::rgb(255, 0, 0),
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
            Command::PushClip {
                rect: NRect::new(0.0, 0.0, 10.0, 10.0),
            },
            Command::FillRect {
                rect: NRect::new(0.0, 0.0, 10.0, 10.0),
                color: Color::rgb(0, 0, 255),
            },
            Command::PopClip,
        ]);
        let (_, without_clip) = encodings(vec![Command::FillRect {
            rect: NRect::new(0.0, 0.0, 10.0, 10.0),
            color: Color::rgb(0, 0, 255),
        }]);
        // The layer's begin/end clip are extra draw entries.
        assert!(with_clip > without_clip);
    }
}
