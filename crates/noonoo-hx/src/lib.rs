//! # noonoo-hx
//!
//! A small, owned **htmx-like** layer: declarative `hx-*` attributes drive a
//! request and a DOM swap, with **no script**.
//!
//! It is cheap here because Noonoo already owns every primitive htmx needs —
//! the DOM with checked mutation, an event system with bubbling, an HTML parser,
//! and a loader seam. This crate is the glue:
//!
//! | attribute | meaning |
//! |---|---|
//! | `hx-get` / `hx-post` / `hx-put` / `hx-delete` | the request URL + method |
//! | `hx-trigger` | the event that fires it (`click`, `change`, `input`, …; default `click`) |
//! | `hx-target` | what to swap (`this`, a selector, `closest S`, `find S`) |
//! | `hx-swap` | `innerHTML` (default), `outerHTML`, `beforebegin`, `afterbegin`, `beforeend`, `afterend`, `delete`, `none` |
//! | `hx-vals` | literal request body for non-GET methods |
//! | `hx-include` | comma/space selectors whose control values join the body |
//!
//! [`Hx::handle`] is called for an event at a node; it walks up to the nearest
//! `hx-*` element whose trigger matches, performs the request through a
//! [`Fetcher`], and swaps the response into the target. [`Hx::mount`] fires
//! `hx-trigger="load"` elements once; [`Hx::tick`] fires `hx-trigger="every Ns"`
//! elements whose interval has elapsed (the caller supplies the clock).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod fetch;

pub use fetch::{Fetcher, HxError, Method, NetFetcher, Request, Response, StaticFetcher};

use noonoo_dom::{Document, NodeData, NodeId, Selector};
use noonoo_events::Event;

use fetch::{Method as M, Request as Req};

/// How a response replaces its target.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Swap {
    /// Replace the target's children.
    InnerHtml,
    /// Replace the target itself.
    OuterHtml,
    /// Insert before the target.
    BeforeBegin,
    /// Insert as the target's first children.
    AfterBegin,
    /// Insert as the target's last children.
    BeforeEnd,
    /// Insert after the target.
    AfterEnd,
    /// Remove the target and insert nothing.
    Delete,
    /// Perform the request but swap nothing.
    None,
}

impl Swap {
    fn parse(spec: &str) -> Swap {
        match spec.trim().to_ascii_lowercase().as_str() {
            "outerhtml" => Swap::OuterHtml,
            "beforebegin" => Swap::BeforeBegin,
            "afterbegin" => Swap::AfterBegin,
            "beforeend" => Swap::BeforeEnd,
            "afterend" => Swap::AfterEnd,
            "delete" => Swap::Delete,
            "none" => Swap::None,
            _ => Swap::InnerHtml,
        }
    }
}

/// What a triggered request did.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Action {
    /// The element whose `hx-*` attribute fired.
    pub element: NodeId,
    /// The method used.
    pub method: Method,
    /// The URL requested.
    pub url: String,
    /// The node that was swapped.
    pub target: NodeId,
    /// How it was swapped.
    pub swap: Swap,
    /// The response status.
    pub status: u16,
}

/// The driver: holds a [`Fetcher`] and interprets `hx-*` attributes.
pub struct Hx<'a> {
    fetcher: &'a dyn Fetcher,
    fired: Vec<(NodeId, f64)>,
}

impl<'a> Hx<'a> {
    /// Build a driver over a fetcher.
    pub fn new(fetcher: &'a dyn Fetcher) -> Self {
        Self {
            fetcher,
            fired: Vec::new(),
        }
    }

    /// Handle `event` delivered at `node`.
    ///
    /// Walks `node` and its ancestors for the nearest `hx-*` element whose
    /// trigger matches; on a match, performs the request and swap. Returns the
    /// [`Action`] performed, if any.
    pub fn handle(
        &self,
        doc: &mut Document,
        event: &Event,
        node: NodeId,
    ) -> Result<Option<Action>, HxError> {
        let mut cursor = Some(node);
        while let Some(current) = cursor {
            if let Some((method, url)) = request_for(doc, current)
                && trigger_matches(doc, current, event)
            {
                return self.perform(doc, current, method, url).map(Some);
            }
            cursor = doc.tree().parent(current);
        }
        Ok(None)
    }

    /// Fire `hx-trigger="load"` elements once, in document order.
    pub fn mount(&mut self, doc: &mut Document) -> Result<Vec<Action>, HxError> {
        let candidates: Vec<(NodeId, M, String)> = doc
            .elements()
            .into_iter()
            .filter_map(|node| {
                let trigger = attr(doc, node, "hx-trigger")?;
                if !trigger.to_ascii_lowercase().contains("load") {
                    return None;
                }
                let (method, url) = request_for(doc, node)?;
                Some((node, method, url))
            })
            .collect();

        let mut actions = Vec::new();
        for (node, method, url) in candidates {
            actions.push(self.perform(doc, node, method, url)?);
        }
        Ok(actions)
    }

    /// Fire any `hx-trigger="every Ns"` element whose interval has elapsed since
    /// it last fired. The caller supplies the clock (the shell's tick, or the
    /// headless virtual clock).
    pub fn tick(&mut self, doc: &mut Document, now_ms: f64) -> Result<Vec<Action>, HxError> {
        let candidates: Vec<(NodeId, f64, M, String)> = doc
            .elements()
            .into_iter()
            .filter_map(|node| {
                let trigger = attr(doc, node, "hx-trigger")?;
                let interval = parse_interval(&trigger)?;
                let (method, url) = request_for(doc, node)?;
                Some((node, interval, method, url))
            })
            .collect();

        let mut actions = Vec::new();
        for (node, interval, method, url) in candidates {
            match self.last(node) {
                // First sighting only arms the timer.
                None => self.set_last(node, now_ms),
                Some(last) if now_ms - last >= interval => {
                    self.set_last(node, now_ms);
                    actions.push(self.perform(doc, node, method, url)?);
                }
                Some(_) => {}
            }
        }
        Ok(actions)
    }

    fn last(&self, node: NodeId) -> Option<f64> {
        self.fired.iter().find(|(n, _)| *n == node).map(|(_, t)| *t)
    }

    fn set_last(&mut self, node: NodeId, at: f64) {
        match self.fired.iter_mut().find(|(n, _)| *n == node) {
            Some(entry) => entry.1 = at,
            None => self.fired.push((node, at)),
        }
    }

    fn perform(
        &self,
        doc: &mut Document,
        element: NodeId,
        method: M,
        url: String,
    ) -> Result<Action, HxError> {
        let request = Req {
            method,
            url: url.clone(),
            body: body_for(doc, element, method),
        };
        let response = self.fetcher.fetch(&request)?;

        let target = resolve_target(doc, element);
        let swap = swap_for(doc, element);
        swap_into(doc, target, swap, &response.body);

        Ok(Action {
            element,
            method,
            url,
            target,
            swap,
            status: response.status,
        })
    }
}

fn attr(doc: &Document, node: NodeId, name: &str) -> Option<String> {
    doc.element(node)?.attr(name).map(str::to_owned)
}

fn request_for(doc: &Document, node: NodeId) -> Option<(M, String)> {
    for (attribute, method) in [
        ("hx-get", M::Get),
        ("hx-post", M::Post),
        ("hx-put", M::Put),
        ("hx-delete", M::Delete),
    ] {
        if let Some(url) = attr(doc, node, attribute) {
            return Some((method, url));
        }
    }
    None
}

fn body_for(doc: &Document, node: NodeId, method: M) -> Option<String> {
    if method == M::Get {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();

    if let Some(vals) = attr(doc, node, "hx-vals") {
        parts.push(vals);
    }

    // `hx-include` pulls the values of other controls into the body.
    if let Some(include) = attr(doc, node, "hx-include") {
        for spec in include.split(|c: char| c == ',' || c.is_whitespace()) {
            if spec.is_empty() {
                continue;
            }
            let Ok(targets) = doc.query_selector_all(spec) else {
                continue;
            };
            for target in targets {
                if let Some(name) = attr(doc, target, "name") {
                    let value =
                        attr(doc, target, "value").unwrap_or_else(|| doc.text_content(target));
                    parts.push(format!("{name}={value}"));
                }
            }
        }
    }

    if parts.is_empty() {
        return attr(doc, node, "value");
    }
    Some(parts.join("&"))
}

fn swap_for(doc: &Document, node: NodeId) -> Swap {
    attr(doc, node, "hx-swap")
        .map(|spec| Swap::parse(&spec))
        .unwrap_or(Swap::InnerHtml)
}

fn event_name(event: &Event) -> &'static str {
    match event {
        Event::Click(_) => "click",
        Event::PointerDown(_) => "pointerdown",
        Event::PointerUp(_) => "pointerup",
        Event::PointerMove(_) => "pointermove",
        Event::KeyDown(_) => "keydown",
        Event::KeyUp(_) => "keyup",
        Event::Wheel(_) => "wheel",
        Event::Input { .. } => "input",
    }
}

fn trigger_matches(doc: &Document, node: NodeId, event: &Event) -> bool {
    let trigger = attr(doc, node, "hx-trigger")
        .unwrap_or_else(|| "click".to_owned())
        .trim()
        .to_ascii_lowercase();
    let name = event_name(event);
    trigger == name || (name == "input" && trigger == "change")
}

/// Parse an `every Ns` / `every Nms` interval from a trigger, in milliseconds.
fn parse_interval(trigger: &str) -> Option<f64> {
    let lower = trigger.trim().to_ascii_lowercase();
    let rest = lower.strip_prefix("every")?.trim();
    if let Some(ms) = rest.strip_suffix("ms") {
        return ms.trim().parse::<f64>().ok();
    }
    rest.strip_suffix('s')
        .and_then(|seconds| seconds.trim().parse::<f64>().ok())
        .map(|value| value * 1000.0)
}

fn matches_selector(doc: &Document, node: NodeId, selector: &str) -> bool {
    let Ok(selector) = Selector::parse(selector) else {
        return false;
    };
    doc.element(node)
        .is_some_and(|element| selector.matches(element))
}

fn resolve_target(doc: &Document, element: NodeId) -> NodeId {
    let Some(spec) = attr(doc, element, "hx-target") else {
        return element;
    };
    let spec = spec.trim();

    if spec.is_empty() || spec == "this" {
        return element;
    }

    if let Some(selector) = spec.strip_prefix("closest ") {
        let mut cursor = Some(element);
        while let Some(node) = cursor {
            if matches_selector(doc, node, selector.trim()) {
                return node;
            }
            cursor = doc.tree().parent(node);
        }
        return element;
    }

    if let Some(selector) = spec.strip_prefix("find ") {
        for node in doc.tree().descendants(element) {
            if matches_selector(doc, node, selector.trim()) {
                return node;
            }
        }
        return element;
    }

    doc.query_selector(spec).ok().flatten().unwrap_or(element)
}

/// Swap `html` into `target` per `swap`, importing nodes from `html`'s tree.
fn swap_into(doc: &mut Document, target: NodeId, swap: Swap, html: &str) {
    match swap {
        Swap::None => return,
        Swap::Delete => {
            let _ = doc.mutate().remove(target);
            return;
        }
        _ => {}
    }

    let source = noonoo_html::parse(html);
    let mut imported = Vec::new();
    for child in source.tree().children(source.root()).to_vec() {
        if let Some(node) = import_node(doc, &source, child) {
            imported.push(node);
        }
    }
    if imported.is_empty() {
        return;
    }

    match swap {
        Swap::InnerHtml => {
            let _ = doc.mutate().replace_children(target, &imported);
        }
        Swap::BeforeEnd => {
            for &node in &imported {
                let _ = doc.mutate().append_child(target, node);
            }
        }
        Swap::AfterBegin => {
            let existing: Vec<NodeId> = doc.tree().children(target).to_vec();
            match existing.first() {
                Some(&first) => {
                    for &node in &imported {
                        let _ = doc.mutate().insert_before(first, node);
                    }
                }
                None => {
                    for &node in &imported {
                        let _ = doc.mutate().append_child(target, node);
                    }
                }
            }
        }
        Swap::BeforeBegin | Swap::AfterEnd | Swap::OuterHtml => {
            let Some(parent) = doc.tree().parent(target) else {
                return;
            };
            let next = doc
                .tree()
                .children(parent)
                .iter()
                .position(|&child| child == target)
                .and_then(|index| doc.tree().children(parent).get(index + 1).copied());
            let anchor = match (swap, next) {
                (Swap::OuterHtml, _) | (Swap::BeforeBegin, _) => Some(target),
                (Swap::AfterEnd, Some(next)) => Some(next),
                _ => None,
            };
            for &node in &imported {
                match anchor {
                    Some(reference) => {
                        let _ = doc.mutate().insert_before(reference, node);
                    }
                    None => {
                        let _ = doc.mutate().append_child(parent, node);
                    }
                }
            }
            if swap == Swap::OuterHtml {
                let _ = doc.mutate().remove(target);
            }
        }
        Swap::Delete | Swap::None => {}
    }
}

fn import_node(dst: &mut Document, src: &Document, node: NodeId) -> Option<NodeId> {
    match src.node(node)? {
        NodeData::Element(element) => {
            let created = dst.create_element(element.name.clone());
            for attribute in &element.attrs {
                let _ =
                    dst.mutate()
                        .set_attribute(created, &attribute.name, attribute.value.clone());
            }
            for child in src.tree().children(node).to_vec() {
                if let Some(imported) = import_node(dst, src, child) {
                    let _ = dst.mutate().append_child(created, imported);
                }
            }
            Some(created)
        }
        NodeData::Text(text) => Some(dst.create_text(text.clone())),
        NodeData::Comment(text) => Some(dst.create_comment(text.clone())),
        _ => None,
    }
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;
    use noonoo_events::{ElementState, Modifiers, MouseButton, PointerEvent};
    use std::cell::RefCell;

    struct Recorder {
        body: String,
        seen: RefCell<Vec<Request>>,
    }

    impl Fetcher for Recorder {
        fn fetch(&self, request: &Request) -> Result<Response, HxError> {
            self.seen.borrow_mut().push(request.clone());
            Ok(Response {
                status: 200,
                body: self.body.clone(),
            })
        }
    }

    fn click() -> Event {
        Event::Click(PointerEvent {
            x: 0.0,
            y: 0.0,
            button: MouseButton::Primary,
            state: ElementState::Released,
            modifiers: Modifiers::default(),
        })
    }

    #[test]
    fn get_swaps_inner_html() {
        let mut doc = noonoo_html::parse(
            r##"<div id="box"><span>old</span></div><button hx-get="/x" hx-target="#box" hx-swap="innerHTML">go</button>"##,
        );
        let fetcher = StaticFetcher::new().with("/x", "<p>new</p>");
        let hx = Hx::new(&fetcher);
        let button = doc.query_selector("button").unwrap().unwrap();

        let action = hx.handle(&mut doc, &click(), button).unwrap().unwrap();
        assert_eq!(action.status, 200);
        assert_eq!(action.method, Method::Get);
        assert_eq!(action.swap, Swap::InnerHtml);

        let box_id = doc.get_element_by_id("box").unwrap();
        let child = doc.tree().children(box_id)[0];
        assert_eq!(doc.element(child).unwrap().name, "p");
        assert!(doc.query_selector("span").unwrap().is_none());
    }

    #[test]
    fn outer_html_swap_replaces_the_element() {
        let mut doc = noonoo_html::parse(
            r##"<div id="a">old</div><button hx-get="/x" hx-target="#a" hx-swap="outerHTML">go</button>"##,
        );
        let fetcher = StaticFetcher::new().with("/x", r##"<section id="a">new</section>"##);
        let hx = Hx::new(&fetcher);
        let button = doc.query_selector("button").unwrap().unwrap();

        hx.handle(&mut doc, &click(), button).unwrap().unwrap();
        let a = doc.get_element_by_id("a").unwrap();
        assert_eq!(doc.element(a).unwrap().name, "section");
    }

    #[test]
    fn trigger_must_match() {
        let mut doc = noonoo_html::parse(
            r##"<input hx-get="/x" hx-trigger="change" hx-target="#out" hx-swap="innerHTML"><div id="out"></div>"##,
        );
        let fetcher = StaticFetcher::new().with("/x", "ok");
        let hx = Hx::new(&fetcher);
        let input = doc.query_selector("input").unwrap().unwrap();

        assert!(hx.handle(&mut doc, &click(), input).unwrap().is_none());
        assert!(
            hx.handle(&mut doc, &Event::Input { value: "v".into() }, input)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn closest_target() {
        let mut doc = noonoo_html::parse(
            r##"<div class="wrap"><button hx-post="/x" hx-target="closest .wrap" hx-swap="innerHTML">go</button></div>"##,
        );
        let fetcher = StaticFetcher::new().with("/x", "hello");
        let hx = Hx::new(&fetcher);
        let button = doc.query_selector("button").unwrap().unwrap();

        let action = hx.handle(&mut doc, &click(), button).unwrap().unwrap();
        assert_eq!(action.method, Method::Post);
        let wrap = doc.query_selector(".wrap").unwrap().unwrap();
        assert_eq!(action.target, wrap);
        // The body defaulted to nothing (no hx-vals), so the response swapped in.
        assert_eq!(doc.element(wrap).unwrap().name, "div");
    }

    #[test]
    fn an_event_with_no_hx_does_nothing() {
        let mut doc = noonoo_html::parse(r##"<div id="x">plain</div>"##);
        let fetcher = StaticFetcher::new();
        let hx = Hx::new(&fetcher);
        let div = doc.get_element_by_id("x").unwrap();
        assert!(hx.handle(&mut doc, &click(), div).unwrap().is_none());
    }

    #[test]
    fn include_pulls_control_values_into_the_body() {
        let mut doc = noonoo_html::parse(
            r##"<input id="q" name="q" value="hello"><button hx-post="/search" hx-include="#q" hx-target="#out">go</button><div id="out"></div>"##,
        );
        let recorder = Recorder {
            body: "ok".to_owned(),
            seen: RefCell::new(Vec::new()),
        };
        let hx = Hx::new(&recorder);
        let button = doc.query_selector("button").unwrap().unwrap();

        hx.handle(&mut doc, &click(), button).unwrap().unwrap();
        let seen = recorder.seen.borrow();
        assert_eq!(seen[0].method, Method::Post);
        assert_eq!(seen[0].body.as_deref(), Some("q=hello"));
    }

    #[test]
    fn swap_delete_removes_the_target() {
        let mut doc = noonoo_html::parse(
            r##"<div id="gone">x</div><button hx-get="/x" hx-target="#gone" hx-swap="delete">go</button>"##,
        );
        let fetcher = StaticFetcher::new().with("/x", "");
        let hx = Hx::new(&fetcher);
        let button = doc.query_selector("button").unwrap().unwrap();

        hx.handle(&mut doc, &click(), button).unwrap().unwrap();
        assert!(doc.get_element_by_id("gone").is_none());
    }

    #[test]
    fn swap_none_requests_without_swapping() {
        let mut doc = noonoo_html::parse(
            r##"<div id="keep">x</div><button hx-get="/x" hx-target="#keep" hx-swap="none">go</button>"##,
        );
        let recorder = Recorder {
            body: String::new(),
            seen: RefCell::new(Vec::new()),
        };
        let hx = Hx::new(&recorder);
        let button = doc.query_selector("button").unwrap().unwrap();

        hx.handle(&mut doc, &click(), button).unwrap().unwrap();
        assert_eq!(recorder.seen.borrow().len(), 1);
        assert!(doc.get_element_by_id("keep").is_some());
    }

    #[test]
    fn mount_fires_load_triggers() {
        let mut doc = noonoo_html::parse(
            r##"<div hx-get="/x" hx-trigger="load" hx-target="#out" hx-swap="innerHTML"></div><div id="out"></div>"##,
        );
        let fetcher = StaticFetcher::new().with("/x", "loaded");
        let mut hx = Hx::new(&fetcher);

        let actions = hx.mount(&mut doc).unwrap();
        assert_eq!(actions.len(), 1);
        let out = doc.get_element_by_id("out").unwrap();
        assert_eq!(doc.text_content(out), "loaded");
    }

    #[test]
    fn tick_fires_every_on_interval() {
        let mut doc = noonoo_html::parse(
            r##"<div hx-get="/x" hx-trigger="every 1s" hx-target="#out" hx-swap="innerHTML"></div><div id="out"></div>"##,
        );
        let fetcher = StaticFetcher::new().with("/x", "tick");
        let mut hx = Hx::new(&fetcher);

        assert!(hx.tick(&mut doc, 0.0).unwrap().is_empty()); // armed
        assert!(hx.tick(&mut doc, 500.0).unwrap().is_empty()); // not due
        assert_eq!(hx.tick(&mut doc, 1000.0).unwrap().len(), 1); // due
        assert!(hx.tick(&mut doc, 1500.0).unwrap().is_empty());
        assert_eq!(hx.tick(&mut doc, 2000.0).unwrap().len(), 1);
    }
}
