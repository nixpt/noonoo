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
//! - [`App`] implements winit's `ApplicationHandler`: it creates the window and
//!   surface in `can_create_surfaces`, drops the surface in `suspended`, maps
//!   input to `Host` methods, and renders on `RedrawRequested`.
//!
//! ## What the embedder supplies
//!
//! A [`Host`] already populated with layers, a listener for DOM events, and —
//! if fonts are wanted — a `Shaper` (the default is the deterministic
//! [`noonoo_text::MetricShaper`]). Keyboard focus follows pointer *buttons*
//! only; pointer moves are routed to the DOM without stealing focus.

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod input;
pub mod render;

use noonoo_dom::NodeId;
use noonoo_events::ElementState;
use noonoo_events::{Event, EventPhase, FnListener, Propagation, dispatch};
use noonoo_geometry::{Point, Size};
use noonoo_shell::Host;
use noonoo_text::{MetricShaper, Shaper};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowAttributes, WindowId};

pub use input::{Action, LINE_HEIGHT, translate};

type Listener = Box<dyn Fn(NodeId, &Event, EventPhase) -> Propagation>;
type ResizeHook = Box<dyn Fn(&mut Host, Size)>;

/// Wrap the app's listener closure for a dispatch call.
///
/// Written as a macro-ish helper because `FnListener` must wrap a *field*
/// borrow: borrowing `&self` whole would conflict with the `&mut self.host`
/// the dispatch needs.
macro_rules! listener {
    ($app:expr) => {
        &FnListener($app.listener.as_ref())
    };
}

/// A Noonoo window: a [`Host`], its renderer, and the event loop driving both.
pub struct App {
    title: String,
    host: Host,
    shaper: Box<dyn Shaper>,
    listener: Listener,
    resize_hook: Option<ResizeHook>,
    /// The pointer position, in logical window coordinates (the wheel's anchor).
    cursor: Point,
    modifiers: ModifiersState,
    scale: f64,
    /// The `(layer, node)` of the last pointer press, for click synthesis.
    press: Option<(usize, NodeId)>,
    /// Whether input has changed anything since the last layout.
    dirty: bool,
    /// Field order matters: the renderer (holding the surface) must drop
    /// before the window.
    gpu: Option<render::Renderer>,
    window: Option<Box<dyn Window>>,
}

impl App {
    /// A window app over `host`, dispatching DOM events to `listener`.
    pub fn new(
        title: impl Into<String>,
        host: Host,
        listener: impl Fn(NodeId, &Event, EventPhase) -> Propagation + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            host,
            shaper: Box::new(MetricShaper),
            listener: Box::new(listener),
            resize_hook: None,
            cursor: Point::ZERO,
            modifiers: ModifiersState::empty(),
            scale: 1.0,
            press: None,
            dirty: false,
            gpu: None,
            window: None,
        }
    }

    /// Measure text with a real shaper (e.g. `noonoo-ttf`'s `FontSetShaper`).
    pub fn with_shaper(mut self, shaper: Box<dyn Shaper>) -> Self {
        self.shaper = shaper;
        self
    }

    /// Adjust layer rectangles when the window resizes. By default only the
    /// host's viewport updates; layer rectangles are the embedder's policy.
    pub fn with_resize_hook(mut self, hook: impl Fn(&mut Host, Size) + 'static) -> Self {
        self.resize_hook = Some(Box::new(hook));
        self
    }

    /// Run the event loop until the window closes.
    pub fn run(self) -> Result<(), winit::error::EventLoopError> {
        let event_loop = EventLoop::new()?;
        event_loop.run_app(self)
    }

    /// Recompute styles, layout, and paint commands for every layer.
    fn layout(&mut self) {
        self.host.layout(self.shaper.as_ref());
    }

    /// Ask for a frame.
    fn frame(&mut self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Mark state changed and ask for a frame that lays it out.
    fn invalidate(&mut self) {
        self.dirty = true;
        self.frame();
    }

    /// Apply a new physical window size.
    fn resized(&mut self, width: u32, height: u32) {
        let size = input::logical_size(winit::dpi::PhysicalSize::new(width, height), self.scale);
        if let Some(hook) = self.resize_hook.take() {
            hook(&mut self.host, size);
            self.resize_hook = Some(hook);
        }
        self.host.viewport = size;
        if let Some(gpu) = &mut self.gpu {
            gpu.resize(width, height);
        }
        self.invalidate();
    }

    /// Dispatch a pointer event.
    fn pointer(&mut self, point: Point, event: &Event) {
        self.cursor = point;
        match event {
            Event::PointerDown(_) => {
                // `dispatch_pointer` routes *and* sets keyboard focus, which is
                // what a press should do.
                self.press = self.host.dispatch_pointer(point, event, listener!(self));
            }
            Event::PointerUp(pointer) => {
                let hit = self.host.hit(point);
                self.host.dispatch_pointer(point, event, listener!(self));
                // A press and a release on one target is a click.
                if self.press.is_some() && self.press == hit {
                    self.host
                        .dispatch_pointer(point, &Event::Click(*pointer), listener!(self));
                }
                self.press = None;
            }
            Event::PointerMove(_) => {
                // Moves reach the DOM but never move keyboard focus.
                if let Some((index, node)) = self.host.hit(point) {
                    let tree = self
                        .host
                        .layer(index)
                        .expect("hit names a live layer")
                        .engine()
                        .dom()
                        .tree();
                    dispatch(tree, node, event, listener!(self));
                }
            }
            _ => {}
        }
        self.invalidate();
    }

    /// Lay out (if dirty) and present a frame.
    fn draw(&mut self) {
        if self.dirty {
            self.layout();
            self.dirty = false;
        }
        if let Some(error) = self.gpu.as_mut().and_then(|gpu| gpu.draw(&self.host).err()) {
            eprintln!("noonoo-winit: {error}");
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, _event_loop: &dyn ActiveEventLoop) {
        // Surfaces are (re)created in `can_create_surfaces`; the window itself
        // survives suspension.
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.window.is_none() {
            let attributes = WindowAttributes::default()
                .with_title(&self.title)
                .with_surface_size(LogicalSize::new(
                    self.host.viewport.width,
                    self.host.viewport.height,
                ));
            let window = event_loop
                .create_window(attributes)
                .expect("noonoo-winit: create window");
            self.scale = window.scale_factor();
            self.window = Some(window);
        }
        if self.gpu.is_none() {
            let window = self.window.as_ref().expect("window");
            self.gpu = Some(
                pollster::block_on(render::Renderer::new(window.as_ref()))
                    .expect("noonoo-winit: create renderer"),
            );
        }
        self.layout();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = scale_factor;
                if let Some(window) = &self.window {
                    let size = window.surface_size();
                    self.resized(size.width, size.height);
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::Destroyed => self.window = None,
            event => match input::translate(&event, self.scale, self.modifiers) {
                Some(Action::Pointer { point, event }) => self.pointer(point, &event),
                Some(Action::Key(key)) => {
                    let event = match key.state {
                        ElementState::Pressed => Event::KeyDown(key),
                        ElementState::Released => Event::KeyUp(key),
                    };
                    self.host.dispatch_key(&event, listener!(self));
                    self.invalidate();
                }
                Some(Action::Wheel { delta_y }) => {
                    self.host.wheel(self.cursor, delta_y);
                    self.invalidate();
                }
                Some(Action::Resized(_)) => {
                    // SurfaceResized carries physical pixels; sizing runs
                    // through `resized` so the scale factor is applied once.
                }
                Some(Action::Redraw) => self.draw(),
                Some(Action::Close) => event_loop.exit(),
                None => {}
            },
        }
    }

    fn about_to_wait(&mut self, _event_loop: &dyn ActiveEventLoop) {
        // The render loop is redraw-driven; nothing to do between events.
    }

    fn suspended(&mut self, _event_loop: &dyn ActiveEventLoop) {
        // Drop the surface before the window (field order enforces it), and
        // rebuild on the next `can_create_surfaces`.
        self.gpu = None;
    }
}
