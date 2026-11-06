//! A walk-through of the Noonoo stack: an htmx-like swap, then a multi-document
//! host with a sub-document and scroll chaining. Run with:
//!
//! ```sh
//! cargo run -p noonoo --example demo
//! ```

use noonoo::engine::Engine;
use noonoo::events::{ElementState, Event, Modifiers, MouseButton, PointerEvent};
use noonoo::geometry::{Point, Rect, Size};
use noonoo::headless::tree_json;
use noonoo::hx::{Hx, StaticFetcher};
use noonoo::layout::MetricShaper;
use noonoo::shell::Host;

fn click() -> Event {
    Event::Click(PointerEvent {
        x: 0.0,
        y: 0.0,
        button: MouseButton::Primary,
        state: ElementState::Released,
        modifiers: Modifiers::default(),
    })
}

/// The text of each child of `parent`.
fn child_texts(doc: &noonoo::dom::Document, parent: &str) -> Vec<String> {
    let node = doc.get_element_by_id(parent).unwrap();
    doc.tree()
        .children(node)
        .iter()
        .map(|&child| doc.text_content(child))
        .collect()
}

fn main() {
    // ---- 1. htmx-like: a click fetches a fragment and appends it ----------
    println!("== htmx-like swap ==");
    let mut doc = noonoo::html::parse(
        r##"<ul id="items"><li>one</li></ul><button hx-get="/more" hx-target="#items" hx-swap="beforeend">load more</button>"##,
    );
    println!("  items before: {:?}", child_texts(&doc, "items"));

    let fetcher = StaticFetcher::new().with("/more", "<li>two</li>");
    let hx = Hx::new(&fetcher);
    let button = doc.query_selector("button").unwrap().unwrap();

    let action = hx.handle(&mut doc, &click(), button).unwrap().unwrap();
    println!(
        "  click -> {} {} ({}), swap {:?}",
        action.method.as_str(),
        action.url,
        action.status,
        action.swap
    );
    println!("  items after:  {:?}", child_texts(&doc, "items"));

    // The full resolved tree is one JSON line — the headless artifact.
    let json = tree_json(&doc);
    println!("  tree ({} bytes of JSON)", json.len());

    // ---- 2. shell host: chrome + a sub-document, scroll chaining ----------
    println!("\n== multi-document host ==");
    let mut host = Host::new(Size::new(800.0, 600.0));

    // Chrome: a 400x200 slot for a page, plus a tall tail so the chrome itself
    // can scroll.
    let chrome = Engine::from_html(
        r##"<style>#slot { width: 400px; height: 200px; background-color: #eeeeee } #tail { height: 600px }</style><div id="slot"></div><div id="tail">chrome tail</div>"##,
    );
    let parent = host.add_chrome(chrome, Rect::new(0.0, 0.0, 800.0, 600.0));

    // A page embedded in the slot, taller than the slot.
    let slot = host
        .layer(parent)
        .unwrap()
        .engine()
        .dom()
        .get_element_by_id("slot")
        .unwrap();
    let page = Engine::from_html(r##"<div id="page" style="height: 500px">embedded page</div>"##);
    let sub = host.add_sub_document(parent, slot, page);
    host.layout(&MetricShaper);

    println!("  layers: {}", host.layers().len());
    println!(
        "  sub-document rect (sized by the host node): {:?}",
        host.layer(sub).unwrap().rect
    );
    println!(
        "  sub scroll range: {}",
        host.layer(sub).unwrap().max_scroll_y()
    );

    // Wheel deep inside the sub-document: it scrolls, then chains to the parent.
    let chain = host.wheel(Point::new(10.0, 10.0), 400.0);
    println!("  wheel 400 inside sub-doc -> chain {:?}", chain);
    println!("  sub.scroll.y = {}", host.layer(sub).unwrap().scroll().y);
    println!(
        "  parent.scroll.y = {}",
        host.layer(parent).unwrap().scroll().y
    );

    let sub_commands = host.layer(sub).unwrap().commands().len();
    let parent_commands = host.layer(parent).unwrap().commands().len();
    println!("  composed commands: sub {sub_commands}, parent {parent_commands}");
}
