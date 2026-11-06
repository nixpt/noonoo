//! # noonoo-engine
//!
//! Orchestration: the crate that gives you a *working* page — the equivalent of
//! the replaced engine's `BaseDocument` / `HtmlDocument`, but assembled from the
//! small crates instead of a monolith.
//!
//! An [`Engine`] owns a [`noonoo_dom::Document`], the [`Stylesheet`]s parsed out
//! of the document, and (once the `resolve` feature is on) the layout caches
//! that keep it presentable. Each of those abilities is a different crate
//! underneath, and — crucially — the parse and style-ingest steps are **ours**:
//! [`noonoo_html`] and [`noonoo_css`], not a borrowed parser.
//!
//! ## The `Send` question
//!
//! The replaced engine's `Document` was `!Send`, which blocked running a page
//! off the UI thread and forced consumers into a texture-overlay workaround for
//! sub-documents. Noonoo's tree owns no `Rc`/`RefCell` and promises nothing
//! unsafe, so an engine is `Send` whenever its payload is.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use noonoo_css::Stylesheet;
use noonoo_dom::Document;

/// A page: a DOM document, its stylesheets, and (later) its resolved layout.
pub struct Engine {
    dom: Document,
    stylesheets: Vec<Stylesheet>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// Create an empty engine with a fresh document.
    pub fn new() -> Self {
        Self {
            dom: Document::new(),
            stylesheets: Vec::new(),
        }
    }

    /// Parse `html` into a fresh engine, collecting its stylesheets.
    pub fn from_html(html: &str) -> Self {
        let dom = noonoo_html::parse(html);
        let stylesheets = collect_stylesheets(&dom);
        Self { dom, stylesheets }
    }

    /// Borrow the underlying DOM document.
    pub fn dom(&self) -> &Document {
        &self.dom
    }

    /// Mutably borrow the underlying DOM document.
    pub fn dom_mut(&mut self) -> &mut Document {
        &mut self.dom
    }

    /// The stylesheets collected from the document's `<style>` elements.
    pub fn stylesheets(&self) -> &[Stylesheet] {
        &self.stylesheets
    }

    /// Re-scan the document's `<style>` elements for stylesheets.
    pub fn refresh_stylesheets(&mut self) {
        self.stylesheets = collect_stylesheets(&self.dom);
    }

    /// Compute every element's style: cascade the collected stylesheets over the
    /// user-agent sheet and any inline `style` attributes.
    pub fn compute_styles(&self) -> noonoo_style::Styles {
        noonoo_style::compute(&self.dom, &self.stylesheets)
    }

    /// Lay the document out for a viewport, using the given text shaper.
    pub fn layout(
        &self,
        viewport: noonoo_geometry::Size,
        shaper: &dyn noonoo_layout::Shaper,
    ) -> noonoo_layout::LayoutTree {
        let styles = self.compute_styles();
        noonoo_layout::layout(&self.dom, &styles, viewport, shaper)
    }

    /// Consume the engine and return its document.
    pub fn into_dom(self) -> Document {
        self.dom
    }
}

/// Collect and parse the `<style>` elements in a document.
///
/// A `<style>` whose text is empty or fails to parse is skipped; a page with a
/// broken stylesheet still yields its DOM.
fn collect_stylesheets(dom: &Document) -> Vec<Stylesheet> {
    dom.get_elements_by_tag_name("style")
        .into_iter()
        .filter_map(|id| {
            let text = dom.text_content(id);
            if text.trim().is_empty() {
                None
            } else {
                Stylesheet::parse(&text).ok()
            }
        })
        .collect()
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_engine_has_a_document_root() {
        let engine = Engine::new();
        let root = engine.dom().root();
        assert!(engine.dom().tree().contains(root));
        assert!(engine.stylesheets().is_empty());
    }

    #[test]
    fn from_html_parses_the_tree() {
        let engine = Engine::from_html("<div id=\"box\"><span>hi</span></div>");
        let box_id = engine.dom().get_element_by_id("box").unwrap();
        assert_eq!(engine.dom().element(box_id).unwrap().name, "div");
    }

    #[test]
    fn collects_style_sheets() {
        let engine = Engine::from_html("<style>body { color: red; margin: 0 }</style><p>hi</p>");
        assert_eq!(engine.stylesheets().len(), 1);
        assert_eq!(engine.stylesheets()[0].rules.len(), 1);
    }

    #[test]
    fn skips_empty_and_broken_stylesheets() {
        let engine = Engine::from_html(
            "<style></style><style>a { color: red</style><style>b { color: blue }</style>",
        );
        // The empty one is dropped, the unclosed one fails to parse, the valid one stays.
        assert_eq!(engine.stylesheets().len(), 1);
    }

    #[test]
    fn computes_styles_for_a_page() {
        let engine = Engine::from_html(
            "<style>p { display: flex; color: red; margin: 5px }</style><p>x</p>",
        );
        let styles = engine.compute_styles();
        let p = engine.dom().get_elements_by_tag_name("p")[0];
        let style = styles.get(p).unwrap();
        assert_eq!(style.display, noonoo_style::Display::Flex);
        assert_eq!(style.color, noonoo_css::Color::rgb(255, 0, 0));
        assert_eq!(style.margin.top, noonoo_style::Dimension::Px(5.0));
    }

    #[test]
    fn lays_out_a_page() {
        let engine = Engine::from_html("<style>#a { height: 40px }</style><div id=\"a\"></div>");
        let tree = engine.layout(
            noonoo_geometry::Size::new(300.0, 200.0),
            &noonoo_layout::MetricShaper,
        );
        let a = engine.dom().get_element_by_id("a").unwrap();
        let box_ = tree.get(a).unwrap();
        assert_eq!(box_.rect.width, 300.0);
        assert_eq!(box_.rect.height, 40.0);
    }
}
