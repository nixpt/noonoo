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
//! Interactive state is host state: [`Host::hover`] tracks the hovered node,
//! [`Host::press`]/[`Host::release`] the pressed one, and [`Host::set_focus`]
//! keyboard focus. [`Host::layout`] feeds that state into the cascade, so
//! `:hover`/`:active`/`:focus` style the elements they name.
//!
//! The host model is dependency-free and testable headless. The actual window
//! and GPU surface (`window`/`gpu`) are a borrowed backend behind features. The
//! accessibility tree is **owned** (`a11y`, on by default): [`Host::a11y`]
//! composes each layer's tree under one window root, with bounds offset and ids
//! renumbered, so a platform bridge (see `integrations/noonoo-accesskit`) can
//! pass it straight to AT-SPI/UiKit/WinRT.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use noonoo_dom::NodeId;
use noonoo_engine::Engine;
use noonoo_events::{Event, EventListener, dispatch};
use noonoo_geometry::{Point, Rect, Size};
use noonoo_layout::{LayoutTree, Shaper};
use noonoo_paint::CommandList;
use noonoo_style::{Cursor, Interaction, Styles};

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
    scroll: Point,
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

    /// The layer's current scroll offset.
    pub fn scroll(&self) -> Point {
        self.scroll
    }

    /// The maximum vertical scroll for this layer (content height minus the
    /// viewport height, floored at zero).
    pub fn max_scroll_y(&self) -> f32 {
        (self.layout.content_height() - self.rect.height).max(0.0)
    }

    /// This layer's accessibility tree, with bounds in the layer's own
    /// coordinates and `focus` marked on the source node.
    #[cfg(feature = "a11y")]
    pub fn a11y(&self, focus: Option<NodeId>) -> noonoo_a11y::A11yTree {
        noonoo_a11y::build_in(self.engine.dom(), self.styles(), Some(self.layout()), focus)
    }
}

/// A window hosting several documents.
pub struct Host {
    /// The window size.
    pub viewport: Size,
    layers: Vec<Layer>,
    focus: Option<(usize, NodeId)>,
    hovered: Option<(usize, NodeId)>,
    active: Option<(usize, NodeId)>,
}

impl Host {
    /// An empty host for a window of `viewport`.
    pub fn new(viewport: Size) -> Self {
        Self {
            viewport,
            layers: Vec::new(),
            focus: None,
            hovered: None,
            active: None,
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
            scroll: Point::ZERO,
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
            scroll: Point::ZERO,
        });
        index
    }

    /// Every layer, in admission order (later layers paint on top).
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// Every layer, mutably — for a host adjusting rectangles on resize.
    pub fn layers_mut(&mut self) -> &mut [Layer] {
        &mut self.layers
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

            let interaction = self.interaction_for(index);
            let layer = &mut self.layers[index];
            let styles = layer
                .engine
                .compute_styles_in(layer.rect.size(), interaction);
            let mut layout =
                noonoo_layout::layout(layer.engine.dom(), &styles, layer.rect.size(), shaper);
            // Preserve what the user scrolled across a re-layout.
            layout.inherit_scroll(&layer.layout);
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

    /// The topmost layer and *element* under `point`: a hit on text resolves
    /// to the element containing it, which is what `:hover`, `:active`,
    /// focus and `cursor` apply to.
    pub fn element_hit(&self, point: Point) -> Option<(usize, NodeId)> {
        let (index, mut node) = self.hit(point)?;
        let dom = self.layers[index].engine().dom();
        while dom.element(node).is_none() {
            node = dom.tree().parent(node)?;
        }
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

    /// Update the hovered node from a hit test at `point`.
    ///
    /// Returns whether it changed — a caller re-layouts to restyle, which is
    /// when `:hover` takes effect.
    pub fn hover(&mut self, point: Point) -> bool {
        let hit = self.element_hit(point);
        if hit != self.hovered {
            self.hovered = hit;
            true
        } else {
            false
        }
    }

    /// The hovered `(layer, node)`, if any.
    pub fn hovered(&self) -> Option<(usize, NodeId)> {
        self.hovered
    }

    /// Mark the node under `point` as pressed (`:active`).
    pub fn press(&mut self, point: Point) {
        self.active = self.element_hit(point);
    }

    /// Release the press (`:active` ends).
    pub fn release(&mut self) {
        self.active = None;
    }

    /// The pressed `(layer, node)`, if any.
    pub fn active(&self) -> Option<(usize, NodeId)> {
        self.active
    }

    /// The computed `cursor` of the node under `point` (`Auto` when nothing
    /// is there). Call after a layout, so the styles are fresh.
    pub fn cursor_at(&self, point: Point) -> Cursor {
        match self.element_hit(point) {
            Some((index, node)) => self.layers[index]
                .styles
                .get(node)
                .map(|style| style.cursor.clone())
                .unwrap_or(Cursor::Auto),
            None => Cursor::Auto,
        }
    }

    /// The interactive state of one layer: only nodes *in* it count.
    fn interaction_for(&self, index: usize) -> Interaction {
        let on_layer = |state: Option<(usize, NodeId)>| {
            state
                .filter(|(layer, _)| *layer == index)
                .map(|(_, node)| node)
        };
        Interaction {
            hovered: on_layer(self.hovered),
            focused: on_layer(self.focus),
            active: on_layer(self.active),
        }
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
        let (index, node) = self.element_hit(point)?;
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

    /// Re-run paint for one layer against its current layout, after a scroll.
    fn repaint(&mut self, index: usize) {
        let layer = &mut self.layers[index];
        layer.commands = noonoo_paint::paint(&layer.styles, &layer.layout);
    }

    /// Apply a wheel (scroll) delta at `point`, chaining outward.
    ///
    /// Inside a layer, the innermost scroll container under `point` scrolls
    /// first; what it cannot consume goes to the next container outward, then to
    /// the layer's own viewport, then — for a sub-document — to its parent.
    ///
    /// Returns `(layer, consumed)` pairs along the layer-level chain, innermost
    /// first. Per-container consumption is visible through
    /// [`Layer::layout`]/[`LayoutTree::scroll_offset`].
    pub fn wheel(&mut self, point: Point, delta_y: f32) -> Vec<(usize, f32)> {
        let mut chain = Vec::new();
        let Some(mut index) = self.route(point) else {
            return chain;
        };
        let mut leftover = delta_y;
        loop {
            // Scroll containers inside this layer first.
            let local = local_point(self.layers[index].rect, point);
            let (boxes, remaining) = self.layers[index].layout.scroll_at(local, leftover);
            if !boxes.is_empty() {
                self.repaint(index);
                leftover = remaining;
            }

            // Then the layer's own viewport scroll.
            let layer = &mut self.layers[index];
            let max = layer.max_scroll_y();
            let before = layer.scroll.y;
            let after = (before + leftover).clamp(0.0, max);
            let consumed = after - before;
            layer.scroll.y = after;
            if consumed != 0.0 {
                chain.push((index, consumed));
            }
            leftover -= consumed;
            if leftover.abs() < 1e-4 {
                break;
            }
            match layer.anchor {
                Some(anchor) => index = anchor.parent,
                None => break,
            }
        }
        chain
    }

    /// The window's accessibility tree: every layer under one `window` root.
    ///
    /// Bounds are translated by each layer's position (minus its scroll), and
    /// nodes are renumbered so ids are unique across the window. The focused
    /// node, if any, is marked and reported as [`noonoo_a11y::A11yTree::focus`].
    #[cfg(feature = "a11y")]
    pub fn a11y(&self) -> noonoo_a11y::A11yTree {
        let mut trees = Vec::with_capacity(self.layers.len());
        for (index, layer) in self.layers.iter().enumerate() {
            let focused = match self.focus {
                Some((layer_index, node)) if layer_index == index => Some(node),
                _ => None,
            };
            let tree = layer.a11y(focused);
            let offset = Point::new(layer.rect.x - layer.scroll.x, layer.rect.y - layer.scroll.y);
            trees.push((tree, offset));
        }
        let layers: Vec<(&noonoo_a11y::A11yTree, Point)> =
            trees.iter().map(|(tree, offset)| (tree, *offset)).collect();
        noonoo_a11y::compose("", &layers)
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
    use noonoo_paint::Color;
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
    fn pointing_at_text_hovers_and_presses_its_element() {
        let mut host = Host::new(Size::new(400.0, 300.0));
        host.add_content(
            engine(
                "<p>hello</p>",
                "body { margin: 0 } p { margin: 0; cursor: pointer }
                 p:hover { background: red }",
            ),
            Rect::new(0.0, 0.0, 400.0, 300.0),
        );
        host.layout(&MetricShaper);
        // The point is on the text "hello", not just its paragraph.
        let point = Point::new(2.0, 2.0);
        let (_, deepest) = host.hit(point).unwrap();
        let dom = host.layer(0).unwrap().engine().dom();
        assert!(dom.element(deepest).is_none(), "the hit is the text node");

        assert_eq!(host.cursor_at(point), Cursor::Pointer);
        host.hover(point);
        host.press(point);
        let p = host.hovered().unwrap().1;
        let dom = host.layer(0).unwrap().engine().dom();
        assert_eq!(dom.element(p).unwrap().name, "p");
        assert_eq!(host.active().unwrap().1, p);
        host.layout(&MetricShaper);
        assert_eq!(
            host.layer(0)
                .unwrap()
                .styles()
                .get(p)
                .unwrap()
                .background_color,
            Color::rgb(255, 0, 0)
        );
    }

    #[test]
    fn hover_and_active_restyle_on_the_next_layout() {
        let mut host = Host::new(Size::new(400.0, 300.0));
        host.add_content(
            engine(
                r#"<button id="b">hi</button>"#,
                "button { cursor: pointer; background: white }
                 button:hover { background: red }
                 button:active { background: green }
                 button:focus { outline: 1px }",
            ),
            Rect::new(0.0, 0.0, 400.0, 300.0),
        );
        host.layout(&MetricShaper);
        let point = Point::new(10.0, 10.0);

        // Nothing hovered: white, but `cursor: pointer` is unconditional.
        assert!(host.hovered().is_none());
        assert_eq!(host.cursor_at(point), Cursor::Pointer);

        // Hover restyles on the next layout and lights up the cursor.
        assert!(host.hover(point), "the first hover is a change");
        host.layout(&MetricShaper);
        let b = host.hovered().unwrap().1;
        assert_eq!(
            host.layer(0)
                .unwrap()
                .styles()
                .get(b)
                .unwrap()
                .background_color,
            Color::rgb(255, 0, 0)
        );
        assert_eq!(host.cursor_at(point), Cursor::Pointer);
        // Hovering the same point again is not a change.
        assert!(!host.hover(point));

        // Pressing goes active; releasing returns to the hover colour.
        host.press(point);
        host.layout(&MetricShaper);
        assert_eq!(
            host.layer(0)
                .unwrap()
                .styles()
                .get(b)
                .unwrap()
                .background_color,
            Color::rgb(0, 128, 0)
        );
        host.release();
        host.layout(&MetricShaper);
        assert_eq!(
            host.layer(0)
                .unwrap()
                .styles()
                .get(b)
                .unwrap()
                .background_color,
            Color::rgb(255, 0, 0)
        );
    }

    #[test]
    fn hover_does_not_leak_across_layers() {
        let mut host = Host::new(Size::new(400.0, 300.0));
        host.add_chrome(
            engine("<div id=\"c\">x</div>", "div:hover { color: red }"),
            Rect::new(0.0, 0.0, 400.0, 50.0),
        );
        host.add_content(
            engine("<div id=\"d\">y</div>", "div { color: black }"),
            Rect::new(0.0, 50.0, 400.0, 250.0),
        );
        host.layout(&MetricShaper);

        // Hovering the content layer must not style the chrome's div.
        assert!(host.hover(Point::new(10.0, 60.0)));
        host.layout(&MetricShaper);
        let c = host
            .layer(0)
            .unwrap()
            .engine()
            .dom()
            .get_element_by_id("c")
            .unwrap();
        assert_eq!(
            host.layer(0).unwrap().styles().get(c).unwrap().color,
            Color::BLACK
        );
    }

    #[test]
    fn wheel_chains_from_a_sub_document_to_its_parent() {
        let mut host = Host::new(Size::new(400.0, 200.0));
        let parent = host.add_chrome(
            engine(
                r#"<div id="slot"></div><div id="tail"></div>"#,
                "#slot { width: 200px; height: 100px } #tail { height: 400px }",
            ),
            Rect::new(0.0, 0.0, 400.0, 200.0),
        );
        let slot = host
            .layer(parent)
            .unwrap()
            .engine()
            .dom()
            .get_element_by_id("slot")
            .unwrap();
        let sub = engine(r#"<div id="long"></div>"#, "#long { height: 300px }");
        let sub_index = host.add_sub_document(parent, slot, sub);
        host.layout(&MetricShaper);

        // Sub viewport 100 over 300 content → 200; parent 200 over 500 → 300.
        assert_eq!(host.layer(sub_index).unwrap().max_scroll_y(), 200.0);
        assert_eq!(host.layer(parent).unwrap().max_scroll_y(), 300.0);

        // 250 down: the sub-document takes 200, the parent takes the rest.
        let chain = host.wheel(Point::new(10.0, 10.0), 250.0);
        assert_eq!(chain, vec![(sub_index, 200.0), (parent, 50.0)]);
        assert_eq!(host.layer(sub_index).unwrap().scroll().y, 200.0);
        assert_eq!(host.layer(parent).unwrap().scroll().y, 50.0);
    }

    #[test]
    fn wheel_does_not_chain_while_the_sub_document_can_scroll() {
        let mut host = Host::new(Size::new(400.0, 200.0));
        let parent = host.add_chrome(
            engine(
                r#"<div id="slot"></div>"#,
                "#slot { width: 200px; height: 100px }",
            ),
            Rect::new(0.0, 0.0, 400.0, 200.0),
        );
        let slot = host
            .layer(parent)
            .unwrap()
            .engine()
            .dom()
            .get_element_by_id("slot")
            .unwrap();
        let sub = engine(r#"<div id="long"></div>"#, "#long { height: 300px }");
        let sub_index = host.add_sub_document(parent, slot, sub);
        host.layout(&MetricShaper);

        let chain = host.wheel(Point::new(10.0, 10.0), 50.0);
        assert_eq!(chain, vec![(sub_index, 50.0)]);
        assert_eq!(host.layer(parent).unwrap().scroll().y, 0.0);
    }

    #[test]
    fn wheel_scrolls_an_inner_container_before_the_layer() {
        let mut host = Host::new(Size::new(400.0, 300.0));
        let index = host.add_content(
            engine(
                r#"<div id="list"><div id="tall"></div></div><div id="tail"></div>"#,
                "#list { width: 200px; height: 50px; overflow-y: auto } \
                 #tall { height: 200px } #tail { height: 600px }",
            ),
            Rect::new(0.0, 0.0, 400.0, 300.0),
        );
        host.layout(&MetricShaper);
        let list = host
            .layer(index)
            .unwrap()
            .engine()
            .dom()
            .get_element_by_id("list")
            .unwrap();
        assert_eq!(host.layer(index).unwrap().layout().scroll_max(list), 150.0);

        // A wheel over the list scrolls the list, not the layer.
        host.wheel(Point::new(10.0, 10.0), 100.0);
        assert_eq!(
            host.layer(index).unwrap().layout().scroll_offset(list),
            100.0
        );
        assert_eq!(host.layer(index).unwrap().scroll().y, 0.0);
    }

    #[test]
    fn wheel_chains_from_a_container_to_the_layer() {
        let mut host = Host::new(Size::new(400.0, 300.0));
        let index = host.add_content(
            engine(
                r#"<div id="list"><div id="tall"></div></div><div id="tail"></div>"#,
                "#list { width: 200px; height: 50px; overflow-y: auto } \
                 #tall { height: 200px } #tail { height: 600px }",
            ),
            Rect::new(0.0, 0.0, 400.0, 300.0),
        );
        host.layout(&MetricShaper);
        let list = host
            .layer(index)
            .unwrap()
            .engine()
            .dom()
            .get_element_by_id("list")
            .unwrap();

        // 200 down: the list takes 150 (its whole range), the layer the rest.
        let chain = host.wheel(Point::new(10.0, 10.0), 200.0);
        assert_eq!(
            host.layer(index).unwrap().layout().scroll_offset(list),
            150.0
        );
        assert_eq!(host.layer(index).unwrap().scroll().y, 50.0);
        assert_eq!(chain, vec![(index, 50.0)]);
    }

    #[cfg(feature = "a11y")]
    #[test]
    fn a11y_composes_layers_with_bounds_and_focus() {
        let mut host = host();
        let node = host
            .layer(1)
            .unwrap()
            .engine()
            .dom()
            .get_element_by_id("page")
            .unwrap();
        host.set_focus(1, node);

        let tree = host.a11y();
        assert_eq!(tree.root.role, noonoo_a11y::Role::Window);
        assert_eq!(tree.root.children.len(), 2);

        let focused = tree.focus.unwrap();
        let node = tree.find_by_id(focused).unwrap();
        assert_eq!(
            node.node,
            Some(
                host.layer(1)
                    .unwrap()
                    .engine()
                    .dom()
                    .get_element_by_id("page")
                    .unwrap()
            )
        );
        assert_eq!(node.layer, Some(1));
        assert_eq!(node.role, noonoo_a11y::Role::Generic);
        assert!(node.bounds.unwrap().y >= 24.0);
    }

    #[test]
    fn origin_gates_internal_schemes() {
        assert!(navigation_allowed(&Origin::Chrome, "shell"));
        assert!(!navigation_allowed(&Origin::Content, "shell"));
        assert!(navigation_allowed(&Origin::Content, "https"));
        assert!(navigation_allowed(&Origin::Custom("app".into()), "exo"));
    }
}
