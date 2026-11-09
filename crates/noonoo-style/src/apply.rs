//! Resolving cascaded values into a `ComputedStyle`: absolute-ising lengths and
//! applying each property.

use noonoo_css::Property;
use noonoo_css::value::Value;

use crate::Viewport;
use crate::calc::{self, CalcValue, to_px};
use crate::computed::*;
// --- value resolution ------------------------------------------------------

/// A lone math function (`calc()`, `min()`, `max()`, `clamp()`) in `values`.
fn math_fn(values: &[Value]) -> Option<(&str, &[Value])> {
    match values {
        [Value::Function { name, args }]
            if name.eq_ignore_ascii_case("calc")
                || name.eq_ignore_ascii_case("min")
                || name.eq_ignore_ascii_case("max")
                || name.eq_ignore_ascii_case("clamp") =>
        {
            Some((name.as_str(), args))
        }
        _ => None,
    }
}

/// Normalise a math result into a [`Dimension`].
fn calc_dimension(value: CalcValue) -> Option<Dimension> {
    match value {
        CalcValue::Number(number) => Some(Dimension::Px(number)),
        CalcValue::Length { px, percent } => Some(if percent == 0.0 {
            Dimension::Px(px)
        } else if px == 0.0 {
            Dimension::Percent(percent)
        } else {
            Dimension::Calc { px, percent }
        }),
        CalcValue::Extreme { kind, terms } => Some(Dimension::Extreme {
            kind,
            terms: terms.into_boxed_slice(),
        }),
    }
}

/// Normalise a math result into a [`Length`].
///
/// The absolute part keeps its sign here; layout clamps *after* adding the
/// percentage, so `calc(100% - 20px)` stays correct for a narrow container.
fn calc_length(value: CalcValue) -> Option<Length> {
    match value {
        CalcValue::Number(number) => Some(Length::Px(number)),
        CalcValue::Length { px, percent } => Some(if percent == 0.0 {
            Length::Px(px)
        } else if px == 0.0 {
            Length::Percent(percent)
        } else {
            Length::Calc { px, percent }
        }),
        CalcValue::Extreme { kind, terms } => Some(Length::Extreme {
            kind,
            terms: terms.into_boxed_slice(),
        }),
    }
}

pub(crate) fn resolve_font_size(
    values: &[Value],
    parent: f32,
    root: f32,
    viewport: Viewport,
) -> Option<f32> {
    if let Some((name, args)) = math_fn(values) {
        let value = calc::evaluate_function(name, args, parent, root, viewport)?;
        // A percentage in `font-size` is relative to the parent font size.
        return calc::resolve(&value, parent).map(|size| size.max(0.0));
    }
    match values {
        [Value::Length(length)] => Some(to_px(length, parent, root, viewport).max(0.0)),
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
            // CSS-wide keywords: `font-size` inherits, and starts `medium`.
            "initial" => Some(16.0),
            "inherit" | "unset" => Some(parent),
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

pub(crate) fn dimension(
    values: &[Value],
    font_size: f32,
    root: f32,
    viewport: Viewport,
) -> Option<Dimension> {
    if let Some((name, args)) = math_fn(values) {
        let value = calc::evaluate_function(name, args, font_size, root, viewport)?;
        return calc_dimension(value);
    }
    match values {
        [Value::Keyword(keyword)] if keyword == "auto" => Some(Dimension::Auto),
        [Value::Length(length)] => Some(Dimension::Px(to_px(length, font_size, root, viewport))),
        [Value::Percentage(percent)] => Some(Dimension::Percent(*percent)),
        [Value::Number(number)] => Some(Dimension::Px(*number)),
        _ => None,
    }
}

fn length(values: &[Value], font_size: f32, root: f32, viewport: Viewport) -> Option<Length> {
    if let Some((name, args)) = math_fn(values) {
        let value = calc::evaluate_function(name, args, font_size, root, viewport)?;
        return calc_length(value);
    }
    match values {
        [Value::Length(l)] => Some(Length::Px(to_px(l, font_size, root, viewport).max(0.0))),
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

fn border_width_px(values: &[Value], font_size: f32, viewport: Viewport) -> Option<f32> {
    if let Some((name, args)) = math_fn(values) {
        let value = calc::evaluate_function(name, args, font_size, 16.0, viewport)?;
        // Border widths must be absolute; percentages have no reference here.
        return value
            .is_absolute()
            .then(|| calc::resolve(&value, 0.0))
            .flatten()
            .map(|width| width.max(0.0));
    }
    match values {
        [Value::Keyword(keyword)] => match keyword.as_str() {
            "thin" => Some(1.0),
            "medium" => Some(3.0),
            "thick" => Some(5.0),
            _ => None,
        },
        [Value::Length(l)] => Some(to_px(l, font_size, 16.0, viewport).max(0.0)),
        [Value::Number(n)] => Some((*n).max(0.0)),
        _ => None,
    }
}

// --- applying a property ---------------------------------------------------

fn has_current_color(value: &Value) -> bool {
    match value {
        Value::Keyword(k) => k.eq_ignore_ascii_case("currentcolor"),
        Value::Function { args, .. } | Value::Group(args) => args.iter().any(has_current_color),
        _ => false,
    }
}

fn with_current_color(value: &Value, color: noonoo_css::Color) -> Value {
    match value {
        Value::Keyword(k) if k.eq_ignore_ascii_case("currentcolor") => Value::Color(color),
        Value::Function { name, args } => Value::Function {
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| with_current_color(arg, color))
                .collect(),
        },
        Value::Group(args) => Value::Group(
            args.iter()
                .map(|arg| with_current_color(arg, color))
                .collect(),
        ),
        other => other.clone(),
    }
}

pub(crate) fn apply(
    style: &mut ComputedStyle,
    property: &Property,
    values: &[Value],
    font_size: f32,
    root: f32,
    viewport: Viewport,
    parent: Option<&ComputedStyle>,
) {
    // `currentColor` is this element's `color` (applied first by the
    // cascade); on `color` itself that is the inherited colour.
    let substituted;
    let values = if values.iter().any(has_current_color) {
        substituted = values
            .iter()
            .map(|value| with_current_color(value, style.color))
            .collect::<Vec<_>>();
        &substituted[..]
    } else {
        values
    };
    // Global CSS-wide keywords: `inherit` / `initial` / `unset`.
    if let [Value::Keyword(keyword)] = values {
        match keyword.to_ascii_lowercase().as_str() {
            "inherit" => {
                if let Some(parent) = parent {
                    copy_property(style, parent, property);
                }
                return;
            }
            "initial" => {
                let initial = ComputedStyle::default();
                copy_property(style, &initial, property);
                return;
            }
            "unset" => {
                if is_inherited(property) {
                    if let Some(parent) = parent {
                        copy_property(style, parent, property);
                    }
                } else {
                    let initial = ComputedStyle::default();
                    copy_property(style, &initial, property);
                }
                return;
            }
            _ => {}
        }
    }

    match property {
        Property::Display => {
            if let Some(k) = keyword(values) {
                style.display = match k {
                    "none" => Display::None,
                    "block" => Display::Block,
                    "flow-root" => Display::FlowRoot,
                    "inline" => Display::Inline,
                    "inline-block" => Display::InlineBlock,
                    "flex" => Display::Flex,
                    "inline-flex" => Display::InlineFlex,
                    "grid" => Display::Grid,
                    "inline-grid" => Display::InlineGrid,
                    "list-item" => Display::ListItem,
                    "table" => Display::Table,
                    "table-row-group" | "thead-body-group" => Display::TableRowGroup,
                    "table-row" => Display::TableRow,
                    "table-cell" => Display::TableCell,
                    _ => style.display,
                };
            }
        }
        Property::BorderCollapse => {
            if let Some(k) = keyword(values) {
                style.border_collapse = match k {
                    "collapse" => BorderCollapse::Collapse,
                    "separate" => BorderCollapse::Separate,
                    _ => style.border_collapse,
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
        Property::Float => {
            if let Some(k) = keyword(values) {
                style.float = match k {
                    "left" => Float::Left,
                    "right" => Float::Right,
                    _ => Float::None,
                };
            }
        }
        Property::Clear => {
            if let Some(k) = keyword(values) {
                style.clear = match k {
                    "left" => Clear::Left,
                    "right" => Clear::Right,
                    "both" => Clear::Both,
                    _ => Clear::None,
                };
            }
        }
        Property::GridTemplateColumns => {
            if let Some(tracks) = crate::grid::tracks(values, font_size, root, viewport) {
                style.grid_template_columns = tracks;
            }
        }
        Property::GridTemplateRows => {
            if let Some(tracks) = crate::grid::tracks(values, font_size, root, viewport) {
                style.grid_template_rows = tracks;
            }
        }
        Property::GridAutoRows => {
            if let Some(tracks) = crate::grid::tracks(values, font_size, root, viewport) {
                style.grid_auto_rows = tracks;
            }
        }
        Property::GridAutoColumns => {
            if let Some(tracks) = crate::grid::tracks(values, font_size, root, viewport) {
                style.grid_auto_columns = tracks;
            }
        }
        Property::GridColumn => {
            if let Some(range) = crate::grid::placement(values) {
                style.grid_column = range;
            }
        }
        Property::GridRow => {
            if let Some(range) = crate::grid::placement(values) {
                style.grid_row = range;
            }
        }
        Property::MinWidth => set_first(
            &mut style.min_width,
            dimension(values, font_size, root, viewport),
        ),
        Property::MaxWidth => set_first(
            &mut style.max_width,
            dimension(values, font_size, root, viewport),
        ),
        Property::MinHeight => set_first(
            &mut style.min_height,
            dimension(values, font_size, root, viewport),
        ),
        Property::MaxHeight => set_first(
            &mut style.max_height,
            dimension(values, font_size, root, viewport),
        ),
        Property::Width => set_first(
            &mut style.width,
            dimension(values, font_size, root, viewport),
        ),
        Property::Height => set_first(
            &mut style.height,
            dimension(values, font_size, root, viewport),
        ),
        Property::Top => set_first(
            &mut style.inset.top,
            dimension(values, font_size, root, viewport),
        ),
        Property::Right => set_first(
            &mut style.inset.right,
            dimension(values, font_size, root, viewport),
        ),
        Property::Bottom => set_first(
            &mut style.inset.bottom,
            dimension(values, font_size, root, viewport),
        ),
        Property::Left => set_first(
            &mut style.inset.left,
            dimension(values, font_size, root, viewport),
        ),
        Property::MarginTop => set_first(
            &mut style.margin.top,
            dimension(values, font_size, root, viewport),
        ),
        Property::MarginRight => set_first(
            &mut style.margin.right,
            dimension(values, font_size, root, viewport),
        ),
        Property::MarginBottom => set_first(
            &mut style.margin.bottom,
            dimension(values, font_size, root, viewport),
        ),
        Property::MarginLeft => set_first(
            &mut style.margin.left,
            dimension(values, font_size, root, viewport),
        ),
        Property::PaddingTop => set_first(
            &mut style.padding.top,
            length(values, font_size, root, viewport),
        ),
        Property::PaddingRight => set_first(
            &mut style.padding.right,
            length(values, font_size, root, viewport),
        ),
        Property::PaddingBottom => set_first(
            &mut style.padding.bottom,
            length(values, font_size, root, viewport),
        ),
        Property::PaddingLeft => set_first(
            &mut style.padding.left,
            length(values, font_size, root, viewport),
        ),
        Property::BorderTopWidth => set_first(
            &mut style.border_width.top,
            border_width_px(values, font_size, viewport),
        ),
        Property::BorderRightWidth => set_first(
            &mut style.border_width.right,
            border_width_px(values, font_size, viewport),
        ),
        Property::BorderBottomWidth => set_first(
            &mut style.border_width.bottom,
            border_width_px(values, font_size, viewport),
        ),
        Property::BorderLeftWidth => set_first(
            &mut style.border_width.left,
            border_width_px(values, font_size, viewport),
        ),
        Property::BorderTopStyle => set_border_style(&mut style.border_style.top, values),
        Property::BorderRightStyle => set_border_style(&mut style.border_style.right, values),
        Property::BorderBottomStyle => set_border_style(&mut style.border_style.bottom, values),
        Property::BorderLeftStyle => set_border_style(&mut style.border_style.left, values),
        Property::BorderTopColor => set_first(&mut style.border_color.top, color(values)),
        Property::BorderRightColor => set_first(&mut style.border_color.right, color(values)),
        Property::BorderBottomColor => set_first(&mut style.border_color.bottom, color(values)),
        Property::BorderLeftColor => set_first(&mut style.border_color.left, color(values)),
        Property::BorderTopLeftRadius => set_first(
            &mut style.border_radius.top_left,
            length(values, font_size, root, viewport),
        ),
        Property::BorderTopRightRadius => set_first(
            &mut style.border_radius.top_right,
            length(values, font_size, root, viewport),
        ),
        Property::BorderBottomRightRadius => set_first(
            &mut style.border_radius.bottom_right,
            length(values, font_size, root, viewport),
        ),
        Property::BorderBottomLeftRadius => set_first(
            &mut style.border_radius.bottom_left,
            length(values, font_size, root, viewport),
        ),
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
        Property::FlexBasis => set_first(
            &mut style.flex_basis,
            dimension(values, font_size, root, viewport),
        ),
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
        Property::RowGap => set_px(&mut style.row_gap, values, font_size, root, viewport),
        Property::ColumnGap => set_px(&mut style.column_gap, values, font_size, root, viewport),
        Property::Color => set_first(&mut style.color, color(values)),
        Property::BackgroundColor => set_first(&mut style.background_color, color(values)),
        Property::BackgroundImage => {
            style.background_image =
                crate::gradient::parse(values, font_size, root, viewport, style.color);
        }
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
        Property::LineHeight => {
            set_line_height(&mut style.line_height, values, font_size, viewport)
        }
        Property::TextAlign => set_text_align(&mut style.text_align, values),
        Property::TextDecorationLine => {
            let mut decoration = TextDecoration::NONE;
            for value in values {
                if let Value::Keyword(keyword) = value {
                    decoration = decoration.union(match keyword.to_ascii_lowercase().as_str() {
                        "underline" => TextDecoration::UNDERLINE,
                        "overline" => TextDecoration::OVERLINE,
                        "line-through" => TextDecoration::LINE_THROUGH,
                        _ => TextDecoration::NONE,
                    });
                }
            }
            style.text_decoration = decoration;
        }
        Property::WhiteSpace => set_white_space(&mut style.white_space, values),
        Property::LetterSpacing => {
            if keyword(values) == Some("normal") {
                style.letter_spacing = 0.0;
            } else {
                set_px(&mut style.letter_spacing, values, font_size, root, viewport);
            }
        }
        Property::Transform => {
            if keyword(values) == Some("none") {
                style.transform.clear();
            } else if let Some(list) = transform_list(values, font_size, root, viewport) {
                style.transform = list;
            }
        }
        Property::BoxShadow => {
            if keyword(values) == Some("none") {
                style.box_shadow.clear();
            } else if let Some(list) = box_shadows(values, font_size, root, viewport) {
                style.box_shadow = list;
            }
        }
        Property::TextTransform => {
            if let Some(k) = keyword(values) {
                style.text_transform = match k {
                    "uppercase" => TextTransform::Uppercase,
                    "lowercase" => TextTransform::Lowercase,
                    "capitalize" => TextTransform::Capitalize,
                    "none" => TextTransform::None,
                    _ => return,
                };
            }
        }
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
        Property::VerticalAlign => {
            style.vertical_align = match values {
                [Value::Keyword(k)] => match k.as_str() {
                    "baseline" => VerticalAlign::Baseline,
                    "sub" => VerticalAlign::Sub,
                    "super" => VerticalAlign::Super,
                    "text-top" => VerticalAlign::TextTop,
                    "text-bottom" => VerticalAlign::TextBottom,
                    "middle" => VerticalAlign::Middle,
                    "top" => VerticalAlign::Top,
                    "bottom" => VerticalAlign::Bottom,
                    _ => style.vertical_align,
                },
                [Value::Length(l)] => VerticalAlign::Length(to_px(l, font_size, root, viewport)),
                [Value::Percentage(percent)] => VerticalAlign::Percent(*percent),
                [Value::Number(number)] if *number == 0.0 => VerticalAlign::Length(0.0),
                _ => style.vertical_align,
            };
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

fn set_px(slot: &mut f32, values: &[Value], font_size: f32, root: f32, viewport: Viewport) {
    if let Some(Length::Px(px)) = length(values, font_size, root, viewport) {
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

fn set_line_height(slot: &mut Option<f32>, values: &[Value], font_size: f32, viewport: Viewport) {
    let value = if let Some((name, args)) = math_fn(values) {
        // A unitless `line-height` is a multiplier; a length is absolute.
        match calc::evaluate_function(name, args, font_size, font_size, viewport) {
            Some(CalcValue::Number(number)) => font_size * number,
            Some(value) => match calc::resolve(&value, font_size) {
                Some(resolved) => resolved,
                None => return,
            },
            None => return,
        }
    } else {
        match values {
            [Value::Number(number)] => font_size * *number,
            [Value::Length(l)] => to_px(l, font_size, font_size, viewport),
            [Value::Percentage(percent)] => font_size * percent / 100.0,
            [Value::Keyword(k)] if k == "normal" || k == "initial" => {
                *slot = None;
                return;
            }
            _ => return,
        }
    };
    *slot = Some(value);
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
        "-webkit-center" | "-moz-center" => TextAlign::WebkitCenter,
        _ => return,
    };
}

/// A `box-shadow` list, or `None` if any shadow is invalid. Each is
/// `inset? <x> <y> <blur>? <spread>? <color>?` in any order of the
/// keyword, lengths and colour.
fn box_shadows(
    values: &[Value],
    font_size: f32,
    root: f32,
    viewport: Viewport,
) -> Option<Vec<BoxShadow>> {
    let mut list = Vec::new();
    for group in values.split(|value| matches!(value, Value::Comma)) {
        let mut lengths = Vec::new();
        let mut shadow_color = None;
        let mut inset = false;
        for value in group {
            match value {
                Value::Keyword(k) if k.eq_ignore_ascii_case("inset") && !inset => inset = true,
                Value::Length(l) => lengths.push(to_px(l, font_size, root, viewport)),
                Value::Number(n) if *n == 0.0 => lengths.push(0.0),
                other if shadow_color.is_none() => {
                    shadow_color = Some(color(std::slice::from_ref(other))?)
                }
                _ => return None,
            }
        }
        let (x, y, blur, spread) = match lengths.as_slice() {
            [x, y] => (*x, *y, 0.0, 0.0),
            [x, y, blur] => (*x, *y, *blur, 0.0),
            [x, y, blur, spread] => (*x, *y, *blur, *spread),
            _ => return None,
        };
        if blur < 0.0 {
            return None;
        }
        list.push(BoxShadow {
            x,
            y,
            blur,
            spread,
            color: shadow_color,
            inset,
        });
    }
    Some(list)
}

/// A `transform` function list, or `None` if any function is invalid
/// (which drops the whole declaration).
fn transform_list(
    values: &[Value],
    font_size: f32,
    root: f32,
    viewport: Viewport,
) -> Option<Vec<Transform>> {
    let mut list = Vec::new();
    for value in values {
        let Value::Function { name, args } = value else {
            return None;
        };
        let args: Vec<&[Value]> = args
            .split(|arg| matches!(arg, Value::Comma))
            .filter(|arg| !arg.is_empty())
            .collect();
        let offset = |arg: &[Value]| match dimension(arg, font_size, root, viewport)? {
            Dimension::Auto => None,
            length => Some(length),
        };
        let number = |arg: &[Value]| match arg {
            [Value::Number(n)] => Some(*n),
            [Value::Percentage(p)] => Some(p / 100.0),
            _ => None,
        };
        let zero = Dimension::Px(0.0);
        list.push(
            match (name.to_ascii_lowercase().as_str(), args.as_slice()) {
                ("translate", [x]) => Transform::Translate(offset(x)?, zero),
                ("translate", [x, y]) => Transform::Translate(offset(x)?, offset(y)?),
                ("translatex", [x]) => Transform::Translate(offset(x)?, zero),
                ("translatey", [y]) => Transform::Translate(zero, offset(y)?),
                ("scale", [s]) => {
                    let s = number(s)?;
                    Transform::Scale(s, s)
                }
                ("scale", [x, y]) => Transform::Scale(number(x)?, number(y)?),
                ("scalex", [x]) => Transform::Scale(number(x)?, 1.0),
                ("scaley", [y]) => Transform::Scale(1.0, number(y)?),
                ("rotate", [a]) => Transform::Rotate(match a {
                    [Value::Length(length)] => crate::gradient::angle(length)?,
                    [Value::Number(n)] if *n == 0.0 => 0.0,
                    _ => return None,
                }),
                _ => return None,
            },
        );
    }
    Some(list)
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

/// Whether a property inherits by default.
fn is_inherited(property: &Property) -> bool {
    matches!(
        property,
        Property::Color
            | Property::FontSize
            | Property::FontWeight
            | Property::FontStyle
            | Property::FontFamily
            | Property::LineHeight
            | Property::TextAlign
            | Property::WhiteSpace
            | Property::LetterSpacing
            | Property::TextTransform
            | Property::Visibility
            | Property::Cursor
    )
}

/// Copy one property's value from `src` into `dst` (for `inherit`/`initial`).
fn copy_property(dst: &mut ComputedStyle, src: &ComputedStyle, property: &Property) {
    match property {
        Property::Display => dst.display = src.display,
        Property::BorderCollapse => dst.border_collapse = src.border_collapse,
        Property::Position => dst.position = src.position,
        Property::Float => dst.float = src.float,
        Property::Clear => dst.clear = src.clear,
        Property::GridTemplateColumns => {
            dst.grid_template_columns = src.grid_template_columns.clone()
        }
        Property::GridTemplateRows => dst.grid_template_rows = src.grid_template_rows.clone(),
        Property::GridAutoRows => dst.grid_auto_rows = src.grid_auto_rows.clone(),
        Property::GridAutoColumns => dst.grid_auto_columns = src.grid_auto_columns.clone(),
        Property::GridColumn => dst.grid_column = src.grid_column,
        Property::GridRow => dst.grid_row = src.grid_row,
        Property::Width => dst.width = src.width.clone(),
        Property::MinWidth => dst.min_width = src.min_width.clone(),
        Property::MaxWidth => dst.max_width = src.max_width.clone(),
        Property::Height => dst.height = src.height.clone(),
        Property::MinHeight => dst.min_height = src.min_height.clone(),
        Property::MaxHeight => dst.max_height = src.max_height.clone(),
        Property::Top => dst.inset.top = src.inset.top.clone(),
        Property::Right => dst.inset.right = src.inset.right.clone(),
        Property::Bottom => dst.inset.bottom = src.inset.bottom.clone(),
        Property::Left => dst.inset.left = src.inset.left.clone(),
        Property::MarginTop => dst.margin.top = src.margin.top.clone(),
        Property::MarginRight => dst.margin.right = src.margin.right.clone(),
        Property::MarginBottom => dst.margin.bottom = src.margin.bottom.clone(),
        Property::MarginLeft => dst.margin.left = src.margin.left.clone(),
        Property::PaddingTop => dst.padding.top = src.padding.top.clone(),
        Property::PaddingRight => dst.padding.right = src.padding.right.clone(),
        Property::PaddingBottom => dst.padding.bottom = src.padding.bottom.clone(),
        Property::PaddingLeft => dst.padding.left = src.padding.left.clone(),
        Property::BorderTopWidth => dst.border_width.top = src.border_width.top,
        Property::BorderRightWidth => dst.border_width.right = src.border_width.right,
        Property::BorderBottomWidth => dst.border_width.bottom = src.border_width.bottom,
        Property::BorderLeftWidth => dst.border_width.left = src.border_width.left,
        Property::BorderTopStyle => dst.border_style.top = src.border_style.top,
        Property::BorderRightStyle => dst.border_style.right = src.border_style.right,
        Property::BorderBottomStyle => dst.border_style.bottom = src.border_style.bottom,
        Property::BorderLeftStyle => dst.border_style.left = src.border_style.left,
        Property::BorderTopColor => dst.border_color.top = src.border_color.top,
        Property::BorderRightColor => dst.border_color.right = src.border_color.right,
        Property::BorderBottomColor => dst.border_color.bottom = src.border_color.bottom,
        Property::BorderLeftColor => dst.border_color.left = src.border_color.left,
        Property::BorderTopLeftRadius => {
            dst.border_radius.top_left = src.border_radius.top_left.clone()
        }
        Property::BorderTopRightRadius => {
            dst.border_radius.top_right = src.border_radius.top_right.clone()
        }
        Property::BorderBottomRightRadius => {
            dst.border_radius.bottom_right = src.border_radius.bottom_right.clone()
        }
        Property::BorderBottomLeftRadius => {
            dst.border_radius.bottom_left = src.border_radius.bottom_left.clone()
        }
        Property::BoxSizing => dst.box_sizing = src.box_sizing,
        Property::FlexDirection => dst.flex_direction = src.flex_direction,
        Property::FlexWrap => dst.flex_wrap = src.flex_wrap,
        Property::FlexGrow => dst.flex_grow = src.flex_grow,
        Property::FlexShrink => dst.flex_shrink = src.flex_shrink,
        Property::FlexBasis => dst.flex_basis = src.flex_basis.clone(),
        Property::JustifyContent => dst.justify_content = src.justify_content,
        Property::AlignItems => dst.align_items = src.align_items,
        Property::AlignSelf => dst.align_self = src.align_self,
        Property::RowGap => dst.row_gap = src.row_gap,
        Property::ColumnGap => dst.column_gap = src.column_gap,
        Property::Color => dst.color = src.color,
        Property::BackgroundColor => dst.background_color = src.background_color,
        Property::BackgroundImage => dst.background_image = src.background_image.clone(),
        Property::FontSize => dst.font_size = src.font_size,
        Property::FontWeight => dst.font_weight = src.font_weight,
        Property::FontStyle => dst.font_style = src.font_style,
        Property::FontFamily => dst.font_family = src.font_family.clone(),
        Property::LineHeight => dst.line_height = src.line_height,
        Property::TextAlign => dst.text_align = src.text_align,
        Property::TextDecorationLine => dst.text_decoration = src.text_decoration,
        Property::WhiteSpace => dst.white_space = src.white_space,
        Property::LetterSpacing => dst.letter_spacing = src.letter_spacing,
        Property::TextTransform => dst.text_transform = src.text_transform,
        Property::VerticalAlign => dst.vertical_align = src.vertical_align,
        Property::Transform => dst.transform = src.transform.clone(),
        Property::BoxShadow => dst.box_shadow = src.box_shadow.clone(),
        Property::Opacity => dst.opacity = src.opacity,
        Property::OverflowX => dst.overflow_x = src.overflow_x,
        Property::OverflowY => dst.overflow_y = src.overflow_y,
        Property::ZIndex => dst.z_index = src.z_index,
        Property::Visibility => dst.visibility = src.visibility,
        Property::Cursor => dst.cursor = src.cursor.clone(),
        _ => {}
    }
}
