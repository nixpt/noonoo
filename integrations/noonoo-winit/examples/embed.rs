//! A Noonoo window inside an app that owns its winit event loop.
//!
//! ```sh
//! cargo run --example embed
//! ```
//!
//! The app forwards the loop's callbacks to a [`View`], handles some events
//! itself first (Escape quits), and reacts to link clicks by patching the
//! document: the listener only queues the link, and the app applies it after
//! the event, with its own state in hand.

use std::sync::mpsc::{Receiver, Sender, channel};

use noonoo_engine::Engine;
use noonoo_events::{Event, Propagation};
use noonoo_geometry::{Rect, Size};
use noonoo_shell::Host;
use noonoo_winit::{Flow, View};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowId;

const CHROME: &str = r##"
<div style="background: #22335f; color: #fff; padding: 10px 16px; display: flex; gap: 16px">
  <span id="status">Starting</span>
  <a href="app://count" style="color: #9cf">Count</a>
  <a href="app://reset" style="color: #9cf">Reset</a>
</div>"##;

struct Embedder {
    view: View,
    links: Receiver<String>,
    clicks: u32,
}

impl Embedder {
    fn new() -> Self {
        let mut host = Host::new(Size::new(480.0, 120.0));
        host.add_chrome(Engine::from_html(CHROME), Rect::new(0.0, 0.0, 480.0, 120.0));

        // The listener can't borrow the app, so it queues links for it.
        let (send, links): (Sender<String>, _) = channel();
        let view = View::new("Noonoo embedded", host, move |node, event, _phase| {
            if let Event::Click(_) = event {
                let _ = send.send(format!("{node:?}"));
            }
            Propagation::Continue
        });
        Self {
            view,
            links,
            clicks: 0,
        }
    }

    /// Replace the status text in the chrome document.
    fn set_status(&mut self, text: &str) {
        let engine = self.view.host_mut().layers_mut()[0].engine_mut();
        let Some(status) = engine.dom().get_element_by_id("status") else {
            return;
        };
        if let Some(&child) = engine.dom().tree().children(status).first() {
            let _ = engine.dom_mut().mutate().set_text(child, text);
        }
        self.view.invalidate();
    }

    /// Apply the clicks the listener queued.
    fn drain_links(&mut self) {
        let mut clicked = false;
        while self.links.try_recv().is_ok() {
            clicked = true;
        }
        if clicked {
            self.clicks += 1;
            let text = format!("Clicked {} times", self.clicks);
            self.set_status(&text);
        }
    }
}

impl ApplicationHandler for Embedder {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.view.can_create_surfaces(event_loop);
        self.set_status("Embedded: ready");
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, _: WindowId, event: WindowEvent) {
        // The app's own keys first.
        if let WindowEvent::KeyboardInput { event: key, .. } = &event
            && key.state == ElementState::Pressed
            && key.logical_key == Key::Named(NamedKey::Escape)
        {
            event_loop.exit();
            return;
        }
        if self.view.window_event(event) == Flow::Close {
            event_loop.exit();
        }
        self.drain_links();
    }

    fn about_to_wait(&mut self, _: &dyn ActiveEventLoop) {
        self.view.about_to_wait();
    }

    fn proxy_wake_up(&mut self, _: &dyn ActiveEventLoop) {
        self.view.proxy_wake_up();
    }

    fn suspended(&mut self, _: &dyn ActiveEventLoop) {
        self.view.suspended();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    EventLoop::new()?.run_app(Embedder::new())?;
    Ok(())
}
