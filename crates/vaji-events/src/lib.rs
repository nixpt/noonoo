//! # vaji-events
//!
//! The input event model and DOM dispatch. Both the interactive driver
//! (`vaji-shell`) and the headless one (`vaji-headless`) feed this same
//! representation, so a scripted click and a device click are indistinguishable
//! downstream — which is what makes a scenario reproducible.
//!
//! ## A fix, not a port
//!
//! The replaced engine's event driver called the sink **once**, with
//! `current_target` set to the hit node, and did not walk the ancestor chain.
//! A handler registered on an ancestor (the common case for a component
//! wrapper) could therefore never fire, and consumers carried workarounds. Vaji
//! dispatches through the whole [`dispatch_path`], so bubbling works.

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

/// A receiver of DOM events.
pub trait EventSink {
    /// Called once per node on the dispatch path, in [`EventPhase`] order.
    fn on_event(&self, node: NodeId, phase: EventPhase);
}

/// The nodes an event visits, in capture → target → bubble order.
pub fn dispatch_path<T>(tree: &Tree<T>, target: NodeId) -> Vec<(NodeId, EventPhase)> {
    let mut ancestors = Vec::new();
    let mut cursor = tree.get(target).and_then(|node| node.parent);
    while let Some(id) = cursor {
        ancestors.push(id);
        cursor = tree.get(id).and_then(|node| node.parent);
    }

    let mut path: Vec<(NodeId, EventPhase)> = ancestors
        .iter()
        .rev()
        .map(|&id| (id, EventPhase::Capture))
        .collect();
    path.push((target, EventPhase::Target));
    path.extend(ancestors.iter().map(|&id| (id, EventPhase::Bubble)));
    path
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_goes_down_then_up() {
        let mut tree = Tree::new();
        let root = tree.insert(None, "root");
        let mid = tree.insert(Some(root), "mid");
        let leaf = tree.insert(Some(mid), "leaf");

        let path = dispatch_path(&tree, leaf);
        assert_eq!(
            path,
            vec![
                (root, EventPhase::Capture),
                (mid, EventPhase::Capture),
                (leaf, EventPhase::Target),
                (mid, EventPhase::Bubble),
                (root, EventPhase::Bubble),
            ]
        );
    }
}
