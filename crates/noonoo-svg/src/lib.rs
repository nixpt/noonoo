//! # noonoo-svg
//!
//! An SVG **subset** — parse shapes and render them to [`noonoo_paint`]
//! commands. This is the Phase E remainder (`docs/DEPENDENCY-EXIT.md`).
//!
//! SVG is XML, so the parser reuses `noonoo-xml` and `noonoo-dom`, then walks
//! the tree into a small shape model and flattens it to polygons and stroke
//! quads for the command stream. A file that is not well-formed XML parses to an
//! empty document — strict, and honest about it.
//!
//! Supported: `<svg>` / `<g>`, `<rect>`, `<circle>`, `<ellipse>`, `<line>`,
//! `<polygon>`, `<polyline>`, `<path>` (M L H V Z, with C/Q/S/T/A approximated
//! by a line to the endpoint), `fill` / `stroke` / `stroke-width` / `opacity`,
//! `transform` (`translate` + `scale`), and `viewBox` fit. Not yet: gradients,
//! clips, masks, text, and multi-subpath fills.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod parse;
mod render;

use noonoo_css::Color;
use noonoo_geometry::Size;
use noonoo_paint::CommandList;

/// An affine transform limited to independent axis scaling and translation.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Transform {
    /// Horizontal scale.
    pub sx: f32,
    /// Vertical scale.
    pub sy: f32,
    /// Horizontal translation.
    pub tx: f32,
    /// Vertical translation.
    pub ty: f32,
}

impl Transform {
    /// The identity transform.
    pub const IDENTITY: Transform = Transform {
        sx: 1.0,
        sy: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    /// Apply to a point.
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.sx + self.tx, y * self.sy + self.ty)
    }

    /// Compose `parent ∘ child`.
    pub fn compose(parent: Transform, child: Transform) -> Transform {
        Transform {
            sx: child.sx * parent.sx,
            sy: child.sy * parent.sy,
            tx: child.tx * parent.sx + parent.tx,
            ty: child.ty * parent.sy + parent.ty,
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// A shape's geometry, in user units (before transform).
#[derive(Clone, PartialEq, Debug)]
pub enum Geometry {
    /// A rectangle.
    Rect {
        /// Left.
        x: f32,
        /// Top.
        y: f32,
        /// Width.
        width: f32,
        /// Height.
        height: f32,
    },
    /// A circle.
    Circle {
        /// Centre x.
        cx: f32,
        /// Centre y.
        cy: f32,
        /// Radius.
        r: f32,
    },
    /// An ellipse.
    Ellipse {
        /// Centre x.
        cx: f32,
        /// Centre y.
        cy: f32,
        /// Radius x.
        rx: f32,
        /// Radius y.
        ry: f32,
    },
    /// A line segment.
    Line {
        /// Start x.
        x1: f32,
        /// Start y.
        y1: f32,
        /// End x.
        x2: f32,
        /// End y.
        y2: f32,
    },
    /// A closed polygon.
    Polygon(Vec<(f32, f32)>),
    /// An open polyline.
    Polyline(Vec<(f32, f32)>),
    /// A flattened path.
    Path {
        /// The vertices.
        points: Vec<(f32, f32)>,
        /// Whether the path was closed with `Z`.
        closed: bool,
    },
}

/// A drawable shape.
#[derive(Clone, PartialEq, Debug)]
pub struct Shape {
    /// The geometry.
    pub geometry: Geometry,
    /// The fill colour (`None` when `fill="none"`).
    pub fill: Option<Color>,
    /// The stroke colour and width (`None` when stroking is off).
    pub stroke: Option<(Color, f32)>,
    /// The accumulated transform.
    pub transform: Transform,
}

/// A parsed SVG document.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Svg {
    /// Intrinsic width.
    pub width: f32,
    /// Intrinsic height.
    pub height: f32,
    /// The `viewBox` (min-x, min-y, width, height), if given.
    pub view_box: Option<[f32; 4]>,
    /// The flattened shapes.
    pub shapes: Vec<Shape>,
}

impl Svg {
    /// Parse SVG text.
    pub fn parse(source: &str) -> Svg {
        parse::parse(source)
    }

    /// Render to paint commands, fit to `viewport`.
    pub fn to_commands(&self, viewport: Size) -> CommandList {
        render::to_commands(self, viewport)
    }
}

/// Parse SVG text and render it to paint commands.
pub fn render(source: &str, viewport: Size) -> CommandList {
    Svg::parse(source).to_commands(viewport)
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;
    use noonoo_paint::Command;

    fn commands(source: &str) -> CommandList {
        render(source, Size::new(100.0, 100.0))
    }

    #[test]
    fn renders_a_filled_rect() {
        let list = commands(
            r#"<svg width="10" height="10"><rect x="0" y="0" width="10" height="10" fill="red"/></svg>"#,
        );
        assert!(matches!(
            &list.commands()[0],
            Command::FillPolygon { points, color } if points.len() == 4 && *color == Color::rgb(255, 0, 0)
        ));
    }

    #[test]
    fn tessellates_a_circle() {
        let list = commands(r#"<svg><circle cx="5" cy="5" r="5"/></svg>"#);
        let Command::FillPolygon { points, .. } = &list.commands()[0] else {
            panic!("expected a polygon");
        };
        assert_eq!(points.len(), 48);
    }

    #[test]
    fn view_box_scales_to_the_viewport() {
        let list = render(
            r#"<svg viewBox="0 0 10 10"><rect width="10" height="10"/></svg>"#,
            Size::new(100.0, 100.0),
        );
        let Command::FillPolygon { points, .. } = &list.commands()[0] else {
            panic!();
        };
        // 10 user units across a 100px viewport -> the far corner is at 100.
        assert!((points[2].x - 100.0).abs() < 0.001);
        assert!((points[2].y - 100.0).abs() < 0.001);
    }

    #[test]
    fn strokes_a_line() {
        let list = commands(
            r#"<svg><line x1="0" y1="0" x2="10" y2="0" stroke="black" stroke-width="2"/></svg>"#,
        );
        // A stroked line becomes a quad; fill="none" by default for <line>.
        assert!(
            matches!(&list.commands()[0], Command::FillPolygon { points, .. } if points.len() == 4)
        );
    }

    #[test]
    fn fills_are_none_by_default_on_open_shapes() {
        let list =
            commands(r#"<svg><polyline points="0,0 10,0 10,10" fill="none" stroke="red"/></svg>"#);
        // Two stroked edges → two quads, no fill.
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn tolerates_an_xml_prolog_and_comments() {
        let list = commands(
            r#"<?xml version="1.0"?><!-- icon --><svg width="10" height="10"><rect width="10" height="10" fill="red"/></svg>"#,
        );
        assert!(matches!(&list.commands()[0], Command::FillPolygon { .. }));
    }

    #[test]
    fn a_malformed_document_renders_nothing() {
        // `<rect>` is never closed, so the XML parser refuses the document and
        // there is nothing to draw — rather than a guessed tree.
        let list = commands(r#"<svg><rect width="10" height="10"></svg>"#);
        assert_eq!(list.len(), 0);
    }
}
