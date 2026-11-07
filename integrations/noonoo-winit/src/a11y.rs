//! The Linux **AT-SPI bridge**: project the window's accessibility tree onto
//! AccessKit and let assistive technology drive it.
//!
//! The tree itself is owned by `noonoo-a11y`; the projection lives in
//! `noonoo-accesskit`; this module is the *lifecycle* — it creates an
//! `accesskit_unix::Adapter`, keeps its root-window bounds current, pushes a
//! full update whenever the host changes, and forwards actions back to the
//! event loop.
//!
//! AccessKit calls its handlers on another thread. Nothing here shares the
//! [`noonoo_shell::Host`] across threads: the handlers only send [`Message`]s
//! over a channel, and [`crate::App`] services them on the event-loop thread.
//! That keeps the DOM single-threaded and the integration free of locks around
//! engine state.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use accesskit::{
    ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, Rect, TreeUpdate,
};
use noonoo_a11y::{A11yNode, A11yTree};
use winit::dpi::PhysicalPosition;
use winit::window::Window;

/// A request from the accessibility thread to the window thread.
pub enum Message {
    /// AccessKit is activating: send a full tree.
    RequestTree,
    /// Assistive technology asked to act on `request.target`.
    Action(ActionRequest),
}

/// The live AT-SPI adapter plus the last tree sent to it.
pub struct Bridge {
    adapter: accesskit_unix::Adapter,
    receiver: Receiver<Message>,
    tree: Option<A11yTree>,
}

impl Bridge {
    /// Create the adapter and start listening on the session bus.
    ///
    /// `wake` is called after a message is queued, so the event loop can service
    /// it even when the window is idle. The winit pass passes
    /// `EventLoopProxy::wake_up`.
    ///
    /// If there is no session bus (a bare TTY, a CI container), the adapter's
    /// background thread simply has nothing to connect to; the window runs
    /// unchanged.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let (sender, receiver) = mpsc::channel();
        let notify = Notify {
            sender,
            wake: Arc::new(wake),
        };
        let adapter = accesskit_unix::Adapter::new(
            Activation(notify.clone()),
            ActionForward(notify),
            Deactivate,
        );
        Self {
            adapter,
            receiver,
            tree: None,
        }
    }

    /// Keep the adapter's screen-coordinate window bounds current.
    ///
    /// Under Wayland the compositor may not report a position; the client-area
    /// origin then falls back to `(0, 0)`, which is the best available answer.
    pub fn set_window_bounds(&mut self, window: &dyn Window) {
        let position = window
            .outer_position()
            .unwrap_or(PhysicalPosition::new(0, 0));
        let surface = window.surface_size();
        let outer = window.outer_size();
        let x = f64::from(position.x);
        let y = f64::from(position.y);
        let inner = Rect {
            x0: x,
            y0: y,
            x1: x + f64::from(surface.width),
            y1: y + f64::from(surface.height),
        };
        let outer = Rect {
            x0: x,
            y0: y,
            x1: x + f64::from(outer.width),
            y1: y + f64::from(outer.height),
        };
        self.adapter.set_root_window_bounds(outer, inner);
    }

    /// Tell AccessKit whether the native window has keyboard focus.
    pub fn set_window_focus(&mut self, focused: bool) {
        self.adapter.update_window_focus_state(focused);
    }

    /// Push the current tree. The first successful update activates the adapter
    /// if a screen reader asked for the tree while it was still pending.
    pub fn update(&mut self, tree: A11yTree) {
        self.tree = Some(tree);
        if let Some(tree) = self.tree.as_ref() {
            self.adapter
                .update_if_active(|| noonoo_accesskit::tree_update(tree));
        }
    }

    /// The next message from the accessibility thread, if any.
    pub fn try_recv(&self) -> Option<Message> {
        self.receiver.try_recv().ok()
    }

    /// The node last sent to AccessKit, by tree-wide identifier.
    pub fn node(&self, id: u64) -> Option<&A11yNode> {
        self.tree.as_ref()?.find_by_id(id)
    }
}

impl Default for Bridge {
    fn default() -> Self {
        Self::new(|| {})
    }
}

/// A channel plus the event-loop wake-up that makes it prompt.
#[derive(Clone)]
struct Notify {
    sender: Sender<Message>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Notify {
    fn send(&self, message: Message) {
        if self.sender.send(message).is_ok() {
            (self.wake)();
        }
    }
}

struct Activation(Notify);

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.0.send(Message::RequestTree);
        // The full tree is built on the event-loop thread; returning `None`
        // leaves the adapter pending until `Bridge::update` supplies it.
        None
    }
}

struct ActionForward(Notify);

impl ActionHandler for ActionForward {
    fn do_action(&mut self, request: ActionRequest) {
        self.0.send(Message::Action(request));
    }
}

struct Deactivate;

impl DeactivationHandler for Deactivate {
    fn deactivate_accessibility(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_records_the_tree_and_exposes_nodes_without_a_session_bus() {
        let doc = noonoo_html::parse("<button id=\"b\">Save</button>");
        let styles = noonoo_style::compute(&doc, &[]);
        let tree = noonoo_a11y::build(&doc, &styles);
        let mut bridge = Bridge::new(|| {});

        bridge.update(tree);
        let id = bridge.node(1).expect("the document root is retained").id;
        assert_eq!(id, 1);
        assert!(bridge.try_recv().is_none());
    }
}
