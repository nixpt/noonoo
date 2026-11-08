//! # noonoo-style
//!
//! Noonoo's style engine: the CSS **cascade**. It collects rules from the
//! user-agent sheet, author stylesheets, and inline `style` attributes; matches
//! them against a [`noonoo_dom::Document`]; and resolves each element's
//! [`ComputedStyle`] by origin, `!important`, specificity, and source order,
//! then inheritance and initial values.
//!
//! This is Phase A of the dependency-exit plan (`docs/DEPENDENCY-EXIT.md`): it
//! replaces `stylo` with a documented subset. Selectors, values, and properties
//! come from [`noonoo_css`]; this crate owns the matching-to-computed-values
//! step that the borrowed engine used to provide.
//!
//! ## Custom properties and `calc()`
//!
//! `--x` declarations are kept as substitution tokens and inherited like any
//! inherited property. A `var(--x, fallback)` in a declaration is replaced at
//! computed-value time, *after* the cascade has picked a winner, so the value
//! follows cascade and inheritance (see the `vars` module). An unresolvable
//! `var()` drops the declaration, which then keeps its inherited or initial
//! value.
//!
//! `calc()` is evaluated in the same pass, when `em`/`rem` have a font size to
//! resolve against (see the `calc` module). It is typed: lengths and
//! percentages add and subtract, numbers multiply and divide them, and adding a
//! bare number to a length is invalid.
//!
//! `min()`, `max()`, and `clamp()` build on the same evaluator. When their
//! arguments mix percentages with absolute lengths the comparison cannot be
//! settled without the containing block, so the result is kept as a deferred
//! [`Dimension::Extreme`]/[`Length::Extreme`] and resolved in layout. Arguments
//! that share a slope (all absolute, or all percentages) are folded here.
//!
//! Gradients (`linear-gradient()`, `radial-gradient()`, and their `repeating-`
//! forms) parse into [`Gradient`] values in the same pass; geometry that needs
//! the box is resolved later, in `noonoo-paint` (see the `gradient` module).
//!
//! Viewport units (`vw`, `vh`, `vmin`, `vmax`) resolve in the same pass against
//! the [`Viewport`] passed to [`compute_with`].
//!
//! Interactive state — [`Interaction`], the hovered/focused/pressed nodes —
//! flows through [`compute_interactive`], which is what makes `:hover`,
//! `:focus`, and `:active` style the elements they name and nothing else. [`compute`] uses
//! [`Viewport::DEFAULT`] (800x600), matching the headless CLI. `Engine` callers
//! get the real size because the engine computes styles per layout viewport.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod apply;
mod calc;
mod cascade;
pub mod computed;
mod edit;
mod gradient;
mod grid;
mod media;
pub mod ua;
mod vars;

use noonoo_css::Stylesheet;
use noonoo_dom::{Document, NodeId};

pub use computed::{
    AlignItems, BorderCollapse, BorderStyle, BoxSizing, CalcTerm, Clear, ComputedStyle, Corners,
    Cursor, Dimension, Display, Edges, Extreme, FlexDirection, FlexWrap, Float, FontStyle,
    Gradient, GradientKind, GradientStop, GridLine, GridRange, JustifyContent, Length,
    LengthOrAuto, LinearDirection, Overflow, Position, RadialShape, RadialSize, TextAlign,
    TextDecoration, TextTransform, Track, Transform, Visibility, WhiteSpace, ZIndex,
};
pub use edit::{StyleEditError, remove_style_property, set_style_property};

/// Computed styles for a document, indexed by node.
#[derive(Clone, Debug, Default)]
pub struct Styles {
    entries: Vec<Option<ComputedStyle>>,
}

impl Styles {
    /// The computed style of `id`, if it is an element.
    pub fn get(&self, id: NodeId) -> Option<&ComputedStyle> {
        self.entries.get(id.index()).and_then(Option::as_ref)
    }
}

/// The viewport that viewport-relative units (`vw`, `vh`, `vmin`, `vmax`)
/// resolve against.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Viewport {
    /// The viewport width, in pixels.
    pub width: f32,
    /// The viewport height, in pixels.
    pub height: f32,
}

impl Viewport {
    /// The default viewport used by [`compute`] when none is given.
    pub const DEFAULT: Viewport = Viewport {
        width: 800.0,
        height: 600.0,
    };
}

impl Default for Viewport {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The interactive state a cascade styles against: which node the pointer is
/// over, which holds keyboard focus, and which is pressed (`:active`).
///
/// Per node: `:hover` matches only the hovered element, not its ancestors.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Interaction {
    /// The hovered node, if any.
    pub hovered: Option<NodeId>,
    /// The focused node, if any.
    pub focused: Option<NodeId>,
    /// The pressed node, if any.
    pub active: Option<NodeId>,
}

/// Compute every element's style in `document`, cascading the given author
/// stylesheets over the built-in user-agent sheet and inline styles.
///
/// Viewport-relative units resolve against [`Viewport::DEFAULT`]; use
/// [`compute_with`] to supply a real viewport.
pub fn compute(document: &Document, stylesheets: &[Stylesheet]) -> Styles {
    compute_with(document, stylesheets, Viewport::DEFAULT)
}

/// Compute every element's style against a specific viewport, so `vw`/`vh`
/// (and `calc(100vw - 2rem)`) resolve correctly.
pub fn compute_with(document: &Document, stylesheets: &[Stylesheet], viewport: Viewport) -> Styles {
    compute_interactive(document, stylesheets, viewport, Interaction::default())
}

/// Compute every element's style against a viewport *and* the interactive
/// state, so `:hover`/`:focus`/`:active` style the elements they name.
pub fn compute_interactive(
    document: &Document,
    stylesheets: &[Stylesheet],
    viewport: Viewport,
    interaction: Interaction,
) -> Styles {
    Styles {
        entries: cascade::compute(document, stylesheets, viewport, interaction),
    }
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;
    use noonoo_css::Color;

    fn styled(html: &str, css: &str) -> (Document, Styles) {
        let doc = noonoo_html::parse(html);
        let sheet = Stylesheet::parse(css).unwrap();
        let styles = compute(&doc, &[sheet]);
        (doc, styles)
    }

    fn styled_at(html: &str, css: &str, viewport: Viewport) -> (Document, Styles) {
        let doc = noonoo_html::parse(html);
        let sheet = Stylesheet::parse(css).unwrap();
        let styles = compute_with(&doc, &[sheet], viewport);
        (doc, styles)
    }

    #[test]
    fn letter_spacing_resolves_to_pixels_and_inherits() {
        let (doc, styles) = styled(
            r#"<div id="a"><p id="b">x</p></div><div id="c"></div><div id="d"></div>"#,
            "#a { letter-spacing: 2px } #c { font-size: 20px; letter-spacing: 0.5em }
             #d { letter-spacing: normal }",
        );
        let get = |id: &str| styles.get(doc.get_element_by_id(id).unwrap()).unwrap();
        assert_eq!(get("a").letter_spacing, 2.0);
        assert_eq!(get("b").letter_spacing, 2.0);
        assert_eq!(get("c").letter_spacing, 10.0);
        assert_eq!(get("d").letter_spacing, 0.0);
    }

    #[test]
    fn text_transform_parses_and_inherits() {
        let (doc, styles) = styled(
            r#"<div id="a"><p id="b">x</p></div><div id="c"></div><div id="d"></div>"#,
            "#a { text-transform: uppercase } #c { text-transform: capitalize }
             #d { text-transform: lowercase }",
        );
        let get = |id: &str| styles.get(doc.get_element_by_id(id).unwrap()).unwrap();
        assert_eq!(get("b").text_transform, TextTransform::Uppercase);
        assert_eq!(get("c").text_transform, TextTransform::Capitalize);
        assert_eq!(get("d").text_transform, TextTransform::Lowercase);
        assert_eq!(
            get("c").transformed_text("hello big world"),
            "Hello Big World"
        );
        assert_eq!(get("b").transformed_text("straße"), "STRASSE");
        assert_eq!(get("d").transformed_text("ÀB"), "àb");
    }

    #[test]
    fn transform_lists_parse_in_order_and_do_not_inherit() {
        let (doc, styles) = styled(
            r#"<div id="a"><p id="b">x</p></div><div id="c"></div><div id="d"></div>"#,
            "#a { transform: translateX(-50%) translate(4px, 10%) scale(2) rotate(0.25turn) }
             #c { transform: translateY(3px) scale(1, 0.5) scaleX(3) rotate(-90deg) }
             #d { transform: translateX(10px) bogus(1) }",
        );
        let get = |id: &str| styles.get(doc.get_element_by_id(id).unwrap()).unwrap();
        assert_eq!(
            get("a").transform,
            [
                Transform::Translate(Dimension::Percent(-50.0), Dimension::Px(0.0)),
                Transform::Translate(Dimension::Px(4.0), Dimension::Percent(10.0)),
                Transform::Scale(2.0, 2.0),
                Transform::Rotate(90.0),
            ]
        );
        assert!(get("b").transform.is_empty());
        assert_eq!(
            get("c").transform,
            [
                Transform::Translate(Dimension::Px(0.0), Dimension::Px(3.0)),
                Transform::Scale(1.0, 0.5),
                Transform::Scale(3.0, 1.0),
                Transform::Rotate(-90.0),
            ]
        );
        // One bad function invalidates the whole declaration.
        assert!(get("d").transform.is_empty());
    }

    #[test]
    fn line_height_inherits() {
        // A length or percentage inherits as pixels; a bare number inherits
        // as the multiplier, applied to the child's own font size.
        let (doc, styles) = styled(
            r#"<div id="a"><p id="b">x</p></div>
               <div id="c"><p id="d"><b id="e">x</b></p></div>
               <div id="f"><p id="g">x</p></div>
               <div id="h"><p id="i">x</p></div>"#,
            "#a { line-height: 40px } #b { font-size: 30px }
             #c { line-height: 2 } #d { font-size: 20px } #e { font-size: 5px }
             #f { font-size: 10px; line-height: 150% } #g { font-size: 40px }
             #h { line-height: 3 } #i { line-height: normal; font-size: 10px }",
        );
        let lh = |id: &str| {
            styles
                .get(doc.get_element_by_id(id).unwrap())
                .unwrap()
                .line_height
        };
        assert_eq!(lh("b"), 40.0);
        assert_eq!(lh("d"), 40.0);
        assert_eq!(lh("e"), 10.0);
        assert_eq!(lh("g"), 15.0);
        assert_eq!(lh("i"), 12.0);
    }

    #[test]
    fn media_rules_apply_when_the_viewport_matches() {
        // Finding 4 of docs/REVIEW-2026-11-07.md: the at-rule was captured
        // verbatim and never evaluated. The review's exact reproduction is
        // the first case.
        let (doc, styles) = styled_at(
            r#"<p class="m">H</p>"#,
            "@media (min-width:100px){.m{color:blue}}",
            Viewport {
                width: 800.0,
                height: 600.0,
            },
        );
        let m = doc.get_elements_by_tag_name("p")[0];
        assert_eq!(styles.get(m).unwrap().color, Color::rgb(0, 0, 255));

        // The same sheet at a viewport the condition rejects contributes
        // nothing — the UA default stays.
        let (doc, styles) = styled_at(
            r#"<p class="m">H</p>"#,
            "@media (min-width:100px){.m{color:blue}}",
            Viewport {
                width: 50.0,
                height: 600.0,
            },
        );
        let m = doc.get_elements_by_tag_name("p")[0];
        assert_eq!(styles.get(m).unwrap().color, Color::BLACK);

        // A matching media rule and a plain rule cascade in source order.
        let (doc, styles) = styled_at(
            r#"<p class="m">H</p>"#,
            "@media (min-width:100px){.m{color:blue}} .m { color: red }",
            Viewport {
                width: 800.0,
                height: 600.0,
            },
        );
        let m = doc.get_elements_by_tag_name("p")[0];
        assert_eq!(styles.get(m).unwrap().color, Color::rgb(255, 0, 0));
    }

    #[test]
    fn ua_defaults_apply() {
        let (doc, styles) = styled("<body><div>x</div></body>", "");
        let div = doc.get_elements_by_tag_name("div")[0];
        assert_eq!(styles.get(div).unwrap().display, Display::Block);

        let body = doc.get_elements_by_tag_name("body")[0];
        assert_eq!(styles.get(body).unwrap().margin.top, Dimension::Px(8.0));

        let head = noonoo_html::parse("<head><title>t</title></head>");
        let styles = compute(&head, &[]);
        let title = head.get_elements_by_tag_name("title")[0];
        assert_eq!(styles.get(title).unwrap().display, Display::None);
    }

    #[test]
    fn author_overrides_ua_and_inline_overrides_author() {
        let (doc, styles) = styled(
            r#"<body><p class="a" style="color: #00ff00">x</p></body>"#,
            "p.a { color: red; display: flex } p { margin: 4px }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        let s = styles.get(p).unwrap();
        assert_eq!(s.color, Color::rgb(0, 255, 0), "inline wins");
        assert_eq!(s.display, Display::Flex);
        assert_eq!(s.margin.top, Dimension::Px(4.0));
    }

    #[test]
    fn inheritance_and_relative_lengths() {
        let (doc, styles) = styled(
            "<div><span>x</span></div>",
            "div { color: blue } span { font-size: 2em }",
        );
        let span = doc.get_elements_by_tag_name("span")[0];
        let s = styles.get(span).unwrap();
        assert_eq!(s.color, Color::rgb(0, 0, 255), "inherited from div");
        assert_eq!(s.font_size, 32.0, "2em of 16px");
    }

    #[test]
    fn specificity_decides_between_author_rules() {
        let (doc, styles) = styled(
            r#"<div id="main" class="card">x</div>"#,
            "div { color: red } .card { color: green } #main { color: blue }",
        );
        let div = doc.get_element_by_id("main").unwrap();
        assert_eq!(styles.get(div).unwrap().color, Color::rgb(0, 0, 255));
    }

    #[test]
    fn important_beats_later_normal() {
        let (doc, styles) = styled("<p>x</p>", "p { color: red !important } p { color: green }");
        let p = doc.get_elements_by_tag_name("p")[0];
        assert_eq!(styles.get(p).unwrap().color, Color::rgb(255, 0, 0));
    }

    #[test]
    fn global_keywords_inherit_and_initial() {
        // `inherit` on a non-inherited property takes the parent's value.
        let (doc, styles) = styled(
            "<div><span>x</span></div>",
            "div { margin-top: 20px } span { margin-top: inherit }",
        );
        let span = doc.get_elements_by_tag_name("span")[0];
        assert_eq!(styles.get(span).unwrap().margin.top, Dimension::Px(20.0));

        // `initial` on an inherited property resets it.
        let (doc, styles) = styled(
            "<div><span>x</span></div>",
            "div { color: red } span { color: initial }",
        );
        let span = doc.get_elements_by_tag_name("span")[0];
        assert_eq!(styles.get(span).unwrap().color, Color::BLACK);
    }

    #[test]
    fn position_and_offsets_parse() {
        let (doc, styles) = styled(
            r#"<div id="a">x</div>"#,
            "#a { position: absolute; top: 5px; right: 10%; left: auto }",
        );
        let a = doc.get_element_by_id("a").unwrap();
        let s = styles.get(a).unwrap();
        assert_eq!(s.position, Position::Absolute);
        assert_eq!(s.inset.top, Dimension::Px(5.0));
        assert_eq!(s.inset.right, Dimension::Percent(10.0));
        assert_eq!(s.inset.left, Dimension::Auto);
        assert_eq!(s.inset.bottom, Dimension::Auto);
    }

    #[test]
    fn shorthands_reach_computed_style() {
        let (doc, styles) = styled(
            r#"<div class="card">x</div>"#,
            ".card { background: #123456; font: italic bold 20px/1.4 serif }",
        );
        let card = doc.get_elements_by_tag_name("div")[0];
        let s = styles.get(card).unwrap();
        assert_eq!(s.background_color, Color::rgb(0x12, 0x34, 0x56));
        assert_eq!(s.font_style, FontStyle::Italic);
        assert_eq!(s.font_weight, 700);
        assert_eq!(s.font_size, 20.0);
        assert_eq!(s.line_height, 28.0);
        assert_eq!(s.font_family, vec!["serif".to_owned()]);
    }

    #[test]
    fn text_decoration_reaches_computed_style() {
        let (doc, styles) = styled(
            r#"<p class="x">hi</p>"#,
            ".x { text-decoration: underline line-through }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        let s = styles.get(p).unwrap();
        assert!(s.text_decoration.contains(TextDecoration::UNDERLINE));
        assert!(s.text_decoration.contains(TextDecoration::LINE_THROUGH));
        assert!(!s.text_decoration.contains(TextDecoration::OVERLINE));
    }

    #[test]
    fn custom_properties_substitute() {
        let (doc, styles) = styled(
            "<div><p>x</p></div>",
            ":root { --brand: #ff0000 } p { color: var(--brand) }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        assert_eq!(styles.get(p).unwrap().color, Color::rgb(255, 0, 0));
    }

    #[test]
    fn custom_properties_inherit() {
        let (doc, styles) = styled(
            "<div><section><p>x</p></section></div>",
            "div { --gap: 12px } p { margin-top: var(--gap) }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        assert_eq!(styles.get(p).unwrap().margin.top, Dimension::Px(12.0));
    }

    #[test]
    fn a_descendant_can_override_and_forward_the_value() {
        let (doc, styles) = styled(
            "<html><body><div><p>x</p></div></body></html>",
            ":root { --ink: black } div { --ink: blue } p { color: var(--ink) }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        assert_eq!(styles.get(p).unwrap().color, Color::rgb(0, 0, 255));
    }

    #[test]
    fn var_fallback_is_used_when_undefined() {
        let (doc, styles) = styled("<p>x</p>", "p { color: var(--missing, #00ff00) }");
        let p = doc.get_elements_by_tag_name("p")[0];
        assert_eq!(styles.get(p).unwrap().color, Color::rgb(0, 255, 0));
    }

    #[test]
    fn an_unresolvable_var_leaves_the_property_inherited() {
        let (doc, styles) = styled(
            "<div><p>x</p></div>",
            "div { color: red } p { color: var(--missing) }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        assert_eq!(styles.get(p).unwrap().color, Color::rgb(255, 0, 0));
    }

    #[test]
    fn var_works_through_shorthands_and_font_size() {
        let (doc, styles) = styled(
            "<p>x</p>",
            ":root { --gap: 6px; --size: 20px } p { margin: var(--gap); font-size: var(--size) }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        let s = styles.get(p).unwrap();
        assert_eq!(s.margin.top, Dimension::Px(6.0));
        assert_eq!(s.margin.left, Dimension::Px(6.0), "shorthand expands first");
        assert_eq!(s.font_size, 20.0);
    }

    #[test]
    fn a_cycle_falls_back_instead_of_looping() {
        let (doc, styles) = styled(
            "<p>x</p>",
            "p { --a: var(--b); --b: var(--a); color: var(--a, #0000ff) }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        assert_eq!(styles.get(p).unwrap().color, Color::rgb(0, 0, 255));
    }

    #[test]
    fn calc_absolute_lengths_resolve_to_pixels() {
        let (doc, styles) = styled(
            "<p>x</p>",
            "p { margin: calc(4px + 2px); padding-left: calc(2px * 3) }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        let s = styles.get(p).unwrap();
        assert_eq!(s.margin.top, Dimension::Px(6.0));
        assert_eq!(s.padding.left, Length::Px(6.0));
    }

    #[test]
    fn calc_keeps_a_mixed_percentage() {
        let (doc, styles) = styled("<p>x</p>", "p { width: calc(100% - 20px) }");
        let p = doc.get_elements_by_tag_name("p")[0];
        assert_eq!(
            styles.get(p).unwrap().width,
            Dimension::Calc {
                px: -20.0,
                percent: 100.0
            }
        );
    }

    #[test]
    fn calc_uses_the_computed_font_size_and_combines_with_var() {
        let (doc, styles) = styled(
            "<div><p>x</p></div>",
            "div { font-size: 20px } p { --gap: 4px; margin-top: calc(2em - var(--gap)) }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        assert_eq!(styles.get(p).unwrap().margin.top, Dimension::Px(36.0));
    }

    #[test]
    fn calc_font_size_resolves_against_the_parent() {
        let (doc, styles) = styled(
            "<div><p>x</p></div>",
            "div { font-size: 16px } p { font-size: calc(10px + 1em) }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        assert_eq!(styles.get(p).unwrap().font_size, 26.0);
    }

    #[test]
    fn an_invalid_calc_unsets_the_property() {
        let (doc, styles) = styled(
            "<div><p>x</p></div>",
            "div { margin-top: 12px; color: red } p { margin-top: calc(1px + 2) }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        // Adding a number to a length is invalid at computed-value time; a
        // non-inherited property then falls back to its initial value.
        assert_eq!(styles.get(p).unwrap().margin.top, Dimension::Px(0.0));
        assert_eq!(styles.get(p).unwrap().color, Color::rgb(255, 0, 0));
    }

    #[test]
    fn min_and_max_mix_percentages_with_lengths() {
        let (doc, styles) = styled("<p>x</p>", "p { width: min(100%, 600px) }");
        let p = doc.get_elements_by_tag_name("p")[0];
        let expected = Dimension::Extreme {
            kind: Extreme::Min,
            terms: vec![
                CalcTerm {
                    px: 0.0,
                    percent: 100.0,
                },
                CalcTerm {
                    px: 600.0,
                    percent: 0.0,
                },
            ]
            .into_boxed_slice(),
        };
        assert_eq!(styles.get(p).unwrap().width, expected);

        // Absolute-only arguments fold immediately.
        let (doc, styles) = styled("<p>x</p>", "p { width: max(10px, 2em); font-size: 20px }");
        let p = doc.get_elements_by_tag_name("p")[0];
        assert_eq!(styles.get(p).unwrap().width, Dimension::Px(40.0));
    }

    #[test]
    fn clamp_reaches_padding_and_font_size() {
        let (doc, styles) = styled(
            "<div><p>x</p></div>",
            "div { font-size: 16px } p { padding-left: clamp(4px, 10%, 40px); font-size: clamp(10px, 1em, 20px) }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        let s = styles.get(p).unwrap();
        assert!(matches!(
            s.padding.left,
            Length::Extreme {
                kind: Extreme::Clamp,
                ..
            }
        ));
        assert_eq!(s.font_size, 16.0, "clamp(10, 16, 20) at a 16px parent");
    }

    #[test]
    fn viewport_units_resolve_against_the_given_viewport() {
        let viewport = Viewport {
            width: 1000.0,
            height: 500.0,
        };
        let (doc, styles) = styled_at(
            "<p>x</p>",
            "p { width: 50vw; height: 10vh; padding-left: 10vmin; margin-top: 10vmax; font-size: 2vw }",
            viewport,
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        let s = styles.get(p).unwrap();
        assert_eq!(s.width, Dimension::Px(500.0));
        assert_eq!(s.height, Dimension::Px(50.0));
        assert_eq!(s.padding.left, Length::Px(50.0));
        assert_eq!(s.margin.top, Dimension::Px(100.0));
        assert_eq!(s.font_size, 20.0);
    }

    #[test]
    fn viewport_units_compose_with_calc_and_min() {
        let viewport = Viewport {
            width: 1000.0,
            height: 500.0,
        };
        let (doc, styles) = styled_at(
            "<p>x</p>",
            "p { width: calc(100vw - 20px); height: min(100vw, 1200px) }",
            viewport,
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        let s = styles.get(p).unwrap();
        assert_eq!(s.width, Dimension::Px(980.0));
        // Both arguments are absolute once `vw` resolves, so this folds here.
        assert_eq!(s.height, Dimension::Px(1000.0));
    }

    #[test]
    fn border_radius_computes_per_corner() {
        let (doc, styles) = styled(
            "<p>x</p>",
            "p { border-radius: 8px 4px 2px 1px; border-top-left-radius: 10% }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        let radius = &styles.get(p).unwrap().border_radius;
        assert_eq!(radius.top_left, Length::Percent(10.0));
        assert_eq!(radius.top_right, Length::Px(4.0));
        assert_eq!(radius.bottom_right, Length::Px(2.0));
        assert_eq!(radius.bottom_left, Length::Px(1.0));
    }

    #[test]
    fn interactive_pseudo_classes_style_only_the_named_node() {
        let doc = noonoo_html::parse("<div><p id=\"a\">x</p><p id=\"b\">y</p></div>");
        let sheet = Stylesheet::parse("p { color: blue } p:hover { color: red }").unwrap();
        let a = doc.get_element_by_id("a").unwrap();
        let b = doc.get_element_by_id("b").unwrap();
        let div = doc.get_elements_by_tag_name("div")[0];

        // Without state, nothing is hovered.
        let styles = compute(&doc, std::slice::from_ref(&sheet));
        assert_eq!(styles.get(a).unwrap().color, Color::rgb(0, 0, 255));

        let styles = compute_interactive(
            &doc,
            &[sheet],
            Viewport::DEFAULT,
            Interaction {
                hovered: Some(a),
                ..Interaction::default()
            },
        );
        assert_eq!(styles.get(a).unwrap().color, Color::rgb(255, 0, 0));
        assert_eq!(styles.get(b).unwrap().color, Color::rgb(0, 0, 255));
        // The hovered node's ancestors do not match.
        assert_eq!(styles.get(div).unwrap().color, Color::BLACK);
    }

    #[test]
    fn focus_and_active_are_separate_states() {
        let doc = noonoo_html::parse("<input id=\"i\">");
        let sheet = Stylesheet::parse(
            "input { color: black } input:focus { color: blue } input:active { color: red }",
        )
        .unwrap();
        let input = doc.get_element_by_id("i").unwrap();
        let styles = compute_interactive(
            &doc,
            &[sheet],
            Viewport::DEFAULT,
            Interaction {
                focused: Some(input),
                active: Some(input),
                ..Interaction::default()
            },
        );
        // `:active` wins by source order.
        assert_eq!(styles.get(input).unwrap().color, Color::rgb(255, 0, 0));
    }

    #[test]
    fn min_and_max_sizing_parse() {
        let (doc, styles) = styled(
            "<p>x</p>",
            "p { max-width: calc(100% - 20px); min-height: 2em }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        let style = styles.get(p).unwrap();
        assert_eq!(
            style.max_width,
            Dimension::Calc {
                px: -20.0,
                percent: 100.0
            }
        );
        assert_eq!(style.min_height, Dimension::Px(32.0));
        // Defaults are `auto`.
        assert_eq!(style.min_width, Dimension::Auto);
        assert_eq!(style.max_height, Dimension::Auto);
    }

    #[test]
    fn min_and_max_compose_with_variables() {
        let (doc, styles) = styled(
            "<p>x</p>",
            ":root { --cap: 300px } p { width: min(100%, var(--cap)) }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        let Dimension::Extreme { terms, .. } = &styles.get(p).unwrap().width else {
            panic!("expected a deferred comparison");
        };
        assert_eq!(
            terms[1],
            CalcTerm {
                px: 300.0,
                percent: 0.0
            }
        );
    }

    // ── var() inside shorthands: substituted, then expanded, per longhand ──

    fn first_p(html: &str, css: &str) -> ComputedStyle {
        let (doc, styles) = styled(html, css);
        let p = doc.get_elements_by_tag_name("p")[0];
        styles.get(p).unwrap().clone()
    }

    #[test]
    fn var_in_the_background_shorthand() {
        let s = first_p(
            "<p>x</p>",
            ":root { --c: #ff0000 } p { background: var(--c) }",
        );
        assert_eq!(s.background_color, Color::rgb(255, 0, 0));
    }

    #[test]
    fn var_in_the_border_shorthand() {
        let s = first_p(
            "<p>x</p>",
            ":root { --c: #00ff00 } p { border: 2px solid var(--c) }",
        );
        assert_eq!(s.border_color.top, Color::rgb(0, 255, 0));
        assert_eq!(s.border_color.left, Color::rgb(0, 255, 0));
        assert_eq!(s.border_width.bottom, 2.0);
        assert_eq!(s.border_style.right, BorderStyle::Solid);
    }

    #[test]
    fn a_var_holding_several_values_expands_after_substitution() {
        // `--m` is two values: it must split into top/bottom and left/right,
        // not be copied whole onto every side.
        let s = first_p("<p>x</p>", ":root { --m: 1px 2px } p { margin: var(--m) }");
        assert_eq!(s.margin.top, Dimension::Px(1.0));
        assert_eq!(s.margin.right, Dimension::Px(2.0));
        assert_eq!(s.margin.bottom, Dimension::Px(1.0));
        assert_eq!(s.margin.left, Dimension::Px(2.0));
        let s = first_p(
            "<p>x</p>",
            ":root { --b: 3px dashed #0000ff } p { border: var(--b) }",
        );
        assert_eq!(s.border_width.top, 3.0);
        assert_eq!(s.border_style.top, BorderStyle::Dashed);
        assert_eq!(s.border_color.top, Color::rgb(0, 0, 255));
    }

    #[test]
    fn each_longhand_of_a_var_shorthand_still_cascades_on_its_own() {
        let later_longhand = first_p(
            "<p>x</p>",
            ":root { --c: #ff0000 } p { background: var(--c) } p { background-color: #0000ff }",
        );
        assert_eq!(later_longhand.background_color, Color::rgb(0, 0, 255));
        let later_shorthand = first_p(
            "<p>x</p>",
            ":root { --c: #ff0000 } p { background-color: #0000ff } p { background: var(--c) }",
        );
        assert_eq!(later_shorthand.background_color, Color::rgb(255, 0, 0));
    }

    #[test]
    fn an_unresolvable_var_shorthand_leaves_its_longhands_initial() {
        let s = first_p(
            "<p>x</p>",
            "p { border-color: #ff0000 } p { border: var(--missing) }",
        );
        assert_eq!(
            s.border_width.top,
            ComputedStyle::default().border_width.top
        );
        assert_eq!(s.border_style.top, BorderStyle::None);
        let s = first_p(
            "<p>x</p>",
            "p { border: 1px solid var(--missing, #00ff00) }",
        );
        assert_eq!(
            s.border_color.top,
            Color::rgb(0, 255, 0),
            "the fallback applies"
        );
    }

    #[test]
    fn var_in_the_font_shorthand_reaches_font_size() {
        let s = first_p("<p>x</p>", ":root { --f: 20px serif } p { font: var(--f) }");
        assert_eq!(s.font_size, 20.0);
    }

    #[test]
    fn var_in_flex_overflow_and_gap_shorthands() {
        let (doc, styles) = styled(
            r#"<div id="d"><p>x</p></div>"#,
            ":root { --g: 2 } #d { display: flex } p { flex: var(--g); overflow: var(--o, hidden) }",
        );
        let p = doc.get_elements_by_tag_name("p")[0];
        let s = styles.get(p).unwrap();
        assert_eq!(s.flex_grow, 2.0);
        assert_eq!(s.overflow_x, Overflow::Hidden);
        assert_eq!(s.overflow_y, Overflow::Hidden);
    }
}
