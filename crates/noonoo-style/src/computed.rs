//! Computed values.
//!
//! The result of the cascade: for each element, every modelled property reduced
//! to an absolute, ready-to-lay-out value. Lengths are resolved to pixels where
//! the containing block is known (`em`/`rem`/absolute units); percentages that
//! need a containing block are kept as percentages for layout to resolve.

use noonoo_css::Color;

/// A `<display>` value (the subset Noonoo models).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Display {
    /// `none`.
    None,
    /// `block`.
    Block,
    /// `inline`.
    Inline,
    /// `inline-block`.
    InlineBlock,
    /// `flex`.
    Flex,
    /// `inline-flex`.
    InlineFlex,
    /// `list-item`.
    ListItem,
}

/// A `<position>` value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Position {
    /// `static`.
    Static,
    /// `relative`.
    Relative,
    /// `absolute`.
    Absolute,
    /// `fixed`.
    Fixed,
    /// `sticky`.
    Sticky,
}

/// A dimension that may be `auto`, a pixel length, or a percentage.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Dimension {
    /// `auto`.
    Auto,
    /// An absolute length in pixels.
    Px(f32),
    /// A percentage of the containing block.
    Percent(f32),
    /// A `calc()` result that mixes a percentage and an absolute part.
    Calc {
        /// The absolute part, in pixels.
        px: f32,
        /// The percentage part (`50.0` for `50%`).
        percent: f32,
    },
}

/// A margin: a dimension that may be `auto`.
pub type LengthOrAuto = Dimension;

/// A padding or radius: a non-negative length, pixels or percentage.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Length {
    /// An absolute length in pixels.
    Px(f32),
    /// A percentage.
    Percent(f32),
    /// A `calc()` result that mixes a percentage and an absolute part.
    Calc {
        /// The absolute part, in pixels.
        px: f32,
        /// The percentage part (`50.0` for `50%`).
        percent: f32,
    },
}

impl Length {
    /// Zero length.
    pub const ZERO: Length = Length::Px(0.0);
}

/// The four physical edges of a box.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Edges<T> {
    /// `top`.
    pub top: T,
    /// `right`.
    pub right: T,
    /// `bottom`.
    pub bottom: T,
    /// `left`.
    pub left: T,
}

impl<T: Copy> Edges<T> {
    /// The same value on all four edges.
    pub fn all(value: T) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

/// `box-sizing`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoxSizing {
    /// `content-box`.
    ContentBox,
    /// `border-box`.
    BorderBox,
}

/// `text-align`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextAlign {
    /// `start`.
    Start,
    /// `end`.
    End,
    /// `left`.
    Left,
    /// `right`.
    Right,
    /// `center`.
    Center,
    /// `justify`.
    Justify,
}

/// `white-space`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WhiteSpace {
    /// `normal`.
    Normal,
    /// `nowrap`.
    NoWrap,
    /// `pre`.
    Pre,
    /// `pre-wrap`.
    PreWrap,
    /// `pre-line`.
    PreLine,
}

/// `visibility`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Visibility {
    /// `visible`.
    Visible,
    /// `hidden`.
    Hidden,
    /// `collapse`.
    Collapse,
}

/// `overflow`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Overflow {
    /// `visible`.
    Visible,
    /// `hidden`.
    Hidden,
    /// `clip`.
    Clip,
    /// `scroll`.
    Scroll,
    /// `auto`.
    Auto,
}

/// `font-style`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FontStyle {
    /// `normal`.
    Normal,
    /// `italic`.
    Italic,
    /// `oblique`.
    Oblique,
}

/// `border-style` (`none` collapses the border to zero width).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BorderStyle {
    /// `none`.
    None,
    /// `hidden`.
    Hidden,
    /// `solid`.
    Solid,
    /// `dashed`.
    Dashed,
    /// `dotted`.
    Dotted,
    /// `double`.
    Double,
    /// `groove`.
    Groove,
    /// `ridge`.
    Ridge,
    /// `inset`.
    Inset,
    /// `outset`.
    Outset,
}

impl BorderStyle {
    /// Whether this style paints a border.
    pub fn is_visible(self) -> bool {
        self != BorderStyle::None && self != BorderStyle::Hidden
    }
}

/// `flex-direction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlexDirection {
    /// `row`.
    Row,
    /// `row-reverse`.
    RowReverse,
    /// `column`.
    Column,
    /// `column-reverse`.
    ColumnReverse,
}

/// `flex-wrap`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlexWrap {
    /// `nowrap`.
    NoWrap,
    /// `wrap`.
    Wrap,
    /// `wrap-reverse`.
    WrapReverse,
}

/// `justify-content`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JustifyContent {
    /// `flex-start` / `start`.
    Start,
    /// `flex-end` / `end`.
    End,
    /// `center`.
    Center,
    /// `space-between`.
    SpaceBetween,
    /// `space-around`.
    SpaceAround,
    /// `space-evenly`.
    SpaceEvenly,
}

/// `align-items` / `align-self`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AlignItems {
    /// `stretch`.
    Stretch,
    /// `flex-start` / `start`.
    Start,
    /// `flex-end` / `end`.
    End,
    /// `center`.
    Center,
    /// `baseline`.
    Baseline,
}

/// `z-index`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ZIndex {
    /// `auto`.
    Auto,
    /// An integer.
    Number(i32),
}

/// `text-decoration-line`: the lines drawn with a text run, as a small bitset.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TextDecoration(u8);

impl TextDecoration {
    /// No decoration.
    pub const NONE: TextDecoration = TextDecoration(0);
    /// `underline`.
    pub const UNDERLINE: TextDecoration = TextDecoration(0b001);
    /// `overline`.
    pub const OVERLINE: TextDecoration = TextDecoration(0b010);
    /// `line-through`.
    pub const LINE_THROUGH: TextDecoration = TextDecoration(0b100);

    /// Whether `flag` is in the set.
    pub fn contains(self, flag: TextDecoration) -> bool {
        self.0 & flag.0 != 0
    }

    /// The union of two sets.
    pub fn union(self, other: TextDecoration) -> TextDecoration {
        TextDecoration(self.0 | other.0)
    }

    /// Whether the set is empty.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// `cursor`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Cursor {
    /// `auto`.
    Auto,
    /// `default`.
    Default,
    /// `pointer`.
    Pointer,
    /// `text`.
    Text,
    /// `move`.
    Move,
    /// `not-allowed`.
    NotAllowed,
    /// Any other keyword.
    Other(String),
}

/// The computed style of one element.
#[derive(Clone, PartialEq, Debug)]
pub struct ComputedStyle {
    /// `display`.
    pub display: Display,
    /// `position`.
    pub position: Position,
    /// `color`.
    pub color: Color,
    /// `background-color`.
    pub background_color: Color,
    /// `font-size`, in pixels.
    pub font_size: f32,
    /// `font-weight`.
    pub font_weight: u16,
    /// `font-style`.
    pub font_style: FontStyle,
    /// `font-family`.
    pub font_family: Vec<String>,
    /// `line-height`, in pixels (`normal` resolves to `1.2 * font-size`).
    pub line_height: f32,
    /// `text-align`.
    pub text_align: TextAlign,
    /// `text-decoration-line`.
    pub text_decoration: TextDecoration,
    /// `white-space`.
    pub white_space: WhiteSpace,
    /// `visibility`.
    pub visibility: Visibility,
    /// `opacity`.
    pub opacity: f32,
    /// `width`.
    pub width: Dimension,
    /// `height`.
    pub height: Dimension,
    /// `top` / `right` / `bottom` / `left` offsets.
    pub inset: Edges<Dimension>,
    /// `margin`.
    pub margin: Edges<LengthOrAuto>,
    /// `padding`.
    pub padding: Edges<Length>,
    /// Resolved border widths, in pixels.
    pub border_width: Edges<f32>,
    /// `border-style`.
    pub border_style: Edges<BorderStyle>,
    /// `border-color`.
    pub border_color: Edges<Color>,
    /// `box-sizing`.
    pub box_sizing: BoxSizing,
    /// `flex-direction`.
    pub flex_direction: FlexDirection,
    /// `flex-wrap`.
    pub flex_wrap: FlexWrap,
    /// `flex-grow`.
    pub flex_grow: f32,
    /// `flex-shrink`.
    pub flex_shrink: f32,
    /// `flex-basis`.
    pub flex_basis: Dimension,
    /// `justify-content`.
    pub justify_content: JustifyContent,
    /// `align-items`.
    pub align_items: AlignItems,
    /// `align-self`.
    pub align_self: Option<AlignItems>,
    /// `row-gap`, in pixels.
    pub row_gap: f32,
    /// `column-gap`, in pixels.
    pub column_gap: f32,
    /// `overflow-x`.
    pub overflow_x: Overflow,
    /// `overflow-y`.
    pub overflow_y: Overflow,
    /// `z-index`.
    pub z_index: ZIndex,
    /// `cursor`.
    pub cursor: Cursor,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::Inline,
            position: Position::Static,
            color: Color::BLACK,
            background_color: Color::TRANSPARENT,
            font_size: 16.0,
            font_weight: 400,
            font_style: FontStyle::Normal,
            font_family: Vec::new(),
            line_height: 16.0 * 1.2,
            text_align: TextAlign::Start,
            text_decoration: TextDecoration::NONE,
            white_space: WhiteSpace::Normal,
            visibility: Visibility::Visible,
            opacity: 1.0,
            width: Dimension::Auto,
            height: Dimension::Auto,
            inset: Edges::all(Dimension::Auto),
            margin: Edges::all(Dimension::Px(0.0)),
            padding: Edges::all(Length::ZERO),
            border_width: Edges::all(0.0),
            border_style: Edges::all(BorderStyle::None),
            border_color: Edges::all(Color::BLACK),
            box_sizing: BoxSizing::ContentBox,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::NoWrap,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Dimension::Auto,
            justify_content: JustifyContent::Start,
            align_items: AlignItems::Stretch,
            align_self: None,
            row_gap: 0.0,
            column_gap: 0.0,
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
            z_index: ZIndex::Auto,
            cursor: Cursor::Auto,
        }
    }
}

impl ComputedStyle {
    /// The inherited subset, copied from the parent before the cascade runs.
    pub fn inherited_from(parent: &ComputedStyle) -> ComputedStyle {
        ComputedStyle {
            color: parent.color,
            font_size: parent.font_size,
            font_weight: parent.font_weight,
            font_style: parent.font_style,
            font_family: parent.font_family.clone(),
            text_align: parent.text_align,
            white_space: parent.white_space,
            visibility: parent.visibility,
            cursor: parent.cursor.clone(),
            ..ComputedStyle::default()
        }
    }
}
