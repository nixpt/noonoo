//! # noonoo-shell
//!
//! The **document host**: several documents in one window, with input routed to
//! the right one and their paint output composited (see `docs/SHELL.md`).
//!
//! Where the engine it replaces had *one root document plus single-slot
//! overlays* — so pages became blitted bitmaps with no scroll/hover/focus — this
//! host treats every document as first-class: each has its own DOM, styles,
//! layout, and paint output, placed at a rectangle in the window. A pointer or
//! key event is routed to the owning document at the right coordinates, and the
//! composition seam returns one command stream per layer.
//!
//! The host model is dependency-free and testable headless. The actual window
//! and GPU surface (`window`/`gpu`) are a borrowed backend behind features, and
//! the a11y tree (`a11y`) is deferred — the model is what this crate delivers.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use noonoo_dom::NodeId;
use noonoo_engine::Engine;
use noonoo_events::{Event, EventListener, dispatch};
use noonoo_geometry::{Point, Rect, Size};
use noonoo_layout::{LayoutTree, Shaper};
use noonoo_paint::CommandList;
use noonoo_style::Styles;

/// A document's trust context.
///
/// Bound to a document when it is admitted to the host and never derived from
/// content. Internal schemes are reachable only from a trusted origin
/// ([`navigation_allowed`]).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Origin {
    /// Trusted host UI (the chrome document).
    Chrome,
    /// Untrusted content (a page).
    Content,
    /// A host-defined trusted origin.
    Custom(String),
}

impl Origin {
    /// Whether this origin may reach internal command schemes.
    pub fn is_trusted(&self) -> bool {
        !matches!(self, Origin::Content)
    }
}

/// Whether `target_scheme` may be navigated to from `origin`.
pub fn navigation_allowed(origin: &Origin, target_scheme: &str) -> bool {
    match target_scheme {
        "shell" | "noonoo" | "exo" => origin.is_trusted(),
        _ => true,
    }
}

/// A layer's role.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayerKind {
    /// The chrome document.
    Chrome,
    /// A content document.
    Content,
}

/// A sub-document's placement: anchored to a host node in a parent layer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Anchor {
    /// The index of the parent layer.
    pub parent: usize,
    /// The host node in the parent document.
    pub host: NodeId,
}

/// One document placed in the window.
pub struct Layer {
    /// The document's trust context.
    pub origin: Origin,
    /// The layer's role.
    pub kind: LayerKind,
    /// Where the document sits in the window (computed from the parent for
    /// sub-documents, fixed otherwise).
    pub rect: Rect,
    anchor: Option<Anchor>,
    engine: Engine,
    styles: Styles,
    layout: LayoutTree,
    commands: CommandList,
}

impl Layer {
    /// The document.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// The computed styles (populated by [`Host::layout`]).
    pub fn styles(&self) -> &Styles {
        &self.styles
    }

    /// The box tree (populated by [`Host::layout`]).
    pub fn layout(&self) -> &LayoutTree {
        &self.layout
    }

    /// The layer's paint commands (populated by [`Host::layout`]).
    pub fn commands(&self) -> &CommandList {
        &self.commands
    }

    /// The sub-document anchor, if this layer is a sub-document.
    pub fn anchor(&self) -> Option<Anchor> {
        self.anchor
    }
}

/// A window hosting several documents.
pub struct Host {
    /// The window size.
    pub viewport: Size,
    layers: Vec<Layer>,
    focus: Option<(usize, NodeId)>,
}

impl Host {
    /// An empty host for a window of `viewport`.
    pub fn new(viewport: Size) -> Self {
        Self {
            viewport,
            layers: Vec::new(),
            focus: None,
        }
    }

    /// Admit the chrome document, placed at `rect`.
    pub fn add_chrome(&mut self, engine: Engine, rect: Rect) -> usize {
        self.push(LayerKind::Chrome, Origin::Chrome, engine, rect)
    }

    /// Admit a content document, placed at `rect`.
    pub fn add_content(&mut self, engine: Engine, rect: Rect) -> usize {
        self.push(LayerKind::Content, Origin::Content, engine, rect)
    }

    fn push(&mut self, kind: LayerKind, origin: Origin, engine: Engine, rect: Rect) -> usize {
        let index = self.layers.len();
        self.layers.push(Layer {
            origin,
            kind,
            rect,
            anchor: None,
            engine,
            styles: Styles::default(),
            layout: LayoutTree::default(),
            commands: CommandList::default(),
        });
        index
    }

    /// Attach `engine` as a sub-document of the host node `host` in `parent`.
    ///
    /// The parent sizes the sub-document: its rectangle is the host node's
    /// content box, computed on the next [`Host::layout`]. Sub-documents are
    /// first-class layers, so routing, hit-testing, and composition apply to
    /// them unchanged.
    pub fn add_sub_document(&mut self, parent: usize, host: NodeId, engine: Engine) -> usize {
        let index = self.layers.len();
        self.layers.push(Layer {
            origin: Origin::Content,
            kind: LayerKind::Content,
            rect: Rect::default(),
            anchor: Some(Anchor { parent, host }),
            engine,
            styles: Styles::default(),
            layout: LayoutTree::default(),
            commands: CommandList::default(),
        });
        index
    }

    /// Every layer, in admission order (later layers paint on top).
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// The layer at `index`.
    pub fn layer(&self, index: usize) -> Option<&Layer> {
        self.layers.get(index)
    }

    /// Compute styles, layout, and paint commands for every layer.
    /// Compute styles, layout, and paint commands for every layer.
    ///
    /// A sub-document's rectangle is taken from its host node's content box in
    /// the (already laid-out) parent, so the parent sizes it; the sub-document
    /// is then laid out and painted with its own styles.
    pub fn layout(&mut self, shaper: &dyn Shaper) {
        for index in 0..self.layers.len() {
            if let Some(anchor) = self.layers[index].anchor {
                let rect = {
                    let parent = &self.layers[anchor.parent];
                    let content = parent
                        .layout
                        .get(anchor.host)
                        .map(|box_| box_.content_box())
                        .unwrap_or_default();
                    Rect::new(
                        parent.rect.x + content.x,
                        parent.rect.y + content.y,
                        content.width,
                        content.height,
                    )
                };
                self.layers[index].rect = rect;
            }

            let layer = &mut self.layers[index];
            let styles = layer.engine.compute_styles();
            let layout =
                noonoo_layout::layout(layer.engine.dom(), &styles, layer.rect.size(), shaper);
            let commands = noonoo_paint::paint(&styles, &layout);
            layer.styles = styles;
            layer.layout = layout;
            layer.commands = commands;
        }
    }

    /// The topmost layer whose rectangle contains `point`.
    pub fn route(&self, point: Point) -> Option<usize> {
        self.layers
            .iter()
            .rposition(|layer| layer.rect.contains(point))
    }

    /// The topmost layer and node under `point`.
    pub fn hit(&self, point: Point) -> Option<(usize, NodeId)> {
        let index = self.route(point)?;
        let layer = &self.layers[index];
        let local = local_point(layer.rect, point);
        let node = layer.layout.hit_test(local)?;
        Some((index, node))
    }

    /// The layer that last received a pointer event.
    pub fn focus(&self) -> Option<(usize, NodeId)> {
        self.focus
    }

    /// Set focus to a specific layer and node.
    pub fn set_focus(&mut self, layer: usize, node: NodeId) {
        self.focus = Some((layer, node));
    }

    /// Route a pointer event to the layer under `point`, updating focus.
    ///
    /// Returns the `(layer, node)` the event was delivered to, if any.
    pub fn dispatch_pointer(
        &mut self,
        point: Point,
        event: &Event,
        listener: &impl EventListener,
    ) -> Option<(usize, NodeId)> {
        let (index, node) = self.hit(point)?;
        self.focus = Some((index, node));
        dispatch(
            self.layers[index].engine().dom().tree(),
            node,
            event,
            listener,
        );
        Some((index, node))
    }

    /// Route a key event to the focused node (or the focused layer's root).
    pub fn dispatch_key(&mut self, event: &Event, listener: &impl EventListener) -> bool {
        let Some((index, node)) = self.focus else {
            return false;
        };
        let layer = &self.layers[index];
        dispatch(layer.engine().dom().tree(), node, event, listener);
        true
    }
}

fn local_point(rect: Rect, point: Point) -> Point {
    Point::new(point.x - rect.x, point.y - rect.y)
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;
    use noonoo_layout::MetricShaper;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn engine(html: &str, css: &str) -> Engine {
        Engine::from_html(&format!("<style>{css}</style>{html}"))
    }

    fn host() -> Host {
        let mut host = Host::new(Size::new(400.0, 300.0));
        host.add_chrome(
            engine(
                r#"<div id="bar">chrome</div>"#,
                "#bar { height: 24px; background-color: #222222 }",
            ),
            Rect::new(0.0, 0.0, 400.0, 24.0),
        );
        host.add_content(
            engine(
                r#"<div id="page">content</div>"#,
                "#page { height: 100px; background-color: #ffffff }",
            ),
            Rect::new(0.0, 24.0, 400.0, 276.0),
        );
        host.layout(&MetricShaper);
        host
    }

    #[test]
    fn routes_by_region() {
        let host = host();
        assert_eq!(host.route(Point::new(10.0, 10.0)), Some(0));
        assert_eq!(host.route(Point::new(10.0, 50.0)), Some(1));
        assert_eq!(host.route(Point::new(10.0, 999.0)), None);
    }

    #[test]
    fn hit_tests_to_the_right_document_and_node() {
        let host = host();
        let (layer, node) = host.hit(Point::new(10.0, 50.0)).unwrap();
        assert_eq!(layer, 1);
        assert_eq!(
            host.layer(1)
                .unwrap()
                .engine()
                .dom()
                .element(node)
                .unwrap()
                .name,
            "div"
        );
    }

    #[test]
    fn pointer_sets_focus_and_dispatches() {
        let mut host = host();
        let seen: Rc<RefCell<Vec<NodeId>>> = Rc::new(RefCell::new(Vec::new()));
        let listener = {
            let seen = seen.clone();
            noonoo_events::FnListener(
                move |node: NodeId, _e: &Event, _p: noonoo_events::EventPhase| {
                    seen.borrow_mut().push(node);
                    noonoo_events::Propagation::Continue
                },
            )
        };
        let routed = host.dispatch_pointer(
            Point::new(10.0, 50.0),
            &Event::Click(noonoo_events::PointerEvent {
                x: 10.0,
                y: 50.0,
                button: noonoo_events::MouseButton::Primary,
                state: noonoo_events::ElementState::Released,
                modifiers: noonoo_events::Modifiers::default(),
            }),
            &listener,
        );
        let (layer, _node) = routed.unwrap();
        assert_eq!(layer, 1);
        assert_eq!(host.focus().unwrap().0, 1);
        assert!(!seen.borrow().is_empty());

        // A key event goes to the focused layer.
        assert!(host.dispatch_key(&Event::Input { value: "a".into() }, &listener));
    }

    #[test]
    fn composes_one_command_stream_per_layer() {
        let host = host();
        assert_eq!(host.layers().len(), 2);
        assert!(!host.layer(0).unwrap().commands().is_empty());
        assert!(!host.layer(1).unwrap().commands().is_empty());
    }

    #[test]
    fn sub_document_is_sized_by_its_host_node() {
        let mut host = Host::new(Size::new(400.0, 300.0));
        let parent = host.add_chrome(
            engine(
                r#"<div id="slot"></div>"#,
                "#slot { width: 200px; height: 100px }",
            ),
            Rect::new(0.0, 0.0, 400.0, 300.0),
        );
        let slot = host
            .layer(parent)
            .unwrap()
            .engine()
            .dom()
            .get_element_by_id("slot")
            .unwrap();
        let sub = engine(
            r#"<p id="emb">embedded</p>"#,
            "p { height: 40px; background-color: #eeeeee }",
        );
        let sub_index = host.add_sub_document(parent, slot, sub);
        host.layout(&MetricShaper);

        // The parent's host node (200×100) sized the sub-document.
        assert_eq!(
            host.layer(sub_index).unwrap().rect,
            Rect::new(0.0, 0.0, 200.0, 100.0)
        );
        // Inside the sub-document, routing and hit-testing select it.
        assert_eq!(host.route(Point::new(10.0, 10.0)), Some(sub_index));
        let (layer, node) = host.hit(Point::new(10.0, 10.0)).unwrap();
        assert_eq!(layer, sub_index);
        // The deepest node is the text run inside the sub-document's <p>.
        assert_eq!(
            host.layer(sub_index)
                .unwrap()
                .engine()
                .dom()
                .node(node)
                .and_then(|data| data.as_text()),
            Some("embedded")
        );
        // It composed its own command stream.
        assert!(!host.layer(sub_index).unwrap().commands().is_empty());
    }

    #[test]
    fn origin_gates_internal_schemes() {
        assert!(navigation_allowed(&Origin::Chrome, "shell"));
        assert!(!navigation_allowed(&Origin::Content, "shell"));
        assert!(navigation_allowed(&Origin::Content, "https"));
        assert!(navigation_allowed(&Origin::Custom("app".into()), "exo"));
    }
}
