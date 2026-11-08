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
//! [`App::with_fonts`] swaps in another set. Keyboard focus follows pointer *buttons*
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

#[cfg(target_os = "linux")]
pub mod a11y;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use noonoo_dom::NodeId;
use noonoo_events::{
    ElementState, Event, EventPhase, FnListener, Modifiers, MouseButton, PointerEvent, Propagation,
    dispatch,
};
use noonoo_geometry::{Point, Size};
use noonoo_shell::Host;
use noonoo_text::{MetricShaper, Shaper};
use noonoo_ttf::{FontSet, FontSetShaper, OutlineGlyphs};
use winit::application::ApplicationHandler;
use winit::cursor::CursorIcon;
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
    /// Overlays and effects added to every frame; shared with the embedder.
    hooks: frame::FrameHooks,
    /// Set when a hook changed, so the next wake-up draws a frame.
    hooks_changed: Arc<AtomicBool>,
    /// Field order matters: the renderer (holding the surface) must drop
    /// before the window.
    gpu: Option<render::Renderer>,
    window: Option<Box<dyn Window>>,
    /// The Linux AT-SPI adapter, created with the first window.
    #[cfg(target_os = "linux")]
    a11y: Option<a11y::Bridge>,
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
        let app = Self {
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
            hooks: frame::FrameHooks::default(),
            hooks_changed: Arc::new(AtomicBool::new(false)),
            gpu: None,
            window: None,
            #[cfg(target_os = "linux")]
            a11y: None,
        };
        match fonts::system() {
            Some(set) => app.with_fonts(set),
            None => app,
        }
    }

    /// Measure and draw text with the faces in `set`. The shaper and the
    /// glyph renderer always share one set, so text is drawn with the faces
    /// it was measured against.
    pub fn with_fonts(mut self, set: &'static FontSet<'static>) -> Self {
        self.shaper = Box::new(FontSetShaper::new(set));
        self.host.set_glyphs(Box::new(OutlineGlyphs::new(set)));
        self
    }

    /// Measure text with another shaper. Prefer [`App::with_fonts`], which
    /// keeps measuring and drawing on the same faces.
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

    /// The overlays and effects added to every frame. Clone the handle before
    /// [`App::run`] and set them from anywhere; each change asks for a frame.
    pub fn frame_hooks(&self) -> frame::FrameHooks {
        self.hooks.clone()
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
        #[cfg(target_os = "linux")]
        if let Some(window) = &self.window
            && let Some(bridge) = &mut self.a11y
        {
            bridge.set_window_bounds(window.as_ref());
        }
        self.invalidate();
    }

    /// Dispatch a pointer event.
    fn pointer(&mut self, point: Point, event: &Event) {
        self.cursor = point;
        match event {
            Event::PointerDown(_) => {
                // `:active` starts; `dispatch_pointer` routes *and* sets
                // keyboard focus, which is what a press should do.
                self.host.press(point);
                self.press = self.host.dispatch_pointer(point, event, listener!(self));
            }
            Event::PointerUp(pointer) => {
                self.host.release();
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
                self.update_cursor();
            }
            _ => {}
        }
        self.invalidate();
    }

    /// Reflect the hovered node's computed `cursor` on the window.
    fn update_cursor(&mut self) {
        let icon = cursor_icon(self.host.cursor_at(self.cursor));
        if let Some(window) = &self.window {
            window.set_cursor(winit::cursor::Cursor::from(icon));
        }
    }

    /// Lay out (if dirty) and present a frame.
    fn draw(&mut self) {
        if self.dirty {
            self.layout();
            self.dirty = false;
            self.sync_a11y();
        }
        if let Some(error) = self
            .gpu
            .as_mut()
            .and_then(|gpu| gpu.draw(&self.host, self.scale, &self.hooks).err())
        {
            eprintln!("noonoo-winit: {error}");
        }
    }

    /// Push the current accessibility tree to the platform adapter.
    #[cfg(target_os = "linux")]
    fn sync_a11y(&mut self) {
        if self.a11y.is_none() {
            return;
        }
        let tree = self.host.a11y();
        if let Some(bridge) = &mut self.a11y {
            bridge.update(tree);
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn sync_a11y(&mut self) {}

    /// Service accessibility-thread messages on the event-loop thread.
    #[cfg(target_os = "linux")]
    fn pump_a11y(&mut self) {
        let mut messages = Vec::new();
        if let Some(bridge) = &self.a11y {
            while let Some(message) = bridge.try_recv() {
                messages.push(message);
            }
        }
        if messages.is_empty() {
            return;
        }
        for message in messages {
            match message {
                a11y::Message::RequestTree => {}
                a11y::Message::Action(request) => self.a11y_action(request),
            }
        }
        self.sync_a11y();
    }

    #[cfg(not(target_os = "linux"))]
    fn pump_a11y(&mut self) {}

    /// Apply an AccessKit action to the host, on the event-loop thread.
    #[cfg(target_os = "linux")]
    fn a11y_action(&mut self, request: accesskit::ActionRequest) {
        let Some(node) = self
            .a11y
            .as_ref()
            .and_then(|bridge| bridge.node(request.target_node.0))
            .cloned()
        else {
            return;
        };
        let (Some(layer), Some(source)) = (node.layer, node.node) else {
            return;
        };

        match request.action {
            accesskit::Action::Focus => {
                self.host.set_focus(layer, source);
                self.invalidate();
            }
            accesskit::Action::Click => {
                self.host.set_focus(layer, source);
                let point = node
                    .bounds
                    .map(|bounds| {
                        Point::new(
                            bounds.x + bounds.width / 2.0,
                            bounds.y + bounds.height / 2.0,
                        )
                    })
                    .unwrap_or(self.cursor);
                let pointer = PointerEvent {
                    x: f64::from(point.x),
                    y: f64::from(point.y),
                    button: MouseButton::Primary,
                    state: ElementState::Released,
                    modifiers: Modifiers::default(),
                };
                if let Some(layer) = self.host.layer(layer) {
                    dispatch(
                        layer.engine().dom().tree(),
                        source,
                        &Event::Click(pointer),
                        listener!(self),
                    );
                }
                self.invalidate();
            }
            _ => {}
        }
    }
}

/// Noonoo's `cursor` property into a winit cursor icon.
fn cursor_icon(cursor: noonoo_style::Cursor) -> CursorIcon {
    match cursor {
        noonoo_style::Cursor::Pointer => CursorIcon::Pointer,
        noonoo_style::Cursor::Text => CursorIcon::Text,
        noonoo_style::Cursor::Move => CursorIcon::Move,
        noonoo_style::Cursor::NotAllowed => CursorIcon::NotAllowed,
        noonoo_style::Cursor::Auto
        | noonoo_style::Cursor::Default
        | noonoo_style::Cursor::Other(_) => CursorIcon::Default,
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
            let gpu = self.gpu.as_ref().expect("renderer");
            self.hooks
                .attach_gpu(gpu.device().clone(), gpu.queue().clone());
        }
        let proxy = event_loop.create_proxy();
        let changed = self.hooks_changed.clone();
        self.hooks.set_waker(move || {
            changed.store(true, Ordering::Release);
            proxy.wake_up();
        });
        #[cfg(target_os = "linux")]
        if let Some(window) = &self.window {
            let proxy = event_loop.create_proxy();
            let bridge = self
                .a11y
                .get_or_insert_with(move || a11y::Bridge::new(move || proxy.wake_up()));
            bridge.set_window_bounds(window.as_ref());
        }
        self.layout();
        self.update_cursor();
        self.sync_a11y();
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
            WindowEvent::Focused(focused) => {
                #[cfg(target_os = "linux")]
                if let Some(bridge) = &mut self.a11y {
                    bridge.set_window_focus(focused);
                }
                #[cfg(not(target_os = "linux"))]
                let _ = focused;
            }
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
        // Service accessibility requests; the render loop itself is
        // redraw-driven.
        self.pump_a11y();
    }

    fn proxy_wake_up(&mut self, _event_loop: &dyn ActiveEventLoop) {
        // The a11y thread asked the loop to wake so a tree request or action
        // can be serviced promptly, even while the window is otherwise idle.
        self.pump_a11y();
        // A frame hook changed, maybe from another thread: show it.
        if self.hooks_changed.swap(false, Ordering::AcqRel) {
            self.frame();
        }
    }

    fn suspended(&mut self, _event_loop: &dyn ActiveEventLoop) {
        // Drop the surface before the window (field order enforces it), and
        // rebuild on the next `can_create_surfaces`.
        self.hooks.detach_gpu();
        self.gpu = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noonoo_engine::Engine;
    use noonoo_geometry::Rect;
    use noonoo_paint::Command;

    fn app() -> App {
        let mut host = Host::new(Size::new(200.0, 100.0));
        host.add_content(
            Engine::from_html("<p>hello</p>"),
            Rect::new(0.0, 0.0, 200.0, 100.0),
        );
        App::new("test", host, |_, _, _| Propagation::Continue)
    }

    fn glyph_and_text_commands(app: &App) -> (usize, usize) {
        let commands = app.host.layers()[0].commands().commands();
        let glyphs = commands
            .iter()
            .filter(|c| matches!(c, Command::FillContours { .. }))
            .count();
        let runs = commands
            .iter()
            .filter(|c| matches!(c, Command::DrawText { .. }))
            .count();
        (glyphs, runs)
    }

    /// Text reaches the GPU as glyph outlines by default: a window built
    /// with `App::new` draws with the system fonts, with nothing to opt into.
    /// Skipped where no known font family is installed.
    #[test]
    fn text_is_drawn_with_system_fonts_by_default() {
        if fonts::system().is_none() {
            return;
        }
        let mut app = app();
        app.layout();
        let (glyphs, runs) = glyph_and_text_commands(&app);
        assert_eq!(glyphs, "hello".len());
        assert_eq!(runs, 0);
    }
}
