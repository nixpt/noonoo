//! [`View`]: a Noonoo window an embedder's own event loop drives.

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
use winit::cursor::CursorIcon;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowAttributes, WindowId};

#[cfg(target_os = "linux")]
use crate::a11y;
use crate::input::{self, Action};
use crate::{fonts, frame, render};

/// What the embedder should do after a window event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Keep running.
    Continue,
    /// The user asked to close the window.
    Close,
}

type Listener = Box<dyn Fn(NodeId, &Event, EventPhase) -> Propagation>;
type ResizeHook = Box<dyn Fn(&mut Host, Size)>;

/// Wrap the view's listener closure for a dispatch call.
///
/// Written as a macro-ish helper because `FnListener` must wrap a *field*
/// borrow: borrowing `&self` whole would conflict with the `&mut self.host`
/// the dispatch needs.
macro_rules! listener {
    ($view:expr) => {
        &FnListener($view.listener.as_ref())
    };
}

/// A Noonoo window — a [`Host`], its renderer, input state and frame hooks —
/// driven by an event loop someone else owns.
///
/// An embedder with its own [`winit::application::ApplicationHandler`]
/// forwards the loop's callbacks to the methods of the same names
/// ([`View::can_create_surfaces`], [`View::window_event`], …), filtering
/// first whatever it handles itself (shortcuts, say). [`crate::App`] is the
/// stand-alone handler built on one `View`.
pub struct View {
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

impl View {
    /// A window view over `host`, dispatching DOM events to `listener`.
    ///
    /// Text is measured and drawn with the system fonts ([`fonts::system`]):
    /// a GPU window can only draw glyph outlines, so this is the default.
    /// Where no known font is installed, text falls back to the metric
    /// shaper and draws as placeholder boxes.
    pub fn new(
        title: impl Into<String>,
        host: Host,
        listener: impl Fn(NodeId, &Event, EventPhase) -> Propagation + 'static,
    ) -> Self {
        let view = Self {
            title: title.into(),
            host,
            shaper: Box::new(MetricShaper),
            listener: Box::new(listener),
            resize_hook: None,
            cursor: Point::ZERO,
            modifiers: ModifiersState::empty(),
            scale: 1.0,
            press: None,
            // The first update lays the host out.
            dirty: true,
            hooks: frame::FrameHooks::default(),
            hooks_changed: Arc::new(AtomicBool::new(false)),
            gpu: None,
            window: None,
            #[cfg(target_os = "linux")]
            a11y: None,
        };
        match fonts::system() {
            Some(set) => view.with_fonts(set),
            None => view,
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

    /// Measure text with another shaper. Prefer [`View::with_fonts`], which
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
    /// the event loop runs and set them from anywhere; each change asks for a frame.
    pub fn frame_hooks(&self) -> frame::FrameHooks {
        self.hooks.clone()
    }

    /// The host: its layers and their documents.
    pub fn host(&self) -> &Host {
        &self.host
    }

    /// The host, to change in place. Call [`View::invalidate`] after
    /// changing a document so the next frame lays it out.
    pub fn host_mut(&mut self) -> &mut Host {
        &mut self.host
    }

    /// The window, once [`View::can_create_surfaces`] has created it.
    pub fn window(&self) -> Option<&dyn Window> {
        self.window.as_deref()
    }

    /// The window's id, to route an event loop's events to this view.
    pub fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    /// The window's scale factor (physical pixels per logical pixel).
    pub fn scale_factor(&self) -> f64 {
        self.scale
    }

    /// Ask for a frame (laying nothing out unless something changed).
    pub fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Mark the host changed and ask for a frame that lays it out.
    pub fn invalidate(&mut self) {
        self.dirty = true;
        self.request_redraw();
    }

    /// Lay the host out if anything changed since the last update; whether
    /// it did. Each frame does this before drawing.
    pub fn update(&mut self) -> bool {
        if !self.dirty {
            return false;
        }
        self.layout();
        self.dirty = false;
        self.sync_a11y();
        true
    }

    /// Recompute styles, layout, and paint commands for every layer.
    fn layout(&mut self) {
        self.host.layout(self.shaper.as_ref());
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
                // Compare like with like: the press recorded the element
                // under the pointer, so a release on its text still counts.
                let hit = self.host.element_hit(point);
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
        self.update();
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

/// The event loop's callbacks, forwarded by the embedder.
impl View {
    /// Create the window (once) and its GPU surface: call from the event
    /// loop's `can_create_surfaces`.
    pub fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
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
        // The window may not have the size it asked for (a tiling or
        // space-constrained desktop picks its own): lay out and render at
        // the size it got.
        if let Some(window) = &self.window {
            let size = window.surface_size();
            self.resized(size.width, size.height);
        }
        self.dirty = true;
        self.update();
        self.update_cursor();
        self.request_redraw();
    }

    /// Handle one of this view's window events: call from the event loop's
    /// `window_event`. A close request is handed back as [`Flow::Close`];
    /// whether to exit is the embedder's call.
    pub fn window_event(&mut self, event: WindowEvent) -> Flow {
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
            WindowEvent::SurfaceResized(size) => self.resized(size.width, size.height),
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
                // Handled above: sizing runs through `resized`, which
                // applies the scale factor once.
                Some(Action::Resized(_)) => {}
                Some(Action::Redraw) => self.draw(),
                Some(Action::Close) => return Flow::Close,
                None => {}
            },
        }
        Flow::Continue
    }

    /// Service accessibility requests: call from the event loop's
    /// `about_to_wait`.
    pub fn about_to_wait(&mut self) {
        // Service accessibility requests; the render loop itself is
        // redraw-driven.
        self.pump_a11y();
    }

    /// Handle a wake-up from another thread (accessibility, frame hooks):
    /// call from the event loop's `proxy_wake_up`.
    pub fn proxy_wake_up(&mut self) {
        // The a11y thread asked the loop to wake so a tree request or action
        // can be serviced promptly, even while the window is otherwise idle.
        self.pump_a11y();
        // A frame hook changed, maybe from another thread: show it.
        if self.hooks_changed.swap(false, Ordering::AcqRel) {
            self.request_redraw();
        }
    }

    /// Drop the GPU surface: call from the event loop's `suspended`. The
    /// next [`View::can_create_surfaces`] rebuilds it.
    pub fn suspended(&mut self) {
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

    fn view() -> View {
        let mut host = Host::new(Size::new(200.0, 100.0));
        host.add_content(
            Engine::from_html("<p>hello</p>"),
            Rect::new(0.0, 0.0, 200.0, 100.0),
        );
        View::new("test", host, |_, _, _| Propagation::Continue)
    }

    fn glyph_and_text_commands(view: &View) -> (usize, usize) {
        let commands = view.host.layers()[0].commands().commands();
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
    /// with `View::new` draws with the system fonts, with nothing to opt into.
    /// Skipped where no known font family is installed.
    #[test]
    fn text_is_drawn_with_system_fonts_by_default() {
        if fonts::system().is_none() {
            return;
        }
        let mut view = view();
        view.layout();
        let (glyphs, runs) = glyph_and_text_commands(&view);
        assert_eq!(glyphs, "hello".len());
        assert_eq!(runs, 0);
    }
}
