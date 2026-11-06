//! Parsing SVG into the shape model, reusing the HTML tokenizer and DOM.

use noonoo_css::Color;
use noonoo_dom::{Document, NodeId};

use crate::{Geometry, Shape, Svg, Transform};

pub(crate) fn parse(source: &str) -> Svg {
    let document = noonoo_html::parse(source);
    let Some(root) = document.get_elements_by_tag_name("svg").into_iter().next() else {
        return Svg::default();
    };

    let width = attr(&document, root, "width")
        .and_then(|value| number(&value))
        .unwrap_or(300.0);
    let height = attr(&document, root, "height")
        .and_then(|value| number(&value))
        .unwrap_or(150.0);
    let view_box = attr(&document, root, "viewbox").and_then(|value| parse_view_box(&value));

    let mut shapes = Vec::new();
    walk(&document, root, Transform::IDENTITY, &mut shapes);
    Svg {
        width,
        height,
        view_box,
        shapes,
    }
}

fn walk(document: &Document, node: NodeId, transform: Transform, out: &mut Vec<Shape>) {
    for child in document.tree().children(node).to_vec() {
        let Some(element) = document.element(child) else {
            continue;
        };
        match element.name.as_str() {
            "g" => {
                let local = attr(document, child, "transform")
                    .map(|value| parse_transform(&value))
                    .unwrap_or(Transform::IDENTITY);
                walk(document, child, Transform::compose(transform, local), out);
            }
            "rect" | "circle" | "ellipse" | "line" | "polygon" | "polyline" | "path" => {
                if let Some(shape) = shape(document, child, transform) {
                    out.push(shape);
                }
            }
            _ => {}
        }
    }
}

fn shape(document: &Document, node: NodeId, transform: Transform) -> Option<Shape> {
    let name = document.element(node)?.name.clone();
    let geometry = geometry(document, node, &name)?;
    let opacity = attr(document, node, "opacity")
        .and_then(|value| number(&value))
        .unwrap_or(1.0);

    let fill = match attr(document, node, "fill").as_deref() {
        Some("none") => None,
        Some(spec) => color(spec),
        None => Some(Color::BLACK),
    }
    .map(|color| with_opacity(color, opacity));

    let stroke = match attr(document, node, "stroke").as_deref() {
        Some("none") | None => None,
        Some(spec) => color(spec).map(|color| {
            let width = attr(document, node, "stroke-width")
                .and_then(|value| number(&value))
                .unwrap_or(1.0);
            (with_opacity(color, opacity), width)
        }),
    };

    Some(Shape {
        geometry,
        fill,
        stroke,
        transform,
    })
}

fn geometry(document: &Document, node: NodeId, name: &str) -> Option<Geometry> {
    let n = |key: &str| attr(document, node, key).and_then(|value| number(&value));
    match name {
        "rect" => Some(Geometry::Rect {
            x: n("x").unwrap_or(0.0),
            y: n("y").unwrap_or(0.0),
            width: n("width").unwrap_or(0.0),
            height: n("height").unwrap_or(0.0),
        }),
        "circle" => Some(Geometry::Circle {
            cx: n("cx").unwrap_or(0.0),
            cy: n("cy").unwrap_or(0.0),
            r: n("r").unwrap_or(0.0),
        }),
        "ellipse" => Some(Geometry::Ellipse {
            cx: n("cx").unwrap_or(0.0),
            cy: n("cy").unwrap_or(0.0),
            rx: n("rx").unwrap_or(0.0),
            ry: n("ry").unwrap_or(0.0),
        }),
        "line" => Some(Geometry::Line {
            x1: n("x1").unwrap_or(0.0),
            y1: n("y1").unwrap_or(0.0),
            x2: n("x2").unwrap_or(0.0),
            y2: n("y2").unwrap_or(0.0),
        }),
        "polygon" => Some(Geometry::Polygon(point_pairs(
            &attr(document, node, "points").unwrap_or_default(),
        ))),
        "polyline" => Some(Geometry::Polyline(point_pairs(
            &attr(document, node, "points").unwrap_or_default(),
        ))),
        "path" => {
            let (points, closed) = parse_path(&attr(document, node, "d").unwrap_or_default());
            Some(Geometry::Path { points, closed })
        }
        _ => None,
    }
}

fn attr(document: &Document, node: NodeId, name: &str) -> Option<String> {
    document.element(node)?.attr(name).map(str::to_owned)
}

/// Parse a leading number, ignoring any CSS unit suffix (`10px`, `50%`).
fn number(text: &str) -> Option<f32> {
    let numeric: String = text
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'))
        .collect();
    numeric.parse().ok()
}

fn numbers(text: &str) -> Vec<f32> {
    text.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}

fn point_pairs(text: &str) -> Vec<(f32, f32)> {
    numbers(text)
        .chunks(2)
        .filter(|chunk| chunk.len() == 2)
        .map(|chunk| (chunk[0], chunk[1]))
        .collect()
}

fn parse_view_box(text: &str) -> Option<[f32; 4]> {
    let values = numbers(text);
    (values.len() == 4).then(|| [values[0], values[1], values[2], values[3]])
}

fn color(spec: &str) -> Option<Color> {
    let spec = spec.trim();
    if let Some(hex) = spec.strip_prefix('#') {
        return Color::from_hex(hex);
    }
    Color::named(spec).or(Some(Color::BLACK))
}

fn with_opacity(color: Color, opacity: f32) -> Color {
    let alpha = (color.a as f32 * opacity.clamp(0.0, 1.0)).round() as u8;
    Color::rgba(color.r, color.g, color.b, alpha)
}

fn parse_transform(text: &str) -> Transform {
    let mut transform = Transform::IDENTITY;
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let name = rest[..open]
            .split(|c: char| !c.is_ascii_alphabetic())
            .next_back()
            .unwrap_or("");
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let args = numbers(&rest[open + 1..open + close]);
        let step = match name {
            "translate" => Transform {
                sx: 1.0,
                sy: 1.0,
                tx: args.first().copied().unwrap_or(0.0),
                ty: args.get(1).copied().unwrap_or(0.0),
            },
            "scale" => {
                let sx = args.first().copied().unwrap_or(1.0);
                let sy = args.get(1).copied().unwrap_or(sx);
                Transform {
                    sx,
                    sy,
                    tx: 0.0,
                    ty: 0.0,
                }
            }
            _ => Transform::IDENTITY,
        };
        transform = Transform::compose(transform, step);
        rest = &rest[open + close + 1..];
    }
    transform
}

/// Flatten a path `d` to a single polyline (M/L/H/V/Z; curve commands are
/// approximated by a line to their endpoint).
fn parse_path(d: &str) -> (Vec<(f32, f32)>, bool) {
    enum Token {
        Cmd(char),
        Num(f32),
    }

    let mut tokens = Vec::new();
    let mut chars = d.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_alphabetic() {
            tokens.push(Token::Cmd(ch));
            chars.next();
        } else if ch == ',' || ch.is_whitespace() {
            chars.next();
        } else {
            let mut text = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E') {
                    text.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            if let Ok(value) = text.parse() {
                tokens.push(Token::Num(value));
            }
        }
    }

    let mut points = Vec::new();
    let mut index = 0;
    let (mut cx, mut cy) = (0.0_f32, 0.0_f32);
    let (mut sx, mut sy) = (0.0_f32, 0.0_f32);
    let mut closed = false;
    let mut current = 'M';

    while index < tokens.len() {
        if let Token::Cmd(command) = tokens[index] {
            current = command;
            index += 1;
        }
        let relative = current.is_ascii_lowercase();
        let upper = current.to_ascii_uppercase();

        let take = |index: &mut usize, count: usize| -> Option<Vec<f32>> {
            let mut out = Vec::with_capacity(count);
            for _ in 0..count {
                if let Some(Token::Num(value)) = tokens.get(*index) {
                    out.push(*value);
                    *index += 1;
                } else {
                    return None;
                }
            }
            Some(out)
        };

        match upper {
            'M' => {
                let Some(a) = take(&mut index, 2) else { break };
                (cx, cy) = if relative {
                    (cx + a[0], cy + a[1])
                } else {
                    (a[0], a[1])
                };
                (sx, sy) = (cx, cy);
                points.push((cx, cy));
                current = if relative { 'l' } else { 'L' };
            }
            'L' => {
                let Some(a) = take(&mut index, 2) else { break };
                (cx, cy) = if relative {
                    (cx + a[0], cy + a[1])
                } else {
                    (a[0], a[1])
                };
                points.push((cx, cy));
            }
            'H' => {
                let Some(a) = take(&mut index, 1) else { break };
                cx = if relative { cx + a[0] } else { a[0] };
                points.push((cx, cy));
            }
            'V' => {
                let Some(a) = take(&mut index, 1) else { break };
                cy = if relative { cy + a[0] } else { a[0] };
                points.push((cx, cy));
            }
            'C' => {
                let Some(a) = take(&mut index, 6) else { break };
                (cx, cy) = if relative {
                    (cx + a[4], cy + a[5])
                } else {
                    (a[4], a[5])
                };
                points.push((cx, cy));
            }
            'S' | 'Q' => {
                let Some(a) = take(&mut index, 4) else { break };
                (cx, cy) = if relative {
                    (cx + a[2], cy + a[3])
                } else {
                    (a[2], a[3])
                };
                points.push((cx, cy));
            }
            'T' => {
                let Some(a) = take(&mut index, 2) else { break };
                (cx, cy) = if relative {
                    (cx + a[0], cy + a[1])
                } else {
                    (a[0], a[1])
                };
                points.push((cx, cy));
            }
            'A' => {
                let Some(a) = take(&mut index, 7) else { break };
                (cx, cy) = if relative {
                    (cx + a[5], cy + a[6])
                } else {
                    (a[5], a[6])
                };
                points.push((cx, cy));
            }
            'Z' => {
                closed = true;
                (cx, cy) = (sx, sy);
            }
            _ => break,
        }
    }

    (points, closed)
}
