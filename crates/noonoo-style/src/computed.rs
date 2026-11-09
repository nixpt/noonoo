//! Computed values.
//!
//! The result of the cascade: for each element, every modelled property reduced
//! to an absolute, ready-to-lay-out value. Lengths are resolved to pixels where
//! the containing block is known (`em`/`rem`/absolute units); percentages that
//! need a containing block are kept as percentages for layout to resolve.

use noonoo_css::Color;
use noonoo_text::FontRequest;

/// A `<display>` value (the subset Noonoo models).
///
/// The table values model the *structures* a configuration document uses:
/// `table`, row groups, `table-row` and `table-cell`. The exotic parts of
/// CSS Tables (anonymous-cell fixup beyond stray cells, `inline-table`,
/// captions, fixed table layout, `border-spacing`) are not modelled; see
/// `noonoo-layout/src/table.rs` for what layout does with them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Display {
    /// `none`.
    None,
    /// `block`.
    Block,
    /// `flow-root`: a block box that is its own block formatting context.
    FlowRoot,
    /// `inline`.
    Inline,
    /// `inline-block`.
    InlineBlock,
    /// `flex`.
    Flex,
    /// `inline-flex`.
    InlineFlex,
    /// `grid`.
    Grid,
    /// `inline-grid`.
    InlineGrid,
    /// `list-item`.
    ListItem,
    /// `table`.
    Table,
    /// `table-row-group` (also `thead`/`tbody`/`tfoot`).
    TableRowGroup,
    /// `table-row`.
    TableRow,
    /// `table-cell`.
    TableCell,
}

/// `border-collapse`: whether adjacent cells' borders merge into one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BorderCollapse {
    /// `separate` — each cell paints all of its borders (the initial value).
    Separate,
    /// `collapse` — a shared edge is painted once: the cell to the left of
    /// (or above) the edge drops its right (or bottom) border, and the
    /// table's own border paints the frame.
    Collapse,
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
#[derive(Clone, PartialEq, Debug)]
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
    /// A `min()`/`max()`/`clamp()` whose terms cannot be ordered until the
    /// containing block is known (because they mix percentages and lengths).
    Extreme {
        /// Which extreme to select.
        kind: Extreme,
        /// The terms, each `px + percent%`.
        terms: Box<[CalcTerm]>,
    },
}

/// One term of a deferred comparison: `percent% + px`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CalcTerm {
    /// The absolute part, in pixels.
    pub px: f32,
    /// The percentage part (`50.0` for `50%`).
    pub percent: f32,
}

/// Which value a `min()`/`max()`/`clamp()` selects.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Extreme {
    /// `min()`.
    Min,
    /// `max()`.
    Max,
    /// `clamp(min, value, max)`.
    Clamp,
}

/// A margin: a dimension that may be `auto`.
pub type LengthOrAuto = Dimension;

/// A padding or radius: a non-negative length, pixels or percentage.
#[derive(Clone, PartialEq, Debug)]
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
    /// A `min()`/`max()`/`clamp()` deferred to layout (see [`Dimension`]).
    Extreme {
        /// Which extreme to select.
        kind: Extreme,
        /// The terms, each `px + percent%`.
        terms: Box<[CalcTerm]>,
    },
}

impl Dimension {
    /// Resolve against a reference length (usually the containing block's
    /// width or height). `auto` has no length and yields `None`.
    ///
    /// The result is **not** clamped: the caller decides whether a negative
    /// value is legal (margins allow it, widths do not).
    pub fn resolve(&self, reference: f32) -> Option<f32> {
        match self {
            Self::Auto => None,
            Self::Px(px) => Some(*px),
            Self::Percent(percent) => Some(reference * percent / 100.0),
            Self::Calc { px, percent } => Some(px + reference * percent / 100.0),
            Self::Extreme { kind, terms } => Some(extreme(*kind, terms, reference)),
        }
    }
}

impl Length {
    /// Zero length.
    pub const ZERO: Length = Length::Px(0.0);

    /// Resolve against a reference length; the result is not clamped.
    pub fn resolve(&self, reference: f32) -> f32 {
        match self {
            Self::Px(px) => *px,
            Self::Percent(percent) => reference * percent / 100.0,
            Self::Calc { px, percent } => px + reference * percent / 100.0,
            Self::Extreme { kind, terms } => extreme(*kind, terms, reference),
        }
    }
}

/// Fold a deferred `min()`/`max()`/`clamp()` against a reference length.
fn extreme(kind: Extreme, terms: &[CalcTerm], reference: f32) -> f32 {
    let values: Vec<f32> = terms
        .iter()
        .map(|term| term.px + reference * term.percent / 100.0)
        .collect();
    match kind {
        Extreme::Min => values.iter().copied().reduce(f32::min).unwrap_or(0.0),
        Extreme::Max => values.iter().copied().reduce(f32::max).unwrap_or(0.0),
        Extreme::Clamp => match values.as_slice() {
            [min, value, max] => min.max(value.min(*max)),
            _ => 0.0,
        },
    }
}

/// A parsed CSS gradient (`linear-gradient()` / `radial-gradient()`).
#[derive(Clone, PartialEq, Debug)]
pub struct Gradient {
    /// Linear or radial.
    pub kind: GradientKind,
    /// Whether the stops repeat past the end of the line.
    pub repeating: bool,
    /// The colour stops, left to right.
    pub stops: Vec<GradientStop>,
}

/// The geometry of a gradient.
#[derive(Clone, PartialEq, Debug)]
pub enum GradientKind {
    /// A linear gradient.
    Linear {
        /// The gradient direction.
        direction: LinearDirection,
    },
    /// A radial gradient.
    Radial {
        /// `circle` or `ellipse`.
        shape: RadialShape,
        /// How the ending shape is sized.
        size: RadialSize,
        /// The centre, as an offset from the box's top-left corner.
        center: (Dimension, Dimension),
    },
}

/// One track of a grid template.
#[derive(Clone, PartialEq, Debug)]
pub enum Track {
    /// `auto`: sized to the content, stretching with leftover space.
    Auto,
    /// A length or percentage (including `calc()`), resolved against the
    /// relevant axis of the grid container.
    Length(Dimension),
    /// A flexible fraction of the leftover space.
    Fr(f32),
}

/// One side of a `grid-column`/`grid-row` placement.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum GridLine {
    /// `auto`: placed by auto-placement.
    #[default]
    Auto,
    /// A line number (1-based, from the start edge).
    Line(u16),
    /// `span N` tracks wide (default 1).
    Span(u16),
}

/// A `grid-column`/`grid-row` placement: a start line and an end (or span).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct GridRange {
    /// The start line.
    pub start: GridLine,
    /// The end line or span.
    pub end: GridLine,
}

impl GridRange {
    /// The span this range asks for, when resolvable from `auto`-free
    /// endpoints: `1 / 3` spans two tracks.
    pub fn span(&self) -> Option<u16> {
        match (self.start, self.end) {
            (GridLine::Line(start), GridLine::Line(end)) if end > start => Some(end - start),
            (GridLine::Line(_), GridLine::Span(span)) => Some(span),
            (GridLine::Span(span), _) => Some(span),
            _ => None,
        }
    }
}

/// The direction of a linear gradient.
///
/// Corner directions are kept symbolic: the angle depends on the box's aspect
/// ratio, so paint resolves them.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LinearDirection {
    /// An explicit CSS angle in degrees (`0` points up, `90` points right).
    Angle(f32),
    /// `to top`.
    ToTop,
    /// `to right`.
    ToRight,
    /// `to bottom`.
    ToBottom,
    /// `to left`.
    ToLeft,
    /// `to top right`.
    ToTopRight,
    /// `to bottom right`.
    ToBottomRight,
    /// `to bottom left`.
    ToBottomLeft,
    /// `to top left`.
    ToTopLeft,
}

/// The ending shape of a radial gradient.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RadialShape {
    /// A circle.
    Circle,
    /// An ellipse.
    Ellipse,
}

/// How a radial gradient's ending shape is sized.
#[derive(Clone, PartialEq, Debug)]
pub enum RadialSize {
    /// `closest-side`.
    ClosestSide,
    /// `farthest-side`.
    FarthestSide,
    /// `closest-corner`.
    ClosestCorner,
    /// `farthest-corner`.
    FarthestCorner,
    /// Explicit radii; `y` is `None` when only one radius was given (or for a
    /// circle).
    Explicit {
        /// The horizontal radius.
        x: Dimension,
        /// The vertical radius.
        y: Option<Dimension>,
    },
}

/// One colour stop.
#[derive(Clone, PartialEq, Debug)]
pub struct GradientStop {
    /// The stop colour.
    pub color: Color,
    /// Its position, as a length or percentage of the gradient line. `None`
    /// means "interpolate evenly between the neighbours".
    pub position: Option<Dimension>,
}

/// The four corners of a box (`top-left`, `top-right`, `bottom-right`,
/// `bottom-left`, CSS order).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Corners<T> {
    /// `top-left`.
    pub top_left: T,
    /// `top-right`.
    pub top_right: T,
    /// `bottom-right`.
    pub bottom_right: T,
    /// `bottom-left`.
    pub bottom_left: T,
}

impl<T: Clone> Corners<T> {
    /// The same value on all four corners.
    pub fn all(value: T) -> Self {
        Self {
            top_left: value.clone(),
            top_right: value.clone(),
            bottom_right: value.clone(),
            bottom_left: value,
        }
    }
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

impl<T: Clone> Edges<T> {
    /// The same value on all four edges.
    pub fn all(value: T) -> Self {
        Self {
            top: value.clone(),
            right: value.clone(),
            bottom: value.clone(),
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
    /// `-webkit-center` (and `-moz-center`): centred text, and centred
    /// block children whose side margins aren't `auto`. `<center>` uses it.
    WebkitCenter,
}

/// One `transform` function. Lengths stay unresolved: a percentage
/// translation is of the element's own border box, known only at paint.
#[derive(Clone, PartialEq, Debug)]
pub enum Transform {
    /// `translate()` / `translateX()` / `translateY()`.
    Translate(Dimension, Dimension),
    /// `scale()` / `scaleX()` / `scaleY()`.
    Scale(f32, f32),
    /// `rotate()`, in degrees, clockwise.
    Rotate(f32),
}

/// One `box-shadow`, in pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BoxShadow {
    /// Horizontal offset.
    pub x: f32,
    /// Vertical offset.
    pub y: f32,
    /// Blur radius (non-negative).
    pub blur: f32,
    /// Spread: grows (or, negative, shrinks) the shadow box.
    pub spread: f32,
    /// The colour; `None` is `currentColor`, the element's final `color`.
    pub color: Option<noonoo_css::Color>,
    /// An `inset` shadow, drawn inside the padding box.
    pub inset: bool,
}

/// `vertical-align`: where an inline-level box sits on its line,
/// relative to its parent's baseline (CSS 2 §10.8.1).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum VerticalAlign {
    /// `baseline`.
    #[default]
    Baseline,
    /// `sub`: lowered as a subscript.
    Sub,
    /// `super`: raised as a superscript.
    Super,
    /// `text-top`: its top at the parent's content-area top.
    TextTop,
    /// `text-bottom`: its bottom at the parent's content-area bottom.
    TextBottom,
    /// `middle`: its middle half the parent's x-height above the baseline.
    Middle,
    /// `top`: its top at the line box's top.
    Top,
    /// `bottom`: its bottom at the line box's bottom.
    Bottom,
    /// A length, in pixels: raised by it (negative lowers).
    Length(f32),
    /// A percentage of the box's own `line-height`: raised by it.
    Percent(f32),
}

/// `text-transform`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TextTransform {
    /// `none`.
    #[default]
    None,
    /// `uppercase`.
    Uppercase,
    /// `lowercase`.
    Lowercase,
    /// `capitalize`: the first letter of each word.
    Capitalize,
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

/// `float`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Float {
    /// `none` — normal flow.
    #[default]
    None,
    /// `left`.
    Left,
    /// `right`.
    Right,
}

/// `clear`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Clear {
    /// `none`.
    #[default]
    None,
    /// `left`.
    Left,
    /// `right`.
    Right,
    /// `both`.
    Both,
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
    /// `border-collapse`.
    pub border_collapse: BorderCollapse,
    /// `position`.
    pub position: Position,
    /// `float`.
    pub float: Float,
    /// `clear`.
    pub clear: Clear,
    /// `grid-template-columns`.
    pub grid_template_columns: Vec<Track>,
    /// `grid-template-rows`.
    pub grid_template_rows: Vec<Track>,
    /// `grid-auto-rows`, for implicit rows.
    pub grid_auto_rows: Vec<Track>,
    /// `grid-auto-columns`, for implicit columns.
    pub grid_auto_columns: Vec<Track>,
    /// `grid-column` placement.
    pub grid_column: GridRange,
    /// `grid-row` placement.
    pub grid_row: GridRange,
    /// `color`.
    pub color: Color,
    /// `background-color`.
    pub background_color: Color,
    /// `background-image`, when it is a gradient.
    pub background_image: Option<Gradient>,
    /// `font-size`, in pixels.
    pub font_size: f32,
    /// `font-weight`.
    pub font_weight: u16,
    /// `font-style`.
    pub font_style: FontStyle,
    /// `font-family`.
    pub font_family: Vec<String>,
    /// `line-height`, in pixels; `None` for `normal`, the font's own line
    /// height (its ascent, descent and line gap), which only a shaper knows.
    pub line_height: Option<f32>,
    /// `text-align`.
    pub text_align: TextAlign,
    /// `text-decoration-line`.
    pub text_decoration: TextDecoration,
    /// `white-space`.
    pub white_space: WhiteSpace,
    /// `letter-spacing`, in pixels (`normal` is `0`).
    pub letter_spacing: f32,
    /// `text-transform`.
    pub text_transform: TextTransform,
    /// `vertical-align`.
    pub vertical_align: VerticalAlign,
    /// `visibility`.
    pub visibility: Visibility,
    /// `opacity`.
    pub opacity: f32,
    /// `transform`, in the order written (empty for `none`).
    pub transform: Vec<Transform>,
    /// `box-shadow`, first listed (topmost) first; empty for `none`.
    pub box_shadow: Vec<BoxShadow>,
    /// `width`.
    pub width: Dimension,
    /// `min-width`.
    pub min_width: Dimension,
    /// `max-width`.
    pub max_width: Dimension,
    /// `height`.
    pub height: Dimension,
    /// `min-height`.
    pub min_height: Dimension,
    /// `max-height`.
    pub max_height: Dimension,
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
    /// `border-radius`.
    pub border_radius: Corners<Length>,
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
            border_collapse: BorderCollapse::Separate,
            position: Position::Static,
            float: Float::None,
            clear: Clear::None,
            grid_template_columns: Vec::new(),
            grid_template_rows: Vec::new(),
            grid_auto_rows: Vec::new(),
            grid_auto_columns: Vec::new(),
            grid_column: GridRange::default(),
            grid_row: GridRange::default(),
            color: Color::BLACK,
            background_color: Color::TRANSPARENT,
            background_image: None,
            font_size: 16.0,
            font_weight: 400,
            font_style: FontStyle::Normal,
            font_family: Vec::new(),
            line_height: None,
            text_align: TextAlign::Start,
            text_decoration: TextDecoration::NONE,
            white_space: WhiteSpace::Normal,
            letter_spacing: 0.0,
            text_transform: TextTransform::None,
            vertical_align: VerticalAlign::Baseline,
            visibility: Visibility::Visible,
            opacity: 1.0,
            transform: Vec::new(),
            box_shadow: Vec::new(),
            width: Dimension::Auto,
            min_width: Dimension::Auto,
            max_width: Dimension::Auto,
            height: Dimension::Auto,
            min_height: Dimension::Auto,
            max_height: Dimension::Auto,
            inset: Edges::all(Dimension::Auto),
            margin: Edges::all(Dimension::Px(0.0)),
            padding: Edges::all(Length::ZERO),
            border_width: Edges::all(0.0),
            border_style: Edges::all(BorderStyle::None),
            border_color: Edges::all(Color::BLACK),
            border_radius: Corners::all(Length::ZERO),
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
    /// The font request this style makes: its family list, weight, and style,
    /// as a `FontRequest` for the shaper seam.
    pub fn font_request(&self) -> FontRequest<'_> {
        FontRequest {
            families: &self.font_family,
            weight: self.font_weight,
            italic: self.font_style == FontStyle::Italic,
        }
    }

    /// `text` as this element shows it, after `text-transform`. Layout
    /// measures and paint draws this; the DOM text is unchanged.
    pub fn transformed_text<'t>(&self, text: &'t str) -> std::borrow::Cow<'t, str> {
        use std::borrow::Cow;
        match self.text_transform {
            TextTransform::None => Cow::Borrowed(text),
            TextTransform::Uppercase => Cow::Owned(text.to_uppercase()),
            TextTransform::Lowercase => Cow::Owned(text.to_lowercase()),
            TextTransform::Capitalize => {
                let mut out = String::with_capacity(text.len());
                let mut start = true;
                for c in text.chars() {
                    if start && c.is_alphanumeric() {
                        out.extend(c.to_uppercase());
                    } else {
                        out.push(c);
                    }
                    start = c.is_whitespace();
                }
                Cow::Owned(out)
            }
        }
    }

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
            letter_spacing: parent.letter_spacing,
            text_transform: parent.text_transform,
            visibility: parent.visibility,
            cursor: parent.cursor.clone(),
            ..ComputedStyle::default()
        }
    }
}
