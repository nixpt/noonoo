//! The property model: names, typed declarations, and shorthand expansion.
//!
//! A [`Declaration`] pairs a known [`Property`] with its parsed values. Longhand
//! properties keep their values as-is; shorthands (`margin`, `border`, `gap`, …)
//! are expanded into their longhands by [`parse_declaration`], which is what the
//! cascade consumes.

use crate::value::{Color, Value, parse_values};
use noonoo_parse::ParseResult;

/// A known CSS property (longhand), or one Noonoo does not model.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Property {
    /// `display`.
    Display,
    /// `position`.
    Position,
    /// `top`.
    Top,
    /// `right`.
    Right,
    /// `bottom`.
    Bottom,
    /// `left`.
    Left,
    /// `width`.
    Width,
    /// `height`.
    Height,
    /// `min-width`.
    MinWidth,
    /// `min-height`.
    MinHeight,
    /// `max-width`.
    MaxWidth,
    /// `max-height`.
    MaxHeight,
    /// `margin-top`.
    MarginTop,
    /// `margin-right`.
    MarginRight,
    /// `margin-bottom`.
    MarginBottom,
    /// `margin-left`.
    MarginLeft,
    /// `padding-top`.
    PaddingTop,
    /// `padding-right`.
    PaddingRight,
    /// `padding-bottom`.
    PaddingBottom,
    /// `padding-left`.
    PaddingLeft,
    /// `border-top-width`.
    BorderTopWidth,
    /// `border-right-width`.
    BorderRightWidth,
    /// `border-bottom-width`.
    BorderBottomWidth,
    /// `border-left-width`.
    BorderLeftWidth,
    /// `border-top-style`.
    BorderTopStyle,
    /// `border-right-style`.
    BorderRightStyle,
    /// `border-bottom-style`.
    BorderBottomStyle,
    /// `border-left-style`.
    BorderLeftStyle,
    /// `border-top-color`.
    BorderTopColor,
    /// `border-right-color`.
    BorderRightColor,
    /// `border-bottom-color`.
    BorderBottomColor,
    /// `border-left-color`.
    BorderLeftColor,
    /// `border-top-left-radius`.
    BorderTopLeftRadius,
    /// `border-top-right-radius`.
    BorderTopRightRadius,
    /// `border-bottom-right-radius`.
    BorderBottomRightRadius,
    /// `border-bottom-left-radius`.
    BorderBottomLeftRadius,
    /// `box-sizing`.
    BoxSizing,
    /// `flex-direction`.
    FlexDirection,
    /// `flex-wrap`.
    FlexWrap,
    /// `flex-grow`.
    FlexGrow,
    /// `flex-shrink`.
    FlexShrink,
    /// `flex-basis`.
    FlexBasis,
    /// `justify-content`.
    JustifyContent,
    /// `align-items`.
    AlignItems,
    /// `align-self`.
    AlignSelf,
    /// `align-content`.
    AlignContent,
    /// `row-gap`.
    RowGap,
    /// `column-gap`.
    ColumnGap,
    /// `color`.
    Color,
    /// `background-color`.
    BackgroundColor,
    /// `font-size`.
    FontSize,
    /// `font-weight`.
    FontWeight,
    /// `font-style`.
    FontStyle,
    /// `font-family`.
    FontFamily,
    /// `line-height`.
    LineHeight,
    /// `text-align`.
    TextAlign,
    /// `text-decoration-line`.
    TextDecorationLine,
    /// `white-space`.
    WhiteSpace,
    /// `opacity`.
    Opacity,
    /// `overflow-x`.
    OverflowX,
    /// `overflow-y`.
    OverflowY,
    /// `z-index`.
    ZIndex,
    /// `visibility`.
    Visibility,
    /// `cursor`.
    Cursor,
    /// A custom property (`--foo`).
    Custom(String),
    /// A property Noonoo parses but does not model.
    Unknown(String),
}

impl Property {
    /// Map a property name to a [`Property`].
    pub fn parse(name: &str) -> Property {
        let name = name.trim().to_ascii_lowercase();
        if let Some(custom) = name.strip_prefix("--") {
            return Property::Custom(custom.to_owned());
        }
        match name.as_str() {
            "display" => Self::Display,
            "position" => Self::Position,
            "top" => Self::Top,
            "right" => Self::Right,
            "bottom" => Self::Bottom,
            "left" => Self::Left,
            "width" => Self::Width,
            "height" => Self::Height,
            "min-width" => Self::MinWidth,
            "min-height" => Self::MinHeight,
            "max-width" => Self::MaxWidth,
            "max-height" => Self::MaxHeight,
            "margin-top" => Self::MarginTop,
            "margin-right" => Self::MarginRight,
            "margin-bottom" => Self::MarginBottom,
            "margin-left" => Self::MarginLeft,
            "padding-top" => Self::PaddingTop,
            "padding-right" => Self::PaddingRight,
            "padding-bottom" => Self::PaddingBottom,
            "padding-left" => Self::PaddingLeft,
            "border-top-width" => Self::BorderTopWidth,
            "border-right-width" => Self::BorderRightWidth,
            "border-bottom-width" => Self::BorderBottomWidth,
            "border-left-width" => Self::BorderLeftWidth,
            "border-top-style" => Self::BorderTopStyle,
            "border-right-style" => Self::BorderRightStyle,
            "border-bottom-style" => Self::BorderBottomStyle,
            "border-left-style" => Self::BorderLeftStyle,
            "border-top-color" => Self::BorderTopColor,
            "border-right-color" => Self::BorderRightColor,
            "border-bottom-color" => Self::BorderBottomColor,
            "border-left-color" => Self::BorderLeftColor,
            "border-top-left-radius" => Self::BorderTopLeftRadius,
            "border-top-right-radius" => Self::BorderTopRightRadius,
            "border-bottom-right-radius" => Self::BorderBottomRightRadius,
            "border-bottom-left-radius" => Self::BorderBottomLeftRadius,
            "box-sizing" => Self::BoxSizing,
            "flex-direction" => Self::FlexDirection,
            "flex-wrap" => Self::FlexWrap,
            "flex-grow" => Self::FlexGrow,
            "flex-shrink" => Self::FlexShrink,
            "flex-basis" => Self::FlexBasis,
            "justify-content" => Self::JustifyContent,
            "align-items" => Self::AlignItems,
            "align-self" => Self::AlignSelf,
            "align-content" => Self::AlignContent,
            "row-gap" => Self::RowGap,
            "column-gap" | "grid-column-gap" => Self::ColumnGap,
            "color" => Self::Color,
            "background-color" => Self::BackgroundColor,
            "font-size" => Self::FontSize,
            "font-weight" => Self::FontWeight,
            "font-style" => Self::FontStyle,
            "font-family" => Self::FontFamily,
            "line-height" => Self::LineHeight,
            "text-align" => Self::TextAlign,
            "text-decoration-line" => Self::TextDecorationLine,
            "white-space" => Self::WhiteSpace,
            "opacity" => Self::Opacity,
            "overflow-x" => Self::OverflowX,
            "overflow-y" => Self::OverflowY,
            "z-index" => Self::ZIndex,
            "visibility" => Self::Visibility,
            "cursor" => Self::Cursor,
            _ => Self::Unknown(name),
        }
    }

    /// The canonical property name.
    pub fn name(&self) -> String {
        match self {
            Self::Custom(name) => format!("--{name}"),
            Self::Unknown(name) => name.clone(),
            other => other.longhand_name().to_owned(),
        }
    }

    fn longhand_name(&self) -> &'static str {
        match self {
            Self::Display => "display",
            Self::Position => "position",
            Self::Top => "top",
            Self::Right => "right",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Width => "width",
            Self::Height => "height",
            Self::MinWidth => "min-width",
            Self::MinHeight => "min-height",
            Self::MaxWidth => "max-width",
            Self::MaxHeight => "max-height",
            Self::MarginTop => "margin-top",
            Self::MarginRight => "margin-right",
            Self::MarginBottom => "margin-bottom",
            Self::MarginLeft => "margin-left",
            Self::PaddingTop => "padding-top",
            Self::PaddingRight => "padding-right",
            Self::PaddingBottom => "padding-bottom",
            Self::PaddingLeft => "padding-left",
            Self::BorderTopWidth => "border-top-width",
            Self::BorderRightWidth => "border-right-width",
            Self::BorderBottomWidth => "border-bottom-width",
            Self::BorderLeftWidth => "border-left-width",
            Self::BorderTopStyle => "border-top-style",
            Self::BorderRightStyle => "border-right-style",
            Self::BorderBottomStyle => "border-bottom-style",
            Self::BorderLeftStyle => "border-left-style",
            Self::BorderTopColor => "border-top-color",
            Self::BorderRightColor => "border-right-color",
            Self::BorderBottomColor => "border-bottom-color",
            Self::BorderLeftColor => "border-left-color",
            Self::BorderTopLeftRadius => "border-top-left-radius",
            Self::BorderTopRightRadius => "border-top-right-radius",
            Self::BorderBottomRightRadius => "border-bottom-right-radius",
            Self::BorderBottomLeftRadius => "border-bottom-left-radius",
            Self::BoxSizing => "box-sizing",
            Self::FlexDirection => "flex-direction",
            Self::FlexWrap => "flex-wrap",
            Self::FlexGrow => "flex-grow",
            Self::FlexShrink => "flex-shrink",
            Self::FlexBasis => "flex-basis",
            Self::JustifyContent => "justify-content",
            Self::AlignItems => "align-items",
            Self::AlignSelf => "align-self",
            Self::AlignContent => "align-content",
            Self::RowGap => "row-gap",
            Self::ColumnGap => "column-gap",
            Self::Color => "color",
            Self::BackgroundColor => "background-color",
            Self::FontSize => "font-size",
            Self::FontWeight => "font-weight",
            Self::FontStyle => "font-style",
            Self::FontFamily => "font-family",
            Self::LineHeight => "line-height",
            Self::TextAlign => "text-align",
            Self::TextDecorationLine => "text-decoration-line",
            Self::WhiteSpace => "white-space",
            Self::Opacity => "opacity",
            Self::OverflowX => "overflow-x",
            Self::OverflowY => "overflow-y",
            Self::ZIndex => "z-index",
            Self::Visibility => "visibility",
            Self::Cursor => "cursor",
            Self::Custom(_) | Self::Unknown(_) => unreachable!("handled by name()"),
        }
    }
}

/// A typed declaration: a longhand property, its values, and `!important`.
#[derive(Clone, PartialEq, Debug)]
pub struct Declaration {
    /// The property.
    pub property: Property,
    /// Its component values.
    pub values: Vec<Value>,
    /// Whether it was flagged `!important`.
    pub important: bool,
}

/// The four physical sides, in CSS order (top, right, bottom, left).
fn sides(top: Property, right: Property, bottom: Property, left: Property) -> [Property; 4] {
    [top, right, bottom, left]
}

/// Parse a declaration and expand any shorthand into longhands.
pub fn parse_declaration(
    name: &str,
    value: &str,
    important: bool,
) -> ParseResult<Vec<Declaration>> {
    let values = parse_values(value)?;
    let lowered = name.trim().to_ascii_lowercase();

    let expanded = match lowered.as_str() {
        "margin" => box_shorthand(
            values.clone(),
            important,
            sides(
                Property::MarginTop,
                Property::MarginRight,
                Property::MarginBottom,
                Property::MarginLeft,
            ),
        ),
        "padding" => box_shorthand(
            values.clone(),
            important,
            sides(
                Property::PaddingTop,
                Property::PaddingRight,
                Property::PaddingBottom,
                Property::PaddingLeft,
            ),
        ),
        "inset" => box_shorthand(
            values.clone(),
            important,
            sides(
                Property::Top,
                Property::Right,
                Property::Bottom,
                Property::Left,
            ),
        ),
        "border-width" => box_shorthand(
            values.clone(),
            important,
            sides(
                Property::BorderTopWidth,
                Property::BorderRightWidth,
                Property::BorderBottomWidth,
                Property::BorderLeftWidth,
            ),
        ),
        "border-style" => box_shorthand(
            values.clone(),
            important,
            sides(
                Property::BorderTopStyle,
                Property::BorderRightStyle,
                Property::BorderBottomStyle,
                Property::BorderLeftStyle,
            ),
        ),
        "border-color" => box_shorthand(
            values.clone(),
            important,
            sides(
                Property::BorderTopColor,
                Property::BorderRightColor,
                Property::BorderBottomColor,
                Property::BorderLeftColor,
            ),
        ),
        // Elliptical syntax (`a / b`) is dropped to its horizontal radii.
        "border-radius" => box_shorthand(
            values
                .iter()
                .take_while(|v| !matches!(v, Value::Slash))
                .cloned()
                .collect(),
            important,
            sides(
                Property::BorderTopLeftRadius,
                Property::BorderTopRightRadius,
                Property::BorderBottomRightRadius,
                Property::BorderBottomLeftRadius,
            ),
        ),
        "gap" => pair_shorthand(
            values.clone(),
            important,
            Property::RowGap,
            Property::ColumnGap,
        ),
        "overflow" => pair_shorthand(
            values.clone(),
            important,
            Property::OverflowX,
            Property::OverflowY,
        ),
        "flex" => Some(expand_flex(values.clone(), important)),
        "flex-flow" => Some(expand_flex_flow(values.clone(), important)),
        "border" => Some(expand_border(values.clone(), important)),
        "border-top" => Some(expand_border_side(values.clone(), important, 0)),
        "border-right" => Some(expand_border_side(values.clone(), important, 1)),
        "border-bottom" => Some(expand_border_side(values.clone(), important, 2)),
        "border-left" => Some(expand_border_side(values.clone(), important, 3)),
        "background" => Some(expand_background(values.clone(), important)),
        "font" => Some(expand_font(values.clone(), important)),
        _ => None,
    };

    Ok(expanded.unwrap_or_else(|| vec![decl(Property::parse(name), values, important)]))
}

fn decl(property: Property, values: Vec<Value>, important: bool) -> Declaration {
    Declaration {
        property,
        values,
        important,
    }
}

/// Expand a 1–4 value box shorthand into four longhands.
fn box_shorthand(
    values: Vec<Value>,
    important: bool,
    properties: [Property; 4],
) -> Option<Vec<Declaration>> {
    let [top, right, bottom, left] = four(&values)?;
    Some(vec![
        decl(properties[0].clone(), vec![top], important),
        decl(properties[1].clone(), vec![right], important),
        decl(properties[2].clone(), vec![bottom], important),
        decl(properties[3].clone(), vec![left], important),
    ])
}

/// Expand a 1–2 value shorthand into two longhands.
fn pair_shorthand(
    values: Vec<Value>,
    important: bool,
    first: Property,
    second: Property,
) -> Option<Vec<Declaration>> {
    match values.as_slice() {
        [both] => Some(vec![
            decl(first.clone(), vec![both.clone()], important),
            decl(second.clone(), vec![both.clone()], important),
        ]),
        [a, b] => Some(vec![
            decl(first, vec![a.clone()], important),
            decl(second, vec![b.clone()], important),
        ]),
        _ => None,
    }
}

/// The CSS 1–4 value shorthand pattern.
fn four(values: &[Value]) -> Option<[Value; 4]> {
    match values {
        [a] => Some([a.clone(), a.clone(), a.clone(), a.clone()]),
        [a, b] => Some([a.clone(), b.clone(), a.clone(), b.clone()]),
        [a, b, c] => Some([a.clone(), b.clone(), c.clone(), b.clone()]),
        [a, b, c, d] => Some([a.clone(), b.clone(), c.clone(), d.clone()]),
        _ => None,
    }
}

fn expand_flex_flow(values: Vec<Value>, important: bool) -> Vec<Declaration> {
    let mut out = Vec::new();
    for value in values {
        if let Value::Keyword(keyword) = &value {
            match keyword.as_str() {
                "row" | "row-reverse" | "column" | "column-reverse" => {
                    out.push(decl(
                        Property::FlexDirection,
                        vec![value.clone()],
                        important,
                    ));
                }
                "nowrap" | "wrap" | "wrap-reverse" => {
                    out.push(decl(Property::FlexWrap, vec![value.clone()], important));
                }
                _ => {}
            }
        }
    }
    out
}

fn expand_flex(values: Vec<Value>, important: bool) -> Vec<Declaration> {
    match values.as_slice() {
        [only] => vec![decl(Property::FlexGrow, vec![only.clone()], important)],
        [grow, shrink] => vec![
            decl(Property::FlexGrow, vec![grow.clone()], important),
            decl(Property::FlexShrink, vec![shrink.clone()], important),
        ],
        [grow, shrink, basis] => vec![
            decl(Property::FlexGrow, vec![grow.clone()], important),
            decl(Property::FlexShrink, vec![shrink.clone()], important),
            decl(Property::FlexBasis, vec![basis.clone()], important),
        ],
        _ => vec![decl(Property::FlexGrow, values, important)],
    }
}

/// Classify the `<width> <style> <color>` tokens of a border shorthand.
fn classify_border(values: Vec<Value>) -> (Option<Value>, Option<Value>, Option<Value>) {
    let mut width = None;
    let mut style = None;
    let mut color = None;
    for value in values {
        if value.as_length().is_some() {
            width = Some(value);
        } else if let Some(name) = value.as_keyword() {
            if Color::named(name).is_some() {
                color = Some(value);
            } else if is_border_style(name) {
                style = Some(value);
            } else if is_border_width_keyword(name) {
                width = Some(value);
            } else {
                style = Some(value);
            }
        } else if value.as_color().is_some() {
            color = Some(value);
        }
    }
    (width, style, color)
}

/// `border: <width> <style> <color>` → width/style/color on all four sides.
fn expand_border(values: Vec<Value>, important: bool) -> Vec<Declaration> {
    let (width, style, color) = classify_border(values);

    let mut out = Vec::new();
    for (value, properties) in [
        (
            width,
            sides(
                Property::BorderTopWidth,
                Property::BorderRightWidth,
                Property::BorderBottomWidth,
                Property::BorderLeftWidth,
            ),
        ),
        (
            style,
            sides(
                Property::BorderTopStyle,
                Property::BorderRightStyle,
                Property::BorderBottomStyle,
                Property::BorderLeftStyle,
            ),
        ),
        (
            color,
            sides(
                Property::BorderTopColor,
                Property::BorderRightColor,
                Property::BorderBottomColor,
                Property::BorderLeftColor,
            ),
        ),
    ] {
        if let Some(value) = value
            && let Some(decls) = box_shorthand(vec![value], important, properties)
        {
            out.extend(decls);
        }
    }
    out
}

/// `border-<side>: <width> <style> <color>` → that one side's three longhands.
fn expand_border_side(values: Vec<Value>, important: bool, index: usize) -> Vec<Declaration> {
    let (width, style, color) = classify_border(values);
    let widths = sides(
        Property::BorderTopWidth,
        Property::BorderRightWidth,
        Property::BorderBottomWidth,
        Property::BorderLeftWidth,
    );
    let styles = sides(
        Property::BorderTopStyle,
        Property::BorderRightStyle,
        Property::BorderBottomStyle,
        Property::BorderLeftStyle,
    );
    let colors = sides(
        Property::BorderTopColor,
        Property::BorderRightColor,
        Property::BorderBottomColor,
        Property::BorderLeftColor,
    );

    let mut out = Vec::new();
    for (value, property) in [
        (width, widths[index].clone()),
        (style, styles[index].clone()),
        (color, colors[index].clone()),
    ] {
        if let Some(value) = value {
            out.push(decl(property, vec![value], important));
        }
    }
    out
}

/// `background: …` → `background-color`, the only part Noonoo models.
///
/// The shorthand resets `background-color` to the colour it names, or to
/// `transparent` when it names none.
fn expand_background(values: Vec<Value>, important: bool) -> Vec<Declaration> {
    let color = values
        .iter()
        .find_map(|value| {
            if value.as_color().is_some() {
                Some(value.clone())
            } else {
                value.as_keyword().and_then(|name| {
                    (name.eq_ignore_ascii_case("transparent") || Color::named(name).is_some())
                        .then(|| value.clone())
                })
            }
        })
        .unwrap_or_else(|| Value::Keyword("transparent".into()));
    vec![decl(Property::BackgroundColor, vec![color], important)]
}

/// `font: [style] [weight] <size>[/<line-height>] <family>` → the longhands
/// Noonoo models.
fn expand_font(values: Vec<Value>, important: bool) -> Vec<Declaration> {
    // A system-font keyword cannot be expanded without the system's metrics.
    if let [Value::Keyword(keyword)] = values.as_slice()
        && matches!(
            keyword.to_ascii_lowercase().as_str(),
            "caption" | "icon" | "menu" | "message-box" | "small-caption" | "status-bar"
        )
    {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut i = 0;

    // Leading style/weight tokens, in any order.
    while i < values.len() {
        match &values[i] {
            Value::Keyword(keyword)
                if matches!(keyword.to_ascii_lowercase().as_str(), "italic" | "oblique") =>
            {
                out.push(decl(
                    Property::FontStyle,
                    vec![values[i].clone()],
                    important,
                ));
                i += 1;
            }
            Value::Keyword(keyword)
                if matches!(
                    keyword.to_ascii_lowercase().as_str(),
                    "bold" | "bolder" | "lighter"
                ) =>
            {
                out.push(decl(
                    Property::FontWeight,
                    vec![values[i].clone()],
                    important,
                ));
                i += 1;
            }
            Value::Number(number) if (100.0..=900.0).contains(number) => {
                out.push(decl(
                    Property::FontWeight,
                    vec![values[i].clone()],
                    important,
                ));
                i += 1;
            }
            // `normal`, `small-caps`, and stretch keywords are tolerated but
            // dropped: Noonoo models neither.
            Value::Keyword(keyword)
                if matches!(
                    keyword.to_ascii_lowercase().as_str(),
                    "normal" | "small-caps"
                ) =>
            {
                i += 1;
            }
            _ => break,
        }
    }

    // The size, and an optional `/ line-height`.
    if i < values.len() {
        out.push(decl(Property::FontSize, vec![values[i].clone()], important));
        i += 1;
        if matches!(values.get(i), Some(Value::Slash)) {
            i += 1;
            if i < values.len() {
                out.push(decl(
                    Property::LineHeight,
                    vec![values[i].clone()],
                    important,
                ));
                i += 1;
            }
        }
    }

    // Everything left is the family list.
    if i < values.len() {
        out.push(decl(Property::FontFamily, values[i..].to_vec(), important));
    }

    out
}

fn is_border_style(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "none"
            | "hidden"
            | "solid"
            | "dashed"
            | "dotted"
            | "double"
            | "groove"
            | "ridge"
            | "inset"
            | "outset"
    )
}

fn is_border_width_keyword(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "thin" | "medium" | "thick"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{Color, Length, LengthUnit};

    fn props(decls: &[Declaration]) -> Vec<String> {
        decls.iter().map(|d| d.property.name()).collect()
    }

    #[test]
    fn longhand_stays_single() {
        let decls = parse_declaration("display", "flex", false).unwrap();
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].property, Property::Display);
        assert_eq!(decls[0].values, vec![Value::Keyword("flex".into())]);
    }

    #[test]
    fn margin_expands_to_four() {
        let decls = parse_declaration("margin", "0 auto", false).unwrap();
        assert_eq!(
            props(&decls),
            vec!["margin-top", "margin-right", "margin-bottom", "margin-left"]
        );
        assert_eq!(decls[0].values, vec![Value::Number(0.0)]);
        assert_eq!(decls[1].values, vec![Value::Keyword("auto".into())]);
        assert_eq!(decls[3].values, vec![Value::Keyword("auto".into())]);
    }

    #[test]
    fn padding_four_values() {
        let decls = parse_declaration("padding", "1px 2px 3px 4px", false).unwrap();
        assert_eq!(decls.len(), 4);
        assert_eq!(
            decls[2].values,
            vec![Value::Length(Length::new(3.0, LengthUnit::Px))]
        );
    }

    #[test]
    fn gap_expands_and_carries_important() {
        let decls = parse_declaration("gap", "1px 2px", true).unwrap();
        assert_eq!(props(&decls), vec!["row-gap", "column-gap"]);
        assert!(decls[0].important);
    }

    #[test]
    fn flex_flow_expands() {
        let decls = parse_declaration("flex-flow", "column wrap", false).unwrap();
        assert_eq!(props(&decls), vec!["flex-direction", "flex-wrap"]);
    }

    #[test]
    fn border_shorthand_sets_all_sides() {
        let decls = parse_declaration("border", "1px solid red", false).unwrap();
        assert_eq!(decls.len(), 12);
        assert!(decls.iter().any(|d| d.property == Property::BorderTopWidth));
        assert!(
            decls
                .iter()
                .any(|d| d.property == Property::BorderLeftStyle)
        );
        assert!(
            decls
                .iter()
                .any(|d| d.property == Property::BorderBottomColor)
        );
        let top_style = decls
            .iter()
            .find(|d| d.property == Property::BorderTopStyle)
            .unwrap();
        assert_eq!(top_style.values, vec![Value::Keyword("solid".into())]);
    }

    #[test]
    fn border_radius_expands() {
        let decls = parse_declaration("border-radius", "4px", false).unwrap();
        assert_eq!(
            props(&decls),
            vec![
                "border-top-left-radius",
                "border-top-right-radius",
                "border-bottom-right-radius",
                "border-bottom-left-radius"
            ]
        );
    }

    #[test]
    fn color_custom_and_unknown() {
        let decls = parse_declaration("color", "#ff0000", false).unwrap();
        assert_eq!(decls[0].values, vec![Value::Color(Color::rgb(255, 0, 0))]);

        let custom = parse_declaration("--brand", "#ff0000", false).unwrap();
        assert_eq!(custom[0].property, Property::Custom("brand".into()));

        let unknown = parse_declaration("grid-template-columns", "1fr 1fr", false).unwrap();
        assert_eq!(
            unknown[0].property,
            Property::Unknown("grid-template-columns".into())
        );
    }

    #[test]
    fn background_extracts_the_colour() {
        let decls =
            parse_declaration("background", "#eeeeee url(bg.png) no-repeat", false).unwrap();
        assert_eq!(props(&decls), vec!["background-color"]);
        assert_eq!(
            decls[0].values,
            vec![Value::Color(Color::rgb(0xee, 0xee, 0xee))]
        );

        // A shorthand that names no colour resets to transparent.
        let none = parse_declaration("background", "none", false).unwrap();
        assert_eq!(none[0].values, vec![Value::Keyword("transparent".into())]);
    }

    #[test]
    fn font_shorthand_expands() {
        let decls =
            parse_declaration("font", r#"italic bold 16px/1.5 "Helvetica", serif"#, false).unwrap();
        assert_eq!(
            props(&decls),
            vec![
                "font-style",
                "font-weight",
                "font-size",
                "line-height",
                "font-family"
            ]
        );
        assert_eq!(decls[0].values, vec![Value::Keyword("italic".into())]);
        assert_eq!(decls[1].values, vec![Value::Keyword("bold".into())]);
        assert_eq!(
            decls[2].values,
            vec![Value::Length(Length::new(16.0, LengthUnit::Px))]
        );
        assert_eq!(decls[3].values, vec![Value::Number(1.5)]);
        assert!(decls[4].values.len() >= 2, "the family list survives");
    }

    #[test]
    fn a_system_font_keyword_is_left_alone() {
        let decls = parse_declaration("font", "caption", false).unwrap();
        assert!(decls.is_empty());
    }

    #[test]
    fn border_side_shorthand_sets_one_side() {
        let decls = parse_declaration("border-top", "2px dashed blue", false).unwrap();
        assert_eq!(
            props(&decls),
            vec!["border-top-width", "border-top-style", "border-top-color"]
        );
        assert_eq!(
            decls[0].values,
            vec![Value::Length(Length::new(2.0, LengthUnit::Px))]
        );
        assert_eq!(decls[1].values, vec![Value::Keyword("dashed".into())]);
        assert_eq!(decls[2].values, vec![Value::Keyword("blue".into())]);
    }
}
