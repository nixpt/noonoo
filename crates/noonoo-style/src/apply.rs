//! Resolving cascaded values into a `ComputedStyle`: absolute-ising lengths and
//! applying each property.

use noonoo_css::Property;
use noonoo_css::value::{Length as CssLength, LengthUnit as CssLengthUnit, Value};

use crate::computed::*;
// --- value resolution ------------------------------------------------------

fn to_px(length: &CssLength, font_size: f32, root_font_size: f32) -> f32 {
    let value = length.value;
    match length.unit {
        CssLengthUnit::Px => value,
        CssLengthUnit::Em => value * font_size,
        CssLengthUnit::Rem => value * root_font_size,
        CssLengthUnit::Ex | CssLengthUnit::Ch => value * font_size * 0.5,
        CssLengthUnit::Pt => value * 96.0 / 72.0,
        CssLengthUnit::Pc => value * 16.0,
        CssLengthUnit::In => value * 96.0,
        CssLengthUnit::Cm => value * 96.0 / 2.54,
        CssLengthUnit::Mm => value * 96.0 / 25.4,
        CssLengthUnit::Q => value * 96.0 / 101.6,
        // Viewport-relative units need a viewport; approximate with the root.
        CssLengthUnit::Vw | CssLengthUnit::Vh | CssLengthUnit::Vmin | CssLengthUnit::Vmax => value,
        CssLengthUnit::Other(_) => 0.0,
    }
}

pub(crate) fn resolve_font_size(values: &[Value], parent: f32, root: f32) -> Option<f32> {
    match values {
        [Value::Length(length)] => Some(to_px(length, parent, root).max(0.0)),
        [Value::Percentage(percent)] => Some((parent * percent / 100.0).max(0.0)),
        [Value::Number(number)] => Some((*number).max(0.0)),
        [Value::Keyword(keyword)] => match keyword.as_str() {
            "xx-small" => Some(9.0),
            "x-small" => Some(10.0),
            "small" => Some(13.0),
            "medium" => Some(16.0),
            "large" => Some(18.0),
            "x-large" => Some(24.0),
            "xx-large" => Some(32.0),
            "smaller" => Some(parent * 0.8),
            "larger" => Some(parent * 1.2),
            _ => None,
        },
        _ => None,
    }
}

fn keyword(values: &[Value]) -> Option<&str> {
    match values {
        [Value::Keyword(keyword)] => Some(keyword),
        _ => None,
    }
}

fn dimension(values: &[Value], font_size: f32, root: f32) -> Option<Dimension> {
    match values {
        [Value::Keyword(keyword)] if keyword == "auto" => Some(Dimension::Auto),
        [Value::Length(length)] => Some(Dimension::Px(to_px(length, font_size, root))),
        [Value::Percentage(percent)] => Some(Dimension::Percent(*percent)),
        [Value::Number(number)] => Some(Dimension::Px(*number)),
        _ => None,
    }
}

fn length(values: &[Value], font_size: f32, root: f32) -> Option<Length> {
    match values {
        [Value::Length(l)] => Some(Length::Px(to_px(l, font_size, root).max(0.0))),
        [Value::Percentage(percent)] => Some(Length::Percent(*percent)),
        [Value::Number(number)] => Some(Length::Px((*number).max(0.0))),
        _ => None,
    }
}

fn color(values: &[Value]) -> Option<noonoo_css::Color> {
    match values {
        [Value::Color(color)] => Some(*color),
        [Value::Keyword(keyword)] => noonoo_css::Color::named(keyword),
        _ => None,
    }
}

fn border_width_px(values: &[Value], font_size: f32) -> Option<f32> {
    match values {
        [Value::Keyword(keyword)] => match keyword.as_str() {
            "thin" => Some(1.0),
            "medium" => Some(3.0),
            "thick" => Some(5.0),
            _ => None,
        },
        [Value::Length(l)] => Some(to_px(l, font_size, 16.0).max(0.0)),
        [Value::Number(n)] => Some((*n).max(0.0)),
        _ => None,
    }
}

// --- applying a property ---------------------------------------------------

pub(crate) fn apply(
    style: &mut ComputedStyle,
    property: &Property,
    values: &[Value],
    font_size: f32,
    root: f32,
) {
    match property {
        Property::Display => {
            if let Some(k) = keyword(values) {
                style.display = match k {
                    "none" => Display::None,
                    "block" => Display::Block,
                    "inline" => Display::Inline,
                    "inline-block" => Display::InlineBlock,
                    "flex" => Display::Flex,
                    "inline-flex" => Display::InlineFlex,
                    "list-item" => Display::ListItem,
                    _ => style.display,
                };
            }
        }
        Property::Position => {
            if let Some(k) = keyword(values) {
                style.position = match k {
                    "static" => Position::Static,
                    "relative" => Position::Relative,
                    "absolute" => Position::Absolute,
                    "fixed" => Position::Fixed,
                    "sticky" => Position::Sticky,
                    _ => style.position,
                };
            }
        }
        Property::Width => set_first(&mut style.width, dimension(values, font_size, root)),
        Property::Height => set_first(&mut style.height, dimension(values, font_size, root)),
        Property::MarginTop => set_first(&mut style.margin.top, dimension(values, font_size, root)),
        Property::MarginRight => {
            set_first(&mut style.margin.right, dimension(values, font_size, root))
        }
        Property::MarginBottom => {
            set_first(&mut style.margin.bottom, dimension(values, font_size, root))
        }
        Property::MarginLeft => {
            set_first(&mut style.margin.left, dimension(values, font_size, root))
        }
        Property::PaddingTop => set_first(&mut style.padding.top, length(values, font_size, root)),
        Property::PaddingRight => {
            set_first(&mut style.padding.right, length(values, font_size, root))
        }
        Property::PaddingBottom => {
            set_first(&mut style.padding.bottom, length(values, font_size, root))
        }
        Property::PaddingLeft => {
            set_first(&mut style.padding.left, length(values, font_size, root))
        }
        Property::BorderTopWidth => set_first(
            &mut style.border_width.top,
            border_width_px(values, font_size),
        ),
        Property::BorderRightWidth => set_first(
            &mut style.border_width.right,
            border_width_px(values, font_size),
        ),
        Property::BorderBottomWidth => set_first(
            &mut style.border_width.bottom,
            border_width_px(values, font_size),
        ),
        Property::BorderLeftWidth => set_first(
            &mut style.border_width.left,
            border_width_px(values, font_size),
        ),
        Property::BorderTopStyle => set_border_style(&mut style.border_style.top, values),
        Property::BorderRightStyle => set_border_style(&mut style.border_style.right, values),
        Property::BorderBottomStyle => set_border_style(&mut style.border_style.bottom, values),
        Property::BorderLeftStyle => set_border_style(&mut style.border_style.left, values),
        Property::BorderTopColor => set_first(&mut style.border_color.top, color(values)),
        Property::BorderRightColor => set_first(&mut style.border_color.right, color(values)),
        Property::BorderBottomColor => set_first(&mut style.border_color.bottom, color(values)),
        Property::BorderLeftColor => set_first(&mut style.border_color.left, color(values)),
        Property::BoxSizing => {
            if let Some(k) = keyword(values) {
                style.box_sizing = match k {
                    "border-box" => BoxSizing::BorderBox,
                    _ => BoxSizing::ContentBox,
                };
            }
        }
        Property::FlexDirection => {
            if let Some(k) = keyword(values) {
                style.flex_direction = match k {
                    "row-reverse" => FlexDirection::RowReverse,
                    "column" => FlexDirection::Column,
                    "column-reverse" => FlexDirection::ColumnReverse,
                    _ => FlexDirection::Row,
                };
            }
        }
        Property::FlexWrap => {
            if let Some(k) = keyword(values) {
                style.flex_wrap = match k {
                    "wrap" => FlexWrap::Wrap,
                    "wrap-reverse" => FlexWrap::WrapReverse,
                    _ => FlexWrap::NoWrap,
                };
            }
        }
        Property::FlexGrow => set_number(&mut style.flex_grow, values),
        Property::FlexShrink => set_number(&mut style.flex_shrink, values),
        Property::FlexBasis => set_first(&mut style.flex_basis, dimension(values, font_size, root)),
        Property::JustifyContent => set_justify(&mut style.justify_content, values),
        Property::AlignItems => {
            set_align(&mut style.align_items, values);
        }
        Property::AlignSelf => {
            let mut value = style.align_items;
            if set_align(&mut value, values).is_some() {
                style.align_self = Some(value);
            }
        }
        Property::RowGap => set_px(&mut style.row_gap, values, font_size, root),
        Property::ColumnGap => set_px(&mut style.column_gap, values, font_size, root),
        Property::Color => set_first(&mut style.color, color(values)),
        Property::BackgroundColor => set_first(&mut style.background_color, color(values)),
        Property::FontWeight => set_font_weight(&mut style.font_weight, values),
        Property::FontStyle => {
            if let Some(k) = keyword(values) {
                style.font_style = match k {
                    "italic" => FontStyle::Italic,
                    "oblique" => FontStyle::Oblique,
                    _ => FontStyle::Normal,
                };
            }
        }
        Property::FontFamily => style.font_family = font_family(values),
        Property::LineHeight => set_line_height(&mut style.line_height, values, font_size),
        Property::TextAlign => set_text_align(&mut style.text_align, values),
        Property::WhiteSpace => set_white_space(&mut style.white_space, values),
        Property::Visibility => set_visibility(&mut style.visibility, values),
        Property::Opacity => {
            if let [Value::Number(number)] = values {
                style.opacity = number.clamp(0.0, 1.0);
            }
        }
        Property::OverflowX | Property::OverflowY => {
            if let Some(overflow) = overflow(values) {
                if matches!(property, Property::OverflowX) {
                    style.overflow_x = overflow;
                } else {
                    style.overflow_y = overflow;
                }
            }
        }
        Property::ZIndex => {
            style.z_index = match values {
                [Value::Number(number)] => ZIndex::Number(*number as i32),
                [Value::Keyword(k)] if k == "auto" => ZIndex::Auto,
                _ => style.z_index,
            };
        }
        Property::Cursor => {
            if let Some(k) = keyword(values) {
                style.cursor = match k {
                    "auto" => Cursor::Auto,
                    "default" => Cursor::Default,
                    "pointer" => Cursor::Pointer,
                    "text" => Cursor::Text,
                    "move" => Cursor::Move,
                    "not-allowed" => Cursor::NotAllowed,
                    other => Cursor::Other(other.to_owned()),
                };
            }
        }
        _ => {}
    }
}

fn set_first<T>(slot: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *slot = value;
    }
}

fn set_number(slot: &mut f32, values: &[Value]) {
    if let [Value::Number(number)] = values {
        *slot = *number;
    }
}

fn set_px(slot: &mut f32, values: &[Value], font_size: f32, root: f32) {
    if let Some(Length::Px(px)) = length(values, font_size, root) {
        *slot = px;
    }
}

fn set_border_style(slot: &mut BorderStyle, values: &[Value]) {
    let Some(k) = keyword(values) else { return };
    *slot = match k {
        "none" => BorderStyle::None,
        "hidden" => BorderStyle::Hidden,
        "solid" => BorderStyle::Solid,
        "dashed" => BorderStyle::Dashed,
        "dotted" => BorderStyle::Dotted,
        "double" => BorderStyle::Double,
        "groove" => BorderStyle::Groove,
        "ridge" => BorderStyle::Ridge,
        "inset" => BorderStyle::Inset,
        "outset" => BorderStyle::Outset,
        _ => return,
    };
}

fn set_justify(slot: &mut JustifyContent, values: &[Value]) {
    let Some(k) = keyword(values) else { return };
    *slot = match k {
        "flex-start" | "start" => JustifyContent::Start,
        "flex-end" | "end" => JustifyContent::End,
        "center" => JustifyContent::Center,
        "space-between" => JustifyContent::SpaceBetween,
        "space-around" => JustifyContent::SpaceAround,
        "space-evenly" => JustifyContent::SpaceEvenly,
        _ => return,
    };
}

fn set_align(slot: &mut AlignItems, values: &[Value]) -> Option<()> {
    let k = keyword(values)?;
    *slot = match k {
        "stretch" => AlignItems::Stretch,
        "flex-start" | "start" => AlignItems::Start,
        "flex-end" | "end" => AlignItems::End,
        "center" => AlignItems::Center,
        "baseline" => AlignItems::Baseline,
        _ => return None,
    };
    Some(())
}

fn set_font_weight(slot: &mut u16, values: &[Value]) {
    let weight = match values {
        [Value::Number(number)] => *number as u16,
        [Value::Keyword(k)] => match k.as_str() {
            "normal" => 400,
            "bold" => 700,
            "bolder" => 700,
            "lighter" => 300,
            _ => return,
        },
        _ => return,
    };
    *slot = weight;
}

fn font_family(values: &[Value]) -> Vec<String> {
    values
        .iter()
        .filter_map(|value| match value {
            Value::Keyword(name) | Value::Str(name) => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn set_line_height(slot: &mut f32, values: &[Value], font_size: f32) {
    let value = match values {
        [Value::Number(number)] => font_size * *number,
        [Value::Length(l)] => to_px(l, font_size, font_size),
        [Value::Percentage(percent)] => font_size * percent / 100.0,
        [Value::Keyword(k)] if k == "normal" => font_size * 1.2,
        _ => return,
    };
    *slot = value;
}

fn set_text_align(slot: &mut TextAlign, values: &[Value]) {
    let Some(k) = keyword(values) else { return };
    *slot = match k {
        "start" => TextAlign::Start,
        "end" => TextAlign::End,
        "left" => TextAlign::Left,
        "right" => TextAlign::Right,
        "center" => TextAlign::Center,
        "justify" => TextAlign::Justify,
        _ => return,
    };
}

fn set_white_space(slot: &mut WhiteSpace, values: &[Value]) {
    let Some(k) = keyword(values) else { return };
    *slot = match k {
        "normal" => WhiteSpace::Normal,
        "nowrap" => WhiteSpace::NoWrap,
        "pre" => WhiteSpace::Pre,
        "pre-wrap" => WhiteSpace::PreWrap,
        "pre-line" => WhiteSpace::PreLine,
        _ => return,
    };
}

fn set_visibility(slot: &mut Visibility, values: &[Value]) {
    let Some(k) = keyword(values) else { return };
    *slot = match k {
        "visible" => Visibility::Visible,
        "hidden" => Visibility::Hidden,
        "collapse" => Visibility::Collapse,
        _ => return,
    };
}

fn overflow(values: &[Value]) -> Option<Overflow> {
    let k = keyword(values)?;
    Some(match k {
        "visible" => Overflow::Visible,
        "hidden" => Overflow::Hidden,
        "clip" => Overflow::Clip,
        "scroll" => Overflow::Scroll,
        "auto" => Overflow::Auto,
        _ => return None,
    })
}
