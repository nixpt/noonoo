//! # noonoo-winit
//!
//! The opt-in **window pass**: run a [`noonoo_shell::Host`] in a real winit
//! window, rendering through `noonoo-vello` onto a wgpu surface, and driving
//! the host with real input.
//!
//! Like the GPU pass, this crate lives **outside the workspace** (root
//! `exclude`) so the engine's default build and lockfile never carry a window
//! or a GPU. See `docs/BACKEND.md` for the pinned versions and the shape.
//!
//! ## Shape
//!
//! - [`input`] translates winit's events into [`noonoo_events::Event`]s —
//!   pure, headless-tested, and the only place that knows both event models.
//! - [`render::Renderer`] owns the wgpu surface and Vello's renderer, composing
//!   one scene per host layer (offset and clipped to the layer's rectangle).
//! - [`View`] is one window: it creates the window and surface in
//!   `can_create_surfaces`, drops the surface in `suspended`, maps input to
//!   `Host` methods, and renders on `RedrawRequested`. An app with its own
//!   winit event loop embeds a `View` and forwards the loop's callbacks to it
//!   (see `examples/embed.rs`); it decides what a close request does.
//! - [`App`] is the stand-alone `ApplicationHandler` around one `View`.
//! - [`frame`] holds what the embedder adds to every frame: scene and texture
//!   overlays and a scene-effects hook, shared through [`frame::FrameHooks`].
//! - [`a11y`] (Linux) bridges the host's accessibility tree to AT-SPI through
//!   AccessKit: the adapter lives here, but the tree and its projection stay in
//!   `noonoo-a11y` / `noonoo-accesskit`.
//!
//! ## What the embedder supplies
//!
//! A [`Host`] already populated with layers and a listener for DOM events.
//! Text is measured and drawn with the system fonts by default ([`fonts`]);
//! [`View::with_fonts`] swaps in another set. Keyboard focus follows pointer *buttons*
//! only; pointer moves route to the DOM without stealing focus, update the
//! host's `:hover` state, and reflect the hovered node's computed `cursor` on
//! the window. On Linux the window is exposed to assistive technology by
//! default; see [`a11y`].

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod fonts;
pub mod frame;
pub mod input;
pub mod render;
pub mod view;

#[cfg(target_os = "linux")]
pub mod a11y;

use noonoo_dom::NodeId;
use noonoo_events::{Event, EventPhase, Propagation};
use noonoo_geometry::Size;
use noonoo_shell::Host;
use noonoo_text::Shaper;
use noonoo_ttf::FontSet;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::WindowId;

pub use input::{Action, LINE_HEIGHT, translate};
pub use view::{Flow, View};

/// A stand-alone Noonoo window: one [`View`] and the event loop driving it.
/// An app with its own event loop embeds a [`View`] instead.
pub struct App {
    view: View,
}

impl App {
    /// A window app over `host`, dispatching DOM events to `listener`.
    ///
    /// Text is measured and drawn with the system fonts ([`fonts::system`]):
    /// a GPU window can only draw glyph outlines, so this is the default.
    /// Where no known font is installed, text falls back to the metric
    /// shaper and isn't drawn.
    pub fn new(
        title: impl Into<String>,
        host: Host,
        listener: impl Fn(NodeId, &Event, EventPhase) -> Propagation + 'static,
    ) -> Self {
        Self {
            view: View::new(title, host, listener),
        }
    }

    /// Measure and draw text with the faces in `set` (see
    /// [`View::with_fonts`]).
    pub fn with_fonts(self, set: &'static FontSet<'static>) -> Self {
        Self {
            view: self.view.with_fonts(set),
        }
    }

    /// Measure text with another shaper (see [`View::with_shaper`]).
    pub fn with_shaper(self, shaper: Box<dyn Shaper>) -> Self {
        Self {
            view: self.view.with_shaper(shaper),
        }
    }

    /// Adjust layer rectangles when the window resizes (see
    /// [`View::with_resize_hook`]).
    pub fn with_resize_hook(self, hook: impl Fn(&mut Host, Size) + 'static) -> Self {
        Self {
            view: self.view.with_resize_hook(hook),
        }
    }

    /// The overlays and effects added to every frame. Clone the handle before
    /// [`App::run`] and set them from anywhere; each change asks for a frame.
    pub fn frame_hooks(&self) -> frame::FrameHooks {
        self.view.frame_hooks()
    }

    /// Run the event loop until the window closes.
    pub fn run(self) -> Result<(), winit::error::EventLoopError> {
        let event_loop = EventLoop::new()?;
        event_loop.run_app(self)
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, _event_loop: &dyn ActiveEventLoop) {
        // Surfaces are (re)created in `can_create_surfaces`; the window itself
        // survives suspension.
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.view.can_create_surfaces(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        if self.view.window_event(event) == Flow::Close {
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, _event_loop: &dyn ActiveEventLoop) {
        self.view.about_to_wait();
    }

    fn proxy_wake_up(&mut self, _event_loop: &dyn ActiveEventLoop) {
        self.view.proxy_wake_up();
    }

    fn suspended(&mut self, _event_loop: &dyn ActiveEventLoop) {
        self.view.suspended();
    }
}
