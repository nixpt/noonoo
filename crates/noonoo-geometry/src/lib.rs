//! # noonoo-geometry
//!
//! The geometry primitives shared by layout, paint, and the shell: a [`Point`],
//! a [`Size`], and a [`Rect`]. They are borrowed by neither engine and used by
//! several, so they live in one small crate rather than being re-declared per
//! layer.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// A position in page space, in logical pixels.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Point {
    /// Horizontal position.
    pub x: f32,
    /// Vertical position.
    pub y: f32,
}

impl Point {
    /// A point at `(x, y)`.
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// The origin.
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    /// This point moved by `(dx, dy)`.
    pub const fn translate(self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
        }
    }
}

/// A width and height, in logical pixels.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Size {
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl Size {
    /// A size of `(width, height)`.
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    /// Zero size.
    pub const ZERO: Size = Size {
        width: 0.0,
        height: 0.0,
    };

    /// Whether either dimension is non-positive.
    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

/// An axis-aligned rectangle, with its origin at the top-left.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl Rect {
    /// A rectangle at `(x, y)` with `(width, height)`.
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// A rectangle at the origin with `size`.
    pub const fn from_size(size: Size) -> Self {
        Self::new(0.0, 0.0, size.width, size.height)
    }

    /// The right edge (`x + width`).
    pub const fn right(&self) -> f32 {
        self.x + self.width
    }

    /// The bottom edge (`y + height`).
    pub const fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// The top-left corner.
    pub const fn origin(&self) -> Point {
        Point::new(self.x, self.y)
    }

    /// The size.
    pub const fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    /// Shrink by the given edges.
    pub fn inset(&self, top: f32, right: f32, bottom: f32, left: f32) -> Rect {
        Rect::new(
            self.x + left,
            self.y + top,
            (self.width - left - right).max(0.0),
            (self.height - top - bottom).max(0.0),
        )
    }

    /// Move by `(dx, dy)`.
    pub const fn translate(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.width, self.height)
    }

    /// Whether `point` lies inside the rectangle (edges inclusive).
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.right()
            && point.y >= self.y
            && point.y <= self.bottom()
    }
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_and_inset() {
        let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!(rect.right(), 110.0);
        assert_eq!(rect.bottom(), 70.0);
        let inner = rect.inset(5.0, 5.0, 5.0, 5.0);
        assert_eq!(inner, Rect::new(15.0, 25.0, 90.0, 40.0));
    }

    #[test]
    fn contains_respects_edges() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(rect.contains(Point::new(5.0, 5.0)));
        assert!(rect.contains(Point::new(10.0, 10.0)));
        assert!(!rect.contains(Point::new(11.0, 5.0)));
    }
}
