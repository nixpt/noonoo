//! Loading `<img>` sources through the host's loader, then laying out and
//! painting with what arrived.

use std::sync::{Arc, Mutex};

use noonoo_engine::Engine;
use noonoo_geometry::Size;
use noonoo_layout::MetricShaper;
use noonoo_net::{Completion, LoadError, Loader, Request};
use noonoo_paint::Command;

const VIEW: Size = Size {
    width: 400.0,
    height: 300.0,
};

/// A PNG signature and IHDR chunk for a `width` × `height` image.
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut data = b"\x89PNG\r\n\x1a\n".to_vec();
    data.extend(13u32.to_be_bytes());
    data.extend(b"IHDR");
    data.extend(width.to_be_bytes());
    data.extend(height.to_be_bytes());
    data.extend([8, 6, 0, 0, 0]);
    data
}

/// Holds every request until the test answers it.
#[derive(Clone, Default)]
struct Held {
    requests: Arc<Mutex<Vec<(Request, Completion)>>>,
}

impl Loader for Held {
    fn start(&self, request: Request, completion: Completion) {
        self.requests.lock().unwrap().push((request, completion));
    }
}

impl Held {
    fn urls(&self) -> Vec<String> {
        let mut urls: Vec<String> = self
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|(request, _)| request.url.clone())
            .collect();
        urls.sort();
        urls
    }

    /// Answer the request for `url` with `result`.
    fn answer(&self, url: &str, result: Result<Vec<u8>, LoadError>) {
        let mut requests = self.requests.lock().unwrap();
        let at = requests.iter().position(|(r, _)| r.url == url).unwrap();
        let (_, completion) = requests.remove(at);
        completion.complete(result);
    }
}

fn width_of(engine: &Engine, name: &str) -> f32 {
    let tree = engine.layout(VIEW, &MetricShaper);
    let node = engine.dom().get_element_by_id(name).unwrap();
    tree.get(node).unwrap().content_box().width
}

#[test]
fn sources_resolve_against_the_base_and_load_once() {
    let mut engine = Engine::from_html(
        r#"<img id="a" src="img/a.png"><img src="img/a.png"><img src="/b.png"><img alt="no src"><img src=" ">"#,
    );
    engine.set_base_url("https://site.example/docs/page.html");
    let held = Held::default();
    engine.set_loader(held.clone());
    assert_eq!(engine.load_resources(), 2);
    assert_eq!(
        held.urls(),
        vec![
            "https://site.example/b.png",
            "https://site.example/docs/img/a.png"
        ]
    );
    // Already requested: nothing new starts.
    assert_eq!(engine.load_resources(), 0);
    assert!(!engine.poll_resources());
}

#[test]
fn an_arrived_image_is_laid_out_and_painted() {
    let mut engine = Engine::from_html(r#"<img id="a" src="a.png" alt="pic">"#);
    let held = Held::default();
    engine.set_loader(held.clone());
    engine.load_resources();
    // Still loading: the alt text holds the place, after the 16px icon.
    assert_eq!(width_of(&engine, "a"), 16.0 + 24.0);
    held.answer("a.png", Ok(png(50, 30)));
    assert!(engine.poll_resources(), "a new image changes the layout");
    assert_eq!(width_of(&engine, "a"), 50.0);
    assert!(!engine.poll_resources());
    let list = engine.paint(VIEW, &MetricShaper);
    assert!(
        list.commands()
            .iter()
            .any(|c| matches!(c, Command::DrawImage { rect, .. } if rect.width == 50.0))
    );
}

#[test]
fn failed_and_undecodable_loads_keep_the_alt_text_and_are_not_retried() {
    let mut engine = Engine::from_html(
        r#"<img id="a" src="a.png" alt="pic"><img id="b" src="b.png" alt="pic">"#,
    );
    let held = Held::default();
    engine.set_loader(held.clone());
    assert_eq!(engine.load_resources(), 2);
    held.answer("a.png", Err(LoadError::Io("404".into())));
    held.answer("b.png", Ok(b"not an image".to_vec()));
    assert!(!engine.poll_resources());
    // The alt text, after the 16px icon.
    assert_eq!(width_of(&engine, "a"), 16.0 + 24.0);
    assert_eq!(width_of(&engine, "b"), 16.0 + 24.0);
    assert_eq!(engine.load_resources(), 0);
}

#[test]
fn a_new_image_element_loads_on_the_next_call() {
    let mut engine = Engine::from_html(r#"<body id="body"></body>"#);
    let held = Held::default();
    engine.set_loader(held.clone());
    assert_eq!(engine.load_resources(), 0);
    let body = engine.dom().get_element_by_id("body").unwrap();
    let image = engine
        .dom_mut()
        .create_element_with("img", [("src", "late.png")]);
    engine.dom_mut().mutate().append_child(body, image).unwrap();
    assert_eq!(engine.load_resources(), 1);
    assert_eq!(held.urls(), vec!["late.png"]);
}

#[test]
fn without_a_loader_nothing_loads() {
    let mut engine = Engine::from_html(r#"<img src="a.png">"#);
    assert_eq!(engine.load_resources(), 0);
    assert!(!engine.poll_resources());
}

#[test]
fn the_file_loader_reads_images_beside_the_page() {
    let dir = std::env::temp_dir().join("noonoo-engine-images");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.png"), png(40, 30)).unwrap();
    let mut engine = Engine::from_html(r#"<img id="a" src="a.png">"#);
    engine.set_base_url(format!("file://{}/index.html", dir.display()));
    engine.set_loader(noonoo_net::FileLoader);
    assert_eq!(engine.load_resources(), 1);
    assert!(engine.poll_resources());
    assert_eq!(width_of(&engine, "a"), 40.0);
}

#[test]
fn the_engine_stays_send_and_sync_with_a_loader() {
    fn send<T: Send + Sync>(_: &T) {}
    let mut engine = Engine::new();
    engine.set_loader(noonoo_net::FileLoader);
    send(&engine);
}
