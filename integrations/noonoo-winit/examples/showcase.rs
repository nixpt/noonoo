//! A Noonoo document in a real window.
//!
//! ```sh
//! cargo run -p noonoo-winit --example showcase
//! ```
//!
//! The chrome bar stays pinned to the top; the content layer fills the rest.
//! A click on the button logs to the terminal — the listener is where a host
//! wires DOM events to its own behaviour.

use noonoo_engine::Engine;
use noonoo_events::Propagation;
use noonoo_geometry::{Rect, Size};
use noonoo_shell::Host;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut host = Host::new(Size::new(800.0, 600.0));

    let chrome = Engine::from_html(
        r##"<header style="background: #22335f; color: #fff; padding: 10px 16px">Noonoo</header>"##,
    );
    host.add_chrome(chrome, Rect::new(0.0, 0.0, 800.0, 44.0));

    let content = Engine::from_html(
        r#"<body style="margin: 0">
  <div style="padding: 16px">
    <h1 style="font-size: 24px; margin: 0 0 8px 0">Window pass</h1>
    <p>This document is laid out, painted, and hit-tested by the engine —
       the window only feeds it input and presents its command stream.</p>
    <button id="go" style="padding: 8px 16px">Click me</button>
  </div>
</body>"#,
    );
    host.add_content(content, Rect::new(0.0, 44.0, 800.0, 556.0));

    // Keep the chrome pinned and the content filling the window on resize.
    let app = noonoo_winit::App::new("Noonoo", host, |_node, event, _phase| {
        if let noonoo_events::Event::Click(_) = event {
            println!("clicked");
        }
        Propagation::Continue
    })
    .with_resize_hook(|host, size| {
        for (index, layer) in host.layers_mut().iter_mut().enumerate() {
            layer.rect.width = size.width - layer.rect.x;
            if index > 0 {
                layer.rect.height = size.height - layer.rect.y;
            }
        }
    });

    app.run()?;
    Ok(())
}
