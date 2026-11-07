//! `glyf` / `loca` parsing: glyph outlines, simple and composite.
//!
//! A glyph is a set of contours of quadratic points in font units (y up). A
//! *simple* glyph stores its points directly; a *composite* glyph is a list of
//! components, each a reference to another glyph with an affine transform. Both
//! reduce to [`Outline`], whose [`Outline::flatten`] turns the quadratic curves
//! into closed polygons ready for the paint stream.
//!
//! This covers TrueType outlines. CFF/OpenType (`OTTO`) fonts have no `glyf`
//! table, so [`Font::outline`] simply returns `None` there — their metrics still
//! parse.

use noonoo_parse::ParseResult;

use crate::Font;
use crate::reader::Reader;

/// A point on a glyph contour, in font units.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Point {
    /// Horizontal position.
    pub x: f32,
    /// Vertical position.
    pub y: f32,
    /// Whether this point is on the curve (quadratic control points are not).
    pub on_curve: bool,
}

/// One closed contour: its points, in order.
#[derive(Clone, PartialEq, Debug)]
pub struct Contour {
    /// The contour's points.
    pub points: Vec<Point>,
}

/// A glyph's outline: its contours, in font units.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Outline {
    /// The contours.
    pub contours: Vec<Contour>,
}

impl Outline {
    /// Whether the outline has no points (an empty glyph, e.g. a space).
    pub fn is_empty(&self) -> bool {
        self.contours
            .iter()
            .all(|contour| contour.points.is_empty())
    }

    /// Flatten the quadratic outlines to polygons (one per contour), sampling
    /// each quadratic segment `steps` times. Closed implicitly: the last vertex
    /// joins the first.
    pub fn flatten(&self, steps: usize) -> Vec<Vec<(f32, f32)>> {
        self.contours
            .iter()
            .filter(|contour| !contour.points.is_empty())
            .map(|contour| flatten_contour(&contour.points, steps.max(1)))
            .collect()
    }
}

impl Font {
    /// A glyph's outline, in font units (y up), or `None` if the font has no
    /// `glyf` outlines (e.g. a CFF font) or the glyph index is out of range.
    pub fn outline(&self, glyph: u16) -> Option<Outline> {
        Some(Outline {
            contours: self.glyph_contours(glyph, 0)?,
        })
    }

    /// Lay `text` out along the baseline and return one polygon per visible
    /// contour, in pixels with the origin at the run's baseline start (y up).
    /// A caller rendering to screen flips y.
    pub fn text_polygons(&self, text: &str, font_size: f32) -> Vec<Vec<(f32, f32)>> {
        let scale = font_size / self.units_per_em().max(1) as f32;
        let mut pen_x = 0.0;
        let mut polygons = Vec::new();
        for character in text.chars() {
            if let Some(glyph) = self.glyph_for_char(character)
                && let Some(outline) = self.outline(glyph)
            {
                for contour in outline.flatten(8) {
                    polygons.push(
                        contour
                            .into_iter()
                            .map(|(x, y)| (pen_x + x * scale, y * scale))
                            .collect(),
                    );
                }
            }
            pen_x += self.advance_of_char(character) as f32 * scale;
        }
        polygons
    }

    fn glyph_range(&self, glyph: u16) -> Option<(usize, usize)> {
        let index = glyph as usize;
        let start = *self.loca.get(index)? as usize;
        let end = *self.loca.get(index + 1)? as usize;
        Some((start, end))
    }

    fn glyph_contours(&self, glyph: u16, depth: usize) -> Option<Vec<Contour>> {
        if depth > 8 {
            return None; // a cyclic or absurdly deep composite
        }
        let (start, end) = self.glyph_range(glyph)?;
        if end <= start {
            return Some(Vec::new()); // empty glyph (e.g. a space)
        }
        let data = self
            .data
            .get(self.glyf_offset + start..self.glyf_offset + end)?;
        let mut reader = Reader::new(data);
        let num_contours = reader.i16().ok()?;
        // Both simple and composite glyphs carry a bounding box after the
        // contour count; the simple/composite layouts diverge below it.
        reader.skip(8).ok()?;
        if num_contours >= 0 {
            parse_simple(&mut reader, num_contours as usize).ok()
        } else {
            self.parse_composite(&mut reader, depth).ok()
        }
    }

    fn parse_composite(&self, reader: &mut Reader, depth: usize) -> ParseResult<Vec<Contour>> {
        let mut contours = Vec::new();
        loop {
            let flags = reader.u16()?;
            let component = reader.u16()?;
            let (arg1, arg2) = if flags & ARG_1_AND_2_ARE_WORDS != 0 {
                (reader.i16()?, reader.i16()?)
            } else {
                (reader.u8()? as i8 as i16, reader.u8()? as i8 as i16)
            };
            let (a, b, c, d) = if flags & WE_HAVE_A_SCALE != 0 {
                let scale = f2dot14(reader.i16()?);
                (scale, 0.0, 0.0, scale)
            } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
                let sx = f2dot14(reader.i16()?);
                let sy = f2dot14(reader.i16()?);
                (sx, 0.0, 0.0, sy)
            } else if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
                (
                    f2dot14(reader.i16()?),
                    f2dot14(reader.i16()?),
                    f2dot14(reader.i16()?),
                    f2dot14(reader.i16()?),
                )
            } else {
                (1.0, 0.0, 0.0, 1.0)
            };

            // Point-matching components (not `ARGS_ARE_XY_VALUES`) are rare; we
            // treat them as no offset rather than implement point matching.
            let (dx, dy) = if flags & ARGS_ARE_XY_VALUES != 0 {
                (arg1 as f32, arg2 as f32)
            } else {
                (0.0, 0.0)
            };

            if let Some(mut component_contours) = self.glyph_contours(component, depth + 1) {
                for contour in &mut component_contours {
                    for point in &mut contour.points {
                        let x = a * point.x + c * point.y + dx;
                        let y = b * point.x + d * point.y + dy;
                        point.x = x;
                        point.y = y;
                    }
                }
                contours.append(&mut component_contours);
            }

            if flags & MORE_COMPONENTS == 0 {
                break;
            }
        }
        Ok(contours)
    }
}

const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
const ARGS_ARE_XY_VALUES: u16 = 0x0002;
const WE_HAVE_A_SCALE: u16 = 0x0008;
const MORE_COMPONENTS: u16 = 0x0020;
const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;

fn f2dot14(value: i16) -> f32 {
    value as f32 / 16384.0
}

fn parse_simple(reader: &mut Reader, num_contours: usize) -> ParseResult<Vec<Contour>> {
    if num_contours == 0 {
        return Ok(Vec::new());
    }

    let mut ends = Vec::with_capacity(num_contours);
    for _ in 0..num_contours {
        ends.push(reader.u16()? as usize);
    }
    let point_count = ends.last().copied().unwrap_or(0) + 1;

    let instruction_length = reader.u16()? as usize;
    reader.skip(instruction_length)?;

    // Flags, with the REPEAT bit expanding runs.
    let mut flags = Vec::with_capacity(point_count);
    while flags.len() < point_count {
        let flag = reader.u8()?;
        flags.push(flag);
        if flag & 0x08 != 0 {
            let repeat = reader.u8()? as usize;
            for _ in 0..repeat {
                if flags.len() < point_count {
                    flags.push(flag);
                }
            }
        }
    }

    // X deltas, then Y deltas, accumulated.
    let mut xs = Vec::with_capacity(point_count);
    let mut x = 0i32;
    for &flag in &flags {
        x += if flag & 0x02 != 0 {
            let value = reader.u8()? as i32;
            if flag & 0x10 != 0 { value } else { -value }
        } else if flag & 0x10 != 0 {
            0
        } else {
            reader.i16()? as i32
        };
        xs.push(x as f32);
    }
    let mut ys = Vec::with_capacity(point_count);
    let mut y = 0i32;
    for &flag in &flags {
        y += if flag & 0x04 != 0 {
            let value = reader.u8()? as i32;
            if flag & 0x20 != 0 { value } else { -value }
        } else if flag & 0x20 != 0 {
            0
        } else {
            reader.i16()? as i32
        };
        ys.push(y as f32);
    }

    let mut contours = Vec::with_capacity(num_contours);
    let mut start = 0;
    for &end in &ends {
        let mut points = Vec::with_capacity(end + 1 - start);
        for i in start..=end {
            points.push(Point {
                x: xs[i],
                y: ys[i],
                on_curve: flags[i] & 0x01 != 0,
            });
        }
        contours.push(Contour { points });
        start = end + 1;
    }
    Ok(contours)
}

/// Flatten one contour to a polygon, treating quadratic control points correctly
/// (including consecutive control points, whose midpoint is implicitly on-curve).
fn flatten_contour(points: &[Point], steps: usize) -> Vec<(f32, f32)> {
    let n = points.len();
    if n == 0 {
        return Vec::new();
    }

    // Rotate to begin at an on-curve point; if the contour is entirely off-curve
    // the start is the midpoint of the last and first points.
    let (ordered, start): (Vec<Point>, (f32, f32)) =
        match points.iter().position(|point| point.on_curve) {
            Some(first) => {
                let rotated: Vec<Point> = (0..n).map(|k| points[(first + k) % n]).collect();
                let origin = (rotated[0].x, rotated[0].y);
                (rotated, origin)
            }
            None => (points.to_vec(), midpoint(&points[n - 1], &points[0])),
        };

    let mut polygon = vec![start];
    let mut current = start;
    for i in 0..n {
        let point = ordered[i];
        if point.on_curve {
            let end = (point.x, point.y);
            if !same(end, current) {
                polygon.push(end);
                current = end;
            }
        } else {
            let next = ordered[(i + 1) % n];
            let end = if next.on_curve {
                (next.x, next.y)
            } else {
                midpoint(&point, &next)
            };
            flatten_quad(current, (point.x, point.y), end, steps, &mut polygon);
            current = end;
        }
    }
    polygon
}

fn flatten_quad(
    from: (f32, f32),
    control: (f32, f32),
    to: (f32, f32),
    steps: usize,
    out: &mut Vec<(f32, f32)>,
) {
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        let u = 1.0 - t;
        out.push((
            u * u * from.0 + 2.0 * u * t * control.0 + t * t * to.0,
            u * u * from.1 + 2.0 * u * t * control.1 + t * t * to.1,
        ));
    }
}

fn midpoint(a: &Point, b: &Point) -> (f32, f32) {
    ((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}

fn same(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
}
