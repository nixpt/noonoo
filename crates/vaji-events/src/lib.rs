//! # vaji-events
//!
//! The input event model and DOM event dispatch. Both the interactive driver (`vaji-shell`) and the headless one (`vaji-headless`) feed
//! this same representation, so a scripted click and a device click are
//! indistinguishable downstream — which is what makes a scenario reproducible.
//!
//! ## A fix, not a port
//!
//! The replaced engine's event driver called the sink **once**, with the target
//! set to the hit node, and never walked the ancestor chain. A handler
//! registered on an ancestor (the common case for a component wrapper) could
//! therefore never fire, and consumers carried workarounds. [`dispatch`] walks
//! the whole [`dispatch_path`], in capture → target → bubble order, so bubbling
//! works.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use vaji_dom::{NodeId, Tree};

/// Which leg of dispatch an event is on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EventPhase {
    /// Travelling from the root down to the target.
    Capture,
    /// At the target.
    Target,
    /// Travelling from the target back up to the root.
    Bubble,
}

/// A mouse/pointer button.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MouseButton {
    /// The primary (usually left) button.
    Primary,
    /// The secondary (usually right) button.
    Secondary,
    /// The middle (usually wheel) button.
    Middle,
    /// Any other button, by platform code.
    Other(u16),
}

/// Whether a key or button is going down or up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ElementState {
    /// Pressed.
    Pressed,
    /// Released.
    Released,
}

/// Keyboard modifiers held during an event.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Modifiers {
    /// Alt / Option.
    pub alt: bool,
    /// Control.
    pub ctrl: bool,
    /// Shift.
    pub shift: bool,
    /// Meta / Command / Super.
    pub meta: bool,
}

/// A pointer event, in logical pixels relative to the target's viewport.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PointerEvent {
    /// Horizontal position.
    pub x: f64,
    /// Vertical position.
    pub y: f64,
    /// Which button.
    pub button: MouseButton,
    /// Down or up.
    pub state: ElementState,
    /// Modifiers held.
    pub modifiers: Modifiers,
}

/// A keyboard event.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KeyEvent {
    /// A logical key name (e.g. `"Enter"`, `"a"`).
    pub key: String,
    /// Down or up.
    pub state: ElementState,
    /// Modifiers held.
    pub modifiers: Modifiers,
}

/// A wheel/scroll event, in logical pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct WheelEvent {
    /// Horizontal delta.
    pub delta_x: f64,
    /// Vertical delta.
    pub delta_y: f64,
}

/// An input event.
#[derive(Clone, PartialEq, Debug)]
pub enum Event {
    /// Pointer pressed.
    PointerDown(PointerEvent),
    /// Pointer released.
    PointerUp(PointerEvent),
    /// Pointer moved.
    PointerMove(PointerEvent),
    /// A click (press+release on one target).
    Click(PointerEvent),
    /// Key pressed.
    KeyDown(KeyEvent),
    /// Key released.
    KeyUp(KeyEvent),
    /// Wheel scrolled.
    Wheel(WheelEvent),
    /// A text input value changed.
    Input {
        /// The new value.
        value: String,
    },
}

/// What a listener tells the dispatcher to do after handling an event.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Propagation {
    /// Keep dispatching to the next node.
    Continue,
    /// Stop dispatch immediately.
    Stop,
}

/// A receiver of events, called once per node on the dispatch path.
pub trait EventListener {
    /// Handle `event` at `target` during `phase`.
    fn handle(&self, target: NodeId, event: &Event, phase: EventPhase) -> Propagation;
}

/// Adapts a closure into an [`EventListener`].
pub struct FnListener<F>(pub F);

impl<F> EventListener for FnListener<F>
where
    F: Fn(NodeId, &Event, EventPhase) -> Propagation,
{
    fn handle(&self, target: NodeId, event: &Event, phase: EventPhase) -> Propagation {
        (self.0)(target, event, phase)
    }
}

/// The nodes an event visits, in capture → target → bubble order.
pub fn dispatch_path<T>(tree: &Tree<T>, target: NodeId) -> Vec<(NodeId, EventPhase)> {
    let ancestors = tree.ancestors(target); // nearest first

    let mut path: Vec<(NodeId, EventPhase)> = ancestors
        .iter()
        .rev()
        .map(|&id| (id, EventPhase::Capture))
        .collect();
    path.push((target, EventPhase::Target));
    path.extend(ancestors.iter().map(|&id| (id, EventPhase::Bubble)));
    path
}

/// Dispatch `event` from `target` through the whole path.
///
/// Returns `true` if a listener stopped propagation.
pub fn dispatch<T>(
    tree: &Tree<T>,
    target: NodeId,
    event: &Event,
    listener: &impl EventListener,
) -> bool {
    for (node, phase) in dispatch_path(tree, target) {
        if listener.handle(node, event, phase) == Propagation::Stop {
            return true;
        }
    }
    false
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use vaji_dom::Document;

    fn tree_with_chain() -> (Document, NodeId, NodeId, NodeId) {
        let mut doc = Document::new();
        let root = doc.root();
        let mid = doc.create_element("div");
        let leaf = doc.create_element("span");
        doc.mutate().append_child(root, mid).unwrap();
        doc.mutate().append_child(mid, leaf).unwrap();
        (doc, root, mid, leaf)
    }

    #[test]
    fn path_goes_down_then_up() {
        let (doc, root, mid, leaf) = tree_with_chain();
        assert_eq!(
            dispatch_path(doc.tree(), leaf),
            vec![
                (root, EventPhase::Capture),
                (mid, EventPhase::Capture),
                (leaf, EventPhase::Target),
                (mid, EventPhase::Bubble),
                (root, EventPhase::Bubble),
            ]
        );
    }

    #[test]
    fn dispatch_reaches_ancestors() {
        let (doc, root, mid, leaf) = tree_with_chain();
        let seen = RefCell::new(Vec::new());
        let listener = FnListener(|node: NodeId, _event: &Event, phase: EventPhase| {
            seen.borrow_mut().push((node, phase));
            Propagation::Continue
        });
        let stopped = dispatch(
            doc.tree(),
            leaf,
            &Event::Click(PointerEvent {
                x: 1.0,
                y: 2.0,
                button: MouseButton::Primary,
                state: ElementState::Released,
                modifiers: Modifiers::default(),
            }),
            &listener,
        );
        assert!(!stopped);
        let seen = seen.into_inner();
        assert_eq!(seen.len(), 5);
        assert_eq!(seen[0], (root, EventPhase::Capture));
        assert!(seen.contains(&(mid, EventPhase::Bubble)));
    }

    #[test]
    fn a_listener_can_stop_propagation() {
        let (doc, _root, mid, leaf) = tree_with_chain();
        let listener = FnListener(move |node: NodeId, _event: &Event, _phase: EventPhase| {
            if node == mid {
                Propagation::Stop
            } else {
                Propagation::Continue
            }
        });
        let stopped = dispatch(
            doc.tree(),
            leaf,
            &Event::Input { value: "x".into() },
            &listener,
        );
        assert!(stopped);
    }
}
