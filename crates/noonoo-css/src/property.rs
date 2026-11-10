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
    /// `float`.
    Float,
    /// `clear`.
    Clear,
    /// `grid-template-columns`.
    GridTemplateColumns,
    /// `grid-template-rows`.
    GridTemplateRows,
    /// `grid-auto-rows`.
    GridAutoRows,
    /// `grid-auto-columns`.
    GridAutoColumns,
    /// `grid-column`.
    GridColumn,
    /// `grid-row`.
    GridRow,
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
    /// `appearance`, or its `-webkit-` or `-moz-` alias.
    Appearance,
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
    /// `background-image`.
    BackgroundImage,
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
    /// `border-collapse`.
    BorderCollapse,
    /// `white-space`.
    WhiteSpace,
    /// `letter-spacing`.
    LetterSpacing,
    /// `text-transform`.
    TextTransform,
    /// `vertical-align`.
    VerticalAlign,
    /// `border-spacing`.
    BorderSpacing,
    /// `transform`.
    Transform,
    /// `box-shadow`.
    BoxShadow,
    /// `content` (meaningful on `::before`/`::after`).
    Content,
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
            "float" => Self::Float,
            "grid-template-columns" => Self::GridTemplateColumns,
            "grid-template-rows" => Self::GridTemplateRows,
            "grid-auto-rows" => Self::GridAutoRows,
            "grid-auto-columns" => Self::GridAutoColumns,
            "grid-column" => Self::GridColumn,
            "grid-row" => Self::GridRow,
            "clear" => Self::Clear,
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
            "appearance" | "-webkit-appearance" | "-moz-appearance" => Self::Appearance,
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
            "background-image" => Self::BackgroundImage,
            "font-size" => Self::FontSize,
            "font-weight" => Self::FontWeight,
            "font-style" => Self::FontStyle,
            "font-family" => Self::FontFamily,
            "line-height" => Self::LineHeight,
            "text-align" => Self::TextAlign,
            "text-decoration-line" => Self::TextDecorationLine,
            "border-collapse" => Self::BorderCollapse,
            "white-space" => Self::WhiteSpace,
            "letter-spacing" => Self::LetterSpacing,
            "text-transform" => Self::TextTransform,
            "vertical-align" => Self::VerticalAlign,
            "border-spacing" => Self::BorderSpacing,
            "transform" => Self::Transform,
            "box-shadow" => Self::BoxShadow,
            "content" => Self::Content,
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
            Self::Float => "float",
            Self::GridTemplateColumns => "grid-template-columns",
            Self::GridTemplateRows => "grid-template-rows",
            Self::GridAutoRows => "grid-auto-rows",
            Self::GridAutoColumns => "grid-auto-columns",
            Self::GridColumn => "grid-column",
            Self::GridRow => "grid-row",
            Self::Clear => "clear",
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
            Self::Appearance => "appearance",
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
            Self::BackgroundImage => "background-image",
            Self::FontSize => "font-size",
            Self::FontWeight => "font-weight",
            Self::FontStyle => "font-style",
            Self::FontFamily => "font-family",
            Self::LineHeight => "line-height",
            Self::TextAlign => "text-align",
            Self::TextDecorationLine => "text-decoration-line",
            Self::BorderCollapse => "border-collapse",
            Self::WhiteSpace => "white-space",
            Self::LetterSpacing => "letter-spacing",
            Self::TextTransform => "text-transform",
            Self::VerticalAlign => "vertical-align",
            Self::BorderSpacing => "border-spacing",
            Self::Transform => "transform",
            Self::BoxShadow => "box-shadow",
            Self::Content => "content",
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

/// The function name of a *pending-substitution* value: the value every
/// longhand of a shorthand gets when the shorthand holds `var()`.
///
/// A shorthand can't be split into longhands until its `var()`s are
/// substituted: `border: var(--b)` doesn't say which part is the color. So,
/// as CSS specifies, each longhand carries the shorthand's name and raw
/// value, `-noonoo-pending-shorthand(border var(--b))`, and still cascades on
/// its own. At computed-value time the style layer substitutes the raw value,
/// calls [`expand_declaration`], and takes its own part. See
/// [`pending_shorthand`].
pub const PENDING_SHORTHAND: &str = "-noonoo-pending-shorthand";

/// The shorthand name and raw values, if `values` is a pending-substitution
/// value (see [`PENDING_SHORTHAND`]).
pub fn pending_shorthand(values: &[Value]) -> Option<(&str, &[Value])> {
    match values {
        [Value::Function { name, args }] if name == PENDING_SHORTHAND => match args.split_first() {
            Some((Value::Keyword(shorthand), raw)) => Some((shorthand.as_str(), raw)),
            _ => None,
        },
        _ => None,
    }
}

/// Whether any value, at any depth, is a `var()` reference.
fn contains_var(values: &[Value]) -> bool {
    values.iter().any(|value| match value {
        Value::Function { name, args } => name.eq_ignore_ascii_case("var") || contains_var(args),
        _ => false,
    })
}

/// A value for each shorthand that sets every longhand it can, used to
/// find which longhands a pending-substitution shorthand covers. A test
/// checks each one expands.
const SHORTHAND_SAMPLES: &[(&str, &str)] = &[
    ("margin", "0"),
    ("padding", "0"),
    ("inset", "0"),
    ("border-width", "0"),
    ("border-style", "none"),
    ("border-color", "black"),
    ("border-radius", "0"),
    ("gap", "0"),
    ("overflow", "visible"),
    ("flex", "1 1 0"),
    ("flex-flow", "row nowrap"),
    ("border", "1px solid black"),
    ("border-top", "1px solid black"),
    ("border-right", "1px solid black"),
    ("border-bottom", "1px solid black"),
    ("border-left", "1px solid black"),
    ("background", "black"),
    ("font", "italic bold 16px/1.2 serif"),
    ("text-decoration", "underline solid black"),
];

/// The longhands `shorthand` (lowercased) sets, or `None` if it is not a
/// shorthand Noonoo expands.
pub(crate) fn shorthand_longhands(shorthand: &str) -> Option<Vec<Property>> {
    let (_, sample) = SHORTHAND_SAMPLES
        .iter()
        .find(|(name, _)| *name == shorthand)?;
    let values = parse_values(sample).ok()?;
    Some(
        expand_declaration(shorthand, values, false)
            .into_iter()
            .map(|declaration| declaration.property)
            .collect(),
    )
}

/// Parse a declaration and expand any shorthand into longhands.
///
/// A shorthand whose value holds `var()` expands to pending-substitution
/// longhands instead (see [`PENDING_SHORTHAND`]).
pub fn parse_declaration(
    name: &str,
    value: &str,
    important: bool,
) -> ParseResult<Vec<Declaration>> {
    let values = parse_values(value)?;
    let lowered = name.trim().to_ascii_lowercase();
    // An invalid declaration is dropped here, so it never competes in the
    // cascade: the next one down applies instead.
    if values.is_empty() && !lowered.starts_with("--") {
        return Err(noonoo_parse::ParseError::new(
            0,
            "a declaration needs a value",
        ));
    }
    if lowered == "display" && !contains_var(&values) && !is_display(&values) {
        return Err(noonoo_parse::ParseError::new(
            0,
            "unsupported display value",
        ));
    }
    if contains_var(&values)
        && let Some(longhands) = shorthand_longhands(&lowered)
    {
        let mut args = vec![Value::Keyword(lowered)];
        args.extend(values);
        let pending = Value::Function {
            name: PENDING_SHORTHAND.into(),
            args,
        };
        return Ok(longhands
            .into_iter()
            .map(|property| decl(property, vec![pending.clone()], important))
            .collect());
    }
    Ok(expand_declaration(name, values, important))
}

/// The `display` values layout knows, plus the CSS-wide keywords the
/// cascade resolves. Anything else (`flow-root`, `contents`, a typo) is
/// dropped rather than letting it win and fall back to `inline`.
fn is_display(values: &[Value]) -> bool {
    let [Value::Keyword(keyword)] = values else {
        return false;
    };
    matches!(
        keyword.to_ascii_lowercase().as_str(),
        "none"
            | "block"
            | "flow-root"
            | "inline"
            | "inline-block"
            | "flex"
            | "inline-flex"
            | "grid"
            | "inline-grid"
            | "list-item"
            | "table"
            | "table-row-group"
            | "table-row"
            | "table-cell"
            | "table-column-group"
            | "table-column"
            | "inherit"
            | "initial"
            | "unset"
    )
}

/// Expand already-parsed values for `name`: a shorthand into its longhands,
/// anything else into one declaration. The values must hold no `var()`;
/// this is the second half of a pending-substitution shorthand.
pub fn expand_declaration(name: &str, values: Vec<Value>, important: bool) -> Vec<Declaration> {
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
        "text-decoration" => Some(expand_text_decoration(values.clone(), important)),
        _ => None,
    };

    expanded.unwrap_or_else(|| vec![decl(Property::parse(name), values, important)])
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

/// `flex` (CSS Flexbox §7.1): `none` is `0 0 auto`, `auto` is `1 1 auto`,
/// `initial` is `0 1 auto`; otherwise up to two numbers (grow, then
/// shrink, which defaults to 1) and a basis in either order, the basis
/// `0%` when only numbers are given.
fn expand_flex(values: Vec<Value>, important: bool) -> Vec<Declaration> {
    let number = |value: f32| Value::Number(value);
    let auto = || Value::Keyword("auto".into());
    let longhands = |grow: Value, shrink: Value, basis: Value| {
        vec![
            decl(Property::FlexGrow, vec![grow], important),
            decl(Property::FlexShrink, vec![shrink], important),
            decl(Property::FlexBasis, vec![basis], important),
        ]
    };
    if let [Value::Keyword(keyword)] = values.as_slice() {
        match keyword.to_ascii_lowercase().as_str() {
            "none" => return longhands(number(0.0), number(0.0), auto()),
            "auto" => return longhands(number(1.0), number(1.0), auto()),
            "initial" => return longhands(number(0.0), number(1.0), auto()),
            "inherit" | "unset" | "revert" => {
                let keyword = Value::Keyword(keyword.clone());
                return longhands(keyword.clone(), keyword.clone(), keyword);
            }
            _ => {}
        }
    }
    let mut numbers = Vec::new();
    let mut basis = None;
    for value in values {
        match value {
            Value::Number(_) if numbers.len() < 2 => numbers.push(value),
            other => basis = Some(other),
        }
    }
    let mut numbers = numbers.into_iter();
    let grow = numbers.next();
    let shrink = numbers.next().unwrap_or_else(|| number(1.0));
    match (grow, basis) {
        (Some(grow), basis) => longhands(grow, shrink, basis.unwrap_or(Value::Percentage(0.0))),
        (None, Some(basis)) => longhands(number(1.0), shrink, basis),
        (None, None) => Vec::new(),
    }
}

/// A keyword that names a colour: `transparent`, `currentColor`, or a
/// named colour.
fn is_color_keyword(name: &str) -> bool {
    name.eq_ignore_ascii_case("transparent")
        || name.eq_ignore_ascii_case("currentcolor")
        || Color::named(name).is_some()
}

/// Classify the `<width> <style> <color>` tokens of a border shorthand.
/// A border shorthand's width, style and colour, each part it leaves out
/// reset to its initial value as shorthands do.
fn border_parts(values: Vec<Value>) -> (Value, Value, Value) {
    let (width, style, color) = classify_border(values);
    (
        width.unwrap_or_else(|| Value::Keyword("medium".into())),
        style.unwrap_or_else(|| Value::Keyword("none".into())),
        color.unwrap_or_else(|| Value::Keyword("currentcolor".into())),
    )
}

fn classify_border(values: Vec<Value>) -> (Option<Value>, Option<Value>, Option<Value>) {
    let mut width = None;
    let mut style = None;
    let mut color = None;
    for value in values {
        if value.as_length().is_some() || value.is_calc() {
            width = Some(value);
        } else if let Some(name) = value.as_keyword() {
            if is_color_keyword(name) {
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
    let (width, style, color) = border_parts(values);

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
        if let Some(decls) = box_shorthand(vec![value], important, properties) {
            out.extend(decls);
        }
    }
    out
}

/// `border-<side>: <width> <style> <color>` → that one side's three longhands.
fn expand_border_side(values: Vec<Value>, important: bool, index: usize) -> Vec<Declaration> {
    let (width, style, color) = border_parts(values);
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
        out.push(decl(property, vec![value], important));
    }
    out
}

/// `background: …` → `background-color` and `background-image`.
///
/// The shorthand resets both longhands: the colour it names (or `transparent`),
/// and the gradient it names (or `none`). Other image kinds (`url()`) are not
/// modelled and leave `background-image: none`.
fn expand_background(values: Vec<Value>, important: bool) -> Vec<Declaration> {
    let color = values
        .iter()
        .find_map(|value| {
            if value.as_color().is_some() {
                Some(value.clone())
            } else {
                value
                    .as_keyword()
                    .and_then(|name| is_color_keyword(name).then(|| value.clone()))
            }
        })
        .unwrap_or_else(|| Value::Keyword("transparent".into()));
    let image = values
        .iter()
        .find(|value| value.is_gradient())
        .cloned()
        .unwrap_or_else(|| Value::Keyword("none".into()));
    vec![
        decl(Property::BackgroundColor, vec![color], important),
        decl(Property::BackgroundImage, vec![image], important),
    ]
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

/// `text-decoration: <line>? <style>? <colour>? <thickness>?` → the one line
/// list Noonoo models, `text-decoration-line`.
fn expand_text_decoration(values: Vec<Value>, important: bool) -> Vec<Declaration> {
    let lines: Vec<Value> = values
        .into_iter()
        .filter(|value| {
            value.as_keyword().is_some_and(|keyword| {
                matches!(
                    keyword.to_ascii_lowercase().as_str(),
                    "none" | "underline" | "overline" | "line-through" | "blink"
                )
            })
        })
        .collect();

    // The shorthand resets the line to `none` when it names no line.
    let lines = if lines.is_empty() {
        vec![Value::Keyword("none".into())]
    } else {
        lines
    };
    vec![decl(Property::TextDecorationLine, lines, important)]
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
    fn border_shorthands_reset_what_they_leave_out() {
        let value_of = |decls: &[Declaration], property: Property| {
            decls
                .iter()
                .find(|d| d.property == property)
                .map(|d| d.values.clone())
        };
        let keyword = |k: &str| Some(vec![Value::Keyword(k.into())]);
        let decls = parse_declaration("border", "none", false).unwrap();
        assert_eq!(decls.len(), 12);
        assert_eq!(
            value_of(&decls, Property::BorderLeftWidth),
            keyword("medium")
        );
        assert_eq!(
            value_of(&decls, Property::BorderTopColor),
            keyword("currentcolor")
        );
        let decls = parse_declaration("border-top", "2px", false).unwrap();
        assert_eq!(decls.len(), 3);
        assert_eq!(value_of(&decls, Property::BorderTopStyle), keyword("none"));
    }

    #[test]
    fn border_shorthand_accepts_calc_width() {
        let decls = parse_declaration("border", "calc(2px) solid red", false).unwrap();
        assert_eq!(decls.len(), 12);
        let top_width = decls
            .iter()
            .find(|d| d.property == Property::BorderTopWidth)
            .unwrap();
        assert!(matches!(&top_width.values[0], Value::Function { name, .. } if name == "calc"));
    }

    #[test]
    fn every_shorthand_sample_expands_into_known_longhands() {
        for (name, sample) in SHORTHAND_SAMPLES {
            let decls = parse_declaration(name, sample, false).unwrap();
            // At least one (`text-decoration` models only its line).
            assert!(!decls.is_empty(), "{name}: {sample} gave {decls:?}");
            assert!(
                decls
                    .iter()
                    .all(|d| !matches!(d.property, Property::Unknown(_))),
                "{name}: {sample} gave {decls:?}"
            );
            assert_eq!(
                shorthand_longhands(name).unwrap(),
                props_of(&decls),
                "{name}"
            );
        }
        assert_eq!(shorthand_longhands("color"), None, "not a shorthand");
    }

    fn props_of(decls: &[Declaration]) -> Vec<Property> {
        decls.iter().map(|d| d.property.clone()).collect()
    }

    #[test]
    fn a_var_shorthand_becomes_pending_longhands() {
        let decls = parse_declaration("border", "1px solid var(--c)", true).unwrap();
        assert_eq!(props_of(&decls), shorthand_longhands("border").unwrap());
        for d in &decls {
            assert!(d.important);
            let (shorthand, raw) = pending_shorthand(&d.values).unwrap();
            assert_eq!(shorthand, "border");
            assert_eq!(raw, parse_values("1px solid var(--c)").unwrap().as_slice());
        }
        // Nested var() counts; a longhand with var() is left as it is.
        let decls = parse_declaration("background", "rgb(var(--r), 0, 0)", false).unwrap();
        assert!(pending_shorthand(&decls[0].values).is_some());
        let decls = parse_declaration("color", "var(--c)", false).unwrap();
        assert_eq!(decls.len(), 1);
        assert!(pending_shorthand(&decls[0].values).is_none());
        // No var(): expanded at parse time, as before.
        let decls = parse_declaration("background", "red", false).unwrap();
        assert!(decls.iter().all(|d| pending_shorthand(&d.values).is_none()));
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

        let unknown = parse_declaration("grid-area", "auto", false).unwrap();
        assert_eq!(unknown[0].property, Property::Unknown("grid-area".into()));
    }

    #[test]
    fn background_extracts_the_colour_and_image() {
        let decls =
            parse_declaration("background", "#eeeeee url(bg.png) no-repeat", false).unwrap();
        assert_eq!(props(&decls), vec!["background-color", "background-image"]);
        assert_eq!(
            decls[0].values,
            vec![Value::Color(Color::rgb(0xee, 0xee, 0xee))]
        );
        // `url()` images are not modelled, so the image longhand resets.
        assert_eq!(decls[1].values, vec![Value::Keyword("none".into())]);

        // A shorthand that names no colour resets to transparent.
        let none = parse_declaration("background", "none", false).unwrap();
        assert_eq!(none[0].values, vec![Value::Keyword("transparent".into())]);
    }

    #[test]
    fn background_shorthand_keeps_a_gradient_image() {
        let decls = parse_declaration(
            "background",
            "linear-gradient(to right, red, blue) #fff",
            false,
        )
        .unwrap();
        assert_eq!(props(&decls), vec!["background-color", "background-image"]);
        assert_eq!(
            decls[0].values,
            vec![Value::Color(Color::rgb(255, 255, 255))]
        );
        let [Value::Function { name, .. }] = decls[1].values.as_slice() else {
            panic!("expected a gradient function");
        };
        assert_eq!(name, "linear-gradient");
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

    #[test]
    fn text_decoration_shorthand_expands() {
        let decls = parse_declaration("text-decoration", "underline", false).unwrap();
        assert_eq!(props(&decls), vec!["text-decoration-line"]);
        assert_eq!(decls[0].values, vec![Value::Keyword("underline".into())]);

        // A shorthand that names no line resets it to none.
        let none = parse_declaration("text-decoration", "#ff0000", false).unwrap();
        assert_eq!(none[0].values, vec![Value::Keyword("none".into())]);
    }

    /// `flex` sets all three longhands: `none`, `auto`, `initial`, a lone
    /// number (basis 0%), a lone basis, and the two-value forms, as CSS
    /// Flexbox §7.1 gives them. `flex: none` left `flex-shrink` at 1, so an
    /// inflexible item shrank anyway (rust-lang.org's nav).
    #[test]
    fn flex_expands_to_grow_shrink_and_basis() {
        let flex = |value: &str| {
            let decls = parse_declaration("flex", value, false).unwrap();
            assert_eq!(
                props(&decls),
                vec!["flex-grow", "flex-shrink", "flex-basis"],
                "{value}"
            );
            decls
                .into_iter()
                .map(|d| d.values[0].clone())
                .collect::<Vec<_>>()
        };
        let n = Value::Number;
        let auto = || Value::Keyword("auto".into());
        let px = |v| {
            Value::Length(Length {
                value: v,
                unit: LengthUnit::Px,
            })
        };
        assert_eq!(flex("none"), vec![n(0.0), n(0.0), auto()]);
        assert_eq!(flex("auto"), vec![n(1.0), n(1.0), auto()]);
        assert_eq!(flex("initial"), vec![n(0.0), n(1.0), auto()]);
        assert_eq!(flex("2"), vec![n(2.0), n(1.0), Value::Percentage(0.0)]);
        assert_eq!(flex("30px"), vec![n(1.0), n(1.0), px(30.0)]);
        assert_eq!(flex("2 3"), vec![n(2.0), n(3.0), Value::Percentage(0.0)]);
        assert_eq!(flex("2 30px"), vec![n(2.0), n(1.0), px(30.0)]);
        assert_eq!(flex("2 3 30px"), vec![n(2.0), n(3.0), px(30.0)]);
        assert_eq!(flex("30px 2 3"), vec![n(2.0), n(3.0), px(30.0)]);
    }
}
