//! Rendering the shape model to paint commands.

use noonoo_geometry::{Point, Size};
use noonoo_paint::{Command, CommandList};

use crate::{Geometry, Svg};

pub(crate) fn to_commands(svg: &Svg, viewport: Size) -> CommandList {
    let (scale_x, scale_y, offset_x, offset_y) = fit(svg, viewport);
    let mut out = CommandList::default();

    for shape in &svg.shapes {
        let (unit, closed) = points(&shape.geometry);
        if unit.len() < 2 {
            continue;
        }

        let mapped: Vec<Point> = unit
            .iter()
            .map(|(x, y)| {
                let (x, y) = shape.transform.apply(*x, *y);
                Point::new(x * scale_x + offset_x, y * scale_y + offset_y)
            })
            .collect();

        if let Some(fill) = shape.fill
            && mapped.len() >= 3
        {
            out.0.push(Command::FillPolygon {
                points: mapped.clone(),
                color: fill,
            });
        }

        if let Some((stroke, width)) = shape.stroke {
            let width = (width * scale_x.max(scale_y)).max(0.5);
            let count = mapped.len();
            let edges = if closed { count } else { count - 1 };
            for i in 0..edges {
                if let Some(quad) = segment_quad(mapped[i], mapped[(i + 1) % count], width) {
                    out.0.push(Command::FillPolygon {
                        points: quad.to_vec(),
                        color: stroke,
                    });
                }
            }
        }
    }

    out
}

/// Fit the SVG (via `viewBox`, else 1:1) into the viewport.
fn fit(svg: &Svg, viewport: Size) -> (f32, f32, f32, f32) {
    match svg.view_box {
        Some([min_x, min_y, width, height]) if width > 0.0 && height > 0.0 => {
            let scale_x = viewport.width / width;
            let scale_y = viewport.height / height;
            (scale_x, scale_y, -min_x * scale_x, -min_y * scale_y)
        }
        _ => (1.0, 1.0, 0.0, 0.0),
    }
}

fn points(geometry: &Geometry) -> (Vec<(f32, f32)>, bool) {
    match geometry {
        Geometry::Rect {
            x,
            y,
            width,
            height,
        } => (
            vec![
                (*x, *y),
                (*x + width, *y),
                (*x + width, *y + height),
                (*x, *y + height),
            ],
            true,
        ),
        Geometry::Circle { cx, cy, r } => (ellipse_points(*cx, *cy, *r, *r), true),
        Geometry::Ellipse { cx, cy, rx, ry } => (ellipse_points(*cx, *cy, *rx, *ry), true),
        Geometry::Line { x1, y1, x2, y2 } => (vec![(*x1, *y1), (*x2, *y2)], false),
        Geometry::Polygon(vertices) => (vertices.clone(), true),
        Geometry::Polyline(vertices) => (vertices.clone(), false),
        Geometry::Path { points, closed } => (points.clone(), *closed),
    }
}

fn ellipse_points(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<(f32, f32)> {
    const SEGMENTS: usize = 48;
    (0..SEGMENTS)
        .map(|k| {
            let angle = std::f32::consts::TAU * k as f32 / SEGMENTS as f32;
            (cx + rx * angle.cos(), cy + ry * angle.sin())
        })
        .collect()
}

/// A stroke quad around the segment `a → b`, `width` wide.
fn segment_quad(a: Point, b: Point, width: f32) -> Option<[Point; 4]> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = (dx * dx + dy * dy).sqrt();
    if length == 0.0 {
        return None;
    }
    let half = width / 2.0;
    let (nx, ny) = (-dy / length * half, dx / length * half);
    Some([
        Point::new(a.x + nx, a.y + ny),
        Point::new(b.x + nx, b.y + ny),
        Point::new(b.x - nx, b.y - ny),
        Point::new(a.x - nx, a.y - ny),
    ])
}
