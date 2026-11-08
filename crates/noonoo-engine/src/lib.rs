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
use noonoo_dom::{Change, Changes, Document, NodeData, NodeId};
use noonoo_style::{Cascade, Interaction, Styles, Viewport};

mod resources;

/// A page: a DOM document, its stylesheets, and (later) its resolved layout.
pub struct Engine {
    dom: Document,
    stylesheets: Vec<Stylesheet>,
    /// The `<style>` elements the stylesheets came from, in order.
    style_elements: Vec<NodeId>,
    /// The stylesheets prepared for [`Engine::restyle`], once it has run.
    cascade: Option<Cascade>,
    /// Subresources: the loader, what was asked of it, and what arrived.
    resources: resources::Resources,
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
            style_elements: Vec::new(),
            cascade: None,
            resources: resources::Resources::default(),
        }
    }

    /// Parse `html` into a fresh engine, collecting its stylesheets.
    pub fn from_html(html: &str) -> Self {
        let mut engine = Self {
            dom: noonoo_html::parse(html),
            ..Self::new()
        };
        engine.refresh_stylesheets();
        engine
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
        (self.style_elements, self.stylesheets) = collect_stylesheets(&self.dom);
        self.cascade = None;
    }

    /// Bring `styles` up to date with the document, its stylesheets,
    /// `viewport` and `interaction`, restyling only what changed since the
    /// last call. Returns how many times an element was styled.
    ///
    /// Start from [`Styles::default`]: the first call computes everything.
    /// After that, edits made through [`Document::mutate`] restyle only the
    /// subtrees they can reach, and a hover, focus or press moving restyles
    /// the nodes it moved between. An edit to a `<style>` element, a new
    /// viewport, or raw writes through [`Document::tree_mut`] restyle
    /// everything.
    pub fn restyle(
        &mut self,
        styles: &mut Styles,
        viewport: noonoo_geometry::Size,
        interaction: Interaction,
    ) -> usize {
        let changes = self.dom.take_changes();
        if self.touches_stylesheets(&changes) {
            self.refresh_stylesheets();
        }
        let viewport = Viewport {
            width: viewport.width,
            height: viewport.height,
        };
        match &self.cascade {
            Some(cascade) if cascade.viewport() == viewport => {
                cascade.restyle(styles, &self.dom, interaction, &changes)
            }
            _ => {
                let cascade = Cascade::new(&self.stylesheets, viewport);
                *styles = cascade.compute(&self.dom, interaction);
                self.cascade = Some(cascade);
                self.dom.elements().len()
            }
        }
    }

    /// Whether `changes` may have changed what the `<style>` elements say:
    /// text inside one changed, or one was added or removed.
    fn touches_stylesheets(&self, changes: &Changes) -> bool {
        if changes.overflowed() {
            return true;
        }
        let in_style = |node: NodeId| {
            std::iter::once(node)
                .chain(self.dom.tree().ancestors(node))
                .any(|id| is_style(&self.dom, id))
        };
        let mut moved = false;
        for change in changes.iter() {
            match *change {
                // `collect_stylesheets` reads no attributes.
                Change::Attributes(_) => {}
                Change::Text(node) if in_style(node) => return true,
                Change::Text(_) => {}
                Change::Children(node) if in_style(node) => return true,
                Change::Children(_) => moved = true,
            }
        }
        moved && style_elements(&self.dom) != self.style_elements
    }

    /// The `@font-face` descriptors collected from the stylesheets, in source
    /// order.
    ///
    /// A host loads each `src` (`noonoo-net` covers `file:` and `data:`),
    /// parses the bytes with `noonoo-ttf`'s `Font`, and registers the face
    /// with a `FontSet`; see the `noonoo` CLI's `--paint` for the pattern.
    pub fn font_faces(&self) -> Vec<noonoo_css::FontFace> {
        self.stylesheets
            .iter()
            .flat_map(|sheet| sheet.font_faces())
            .collect()
    }

    /// Compute every element's style: cascade the collected stylesheets over the
    /// user-agent sheet and any inline `style` attributes.
    ///
    /// Viewport-relative units resolve against
    /// [`noonoo_style::Viewport::DEFAULT`]; [`Engine::compute_styles_for`] uses a
    /// real viewport instead.
    pub fn compute_styles(&self) -> noonoo_style::Styles {
        noonoo_style::compute(&self.dom, &self.stylesheets)
    }

    /// Compute every element's style against `viewport`, so `vw`/`vh` and
    /// `calc(100vw - 2rem)` resolve correctly.
    pub fn compute_styles_for(&self, viewport: noonoo_geometry::Size) -> noonoo_style::Styles {
        self.compute_styles_in(viewport, noonoo_style::Interaction::default())
    }

    /// Compute every element's style against `viewport` and the interactive
    /// state, so `:hover`/`:focus`/`:active` style the elements they name.
    pub fn compute_styles_in(
        &self,
        viewport: noonoo_geometry::Size,
        interaction: noonoo_style::Interaction,
    ) -> noonoo_style::Styles {
        noonoo_style::compute_interactive(
            &self.dom,
            &self.stylesheets,
            noonoo_style::Viewport {
                width: viewport.width,
                height: viewport.height,
            },
            interaction,
        )
    }

    /// Lay the document out for a viewport, using the given text shaper.
    pub fn layout(
        &self,
        viewport: noonoo_geometry::Size,
        shaper: &dyn noonoo_layout::Shaper,
    ) -> noonoo_layout::LayoutTree {
        let styles = self.compute_styles_for(viewport);
        noonoo_layout::layout_with_images(&self.dom, &styles, viewport, shaper, self.images())
    }

    /// Lay out and paint the document, returning the render command stream.
    pub fn paint(
        &self,
        viewport: noonoo_geometry::Size,
        shaper: &dyn noonoo_layout::Shaper,
    ) -> noonoo_paint::CommandList {
        self.paint_with(viewport, shaper, &noonoo_paint::TextRuns)
    }

    /// Lay out and paint the document, drawing text with `glyphs`.
    ///
    /// Use this with a font-backed shaper and matching glyph renderer to render
    /// vector text (see `noonoo-ttf`'s `FontShaper` + `OutlineGlyphs`).
    pub fn paint_with(
        &self,
        viewport: noonoo_geometry::Size,
        shaper: &dyn noonoo_layout::Shaper,
        glyphs: &dyn noonoo_paint::Glyphs,
    ) -> noonoo_paint::CommandList {
        let styles = self.compute_styles_for(viewport);
        let layout =
            noonoo_layout::layout_with_images(&self.dom, &styles, viewport, shaper, self.images());
        noonoo_paint::paint_with(&styles, &layout, glyphs)
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
/// The document's `<style>` elements, and the stylesheets parsed from them.
fn collect_stylesheets(dom: &Document) -> (Vec<NodeId>, Vec<Stylesheet>) {
    let elements = style_elements(dom);
    let sheets = elements
        .iter()
        .filter_map(|&id| {
            let text = dom.text_content(id);
            if text.trim().is_empty() {
                None
            } else {
                Stylesheet::parse(&text).ok()
            }
        })
        .collect();
    (elements, sheets)
}

fn style_elements(dom: &Document) -> Vec<NodeId> {
    dom.get_elements_by_tag_name("style")
}

fn is_style(dom: &Document, id: NodeId) -> bool {
    matches!(dom.node(id), Some(NodeData::Element(element)) if element.name == "style")
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
    fn collects_font_face_descriptors() {
        let engine = Engine::from_html(
            "<style>@font-face { font-family: A; src: url(a.ttf); font-weight: bold }</style>
             <style>@font-face { font-family: B; src: url(b.ttf) url(b.woff) }</style>",
        );
        let faces = engine.font_faces();
        assert_eq!(faces.len(), 2);
        assert_eq!(faces[0].family, "A");
        assert_eq!(faces[0].weight, 700);
        assert_eq!(faces[1].src, vec!["b.ttf".to_owned(), "b.woff".to_owned()]);
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

    #[test]
    fn paints_a_page() {
        let engine = Engine::from_html(
            "<style>#a { background-color: #102030 }</style><div id=\"a\">hi</div>",
        );
        let list = engine.paint(
            noonoo_geometry::Size::new(200.0, 100.0),
            &noonoo_layout::MetricShaper,
        );
        assert!(!list.is_empty());
    }
}
