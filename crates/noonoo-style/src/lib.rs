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

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod apply;
mod cascade;
pub mod computed;
pub mod ua;

use noonoo_css::Stylesheet;
use noonoo_dom::{Document, NodeId};

pub use computed::{
    AlignItems, BorderStyle, BoxSizing, ComputedStyle, Cursor, Dimension, Display, Edges,
    FlexDirection, FlexWrap, FontStyle, JustifyContent, Length, LengthOrAuto, Overflow, Position,
    TextAlign, Visibility, WhiteSpace, ZIndex,
};

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

/// Compute every element's style in `document`, cascading the given author
/// stylesheets over the built-in user-agent sheet and inline styles.
pub fn compute(document: &Document, stylesheets: &[Stylesheet]) -> Styles {
    Styles {
        entries: cascade::compute(document, stylesheets),
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
}
