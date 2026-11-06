//! # noonoo-headless
//!
//! Run Noonoo with no display. Three deterministic capabilities:
//!
//! 1. **Tree dump** — the tree as JSON, so an agent or a test can assert on
//!    structure without a screenshot ([`tree_json`]).
//! 2. **Virtual clock** — time advances only when the caller says so, so a
//!    frame is reproducible ([`Clock`]).
//! 3. **Synthetic input** — feed [`noonoo_events`] from a script ([`Script`]).
//!
//! This is a first-class crate, not a debug aid: an engine that can be observed
//! and driven without a screen is one an agent can verify. Consumers of the
//! replaced engine built a devtools node registry and a headless browse path
//! precisely because the engine did not offer these.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use noonoo_dom::{Document, NodeData, NodeId};
use noonoo_events::{Event, EventListener, dispatch};
use noonoo_json::Json;

/// Serialize a document's tree to JSON.
///
/// The shape is stable and deterministic: objects with keys in a fixed order,
/// children in document order. Node forms:
///
/// ```json
/// {"kind":"document","children":[…]}
/// {"kind":"element","name":"div","attrs":{"id":"x"},"children":[…]}
/// {"kind":"text","text":"hello"}
/// {"kind":"comment","text":"…"}
/// {"kind":"doctype","name":"html"}
/// ```
pub fn tree_json(doc: &Document) -> String {
    node_json(doc, doc.root()).to_string()
}

fn node_json(doc: &Document, id: NodeId) -> Json {
    match doc.node(id) {
        None => Json::Null,
        Some(NodeData::Document) => object(vec![
            ("kind", Json::String("document".into())),
            ("children", children_json(doc, id)),
        ]),
        Some(NodeData::Doctype { name }) => object(vec![
            ("kind", Json::String("doctype".into())),
            ("name", Json::String(name.clone())),
        ]),
        Some(NodeData::Element(element)) => object(vec![
            ("kind", Json::String("element".into())),
            ("name", Json::String(element.name.clone())),
            (
                "attrs",
                Json::Object(
                    element
                        .attrs
                        .iter()
                        .map(|attr| (attr.name.clone(), Json::String(attr.value.clone())))
                        .collect(),
                ),
            ),
            ("children", children_json(doc, id)),
        ]),
        Some(NodeData::Text(text)) => object(vec![
            ("kind", Json::String("text".into())),
            ("text", Json::String(text.clone())),
        ]),
        Some(NodeData::Comment(text)) => object(vec![
            ("kind", Json::String("comment".into())),
            ("text", Json::String(text.clone())),
        ]),
    }
}

fn children_json(doc: &Document, id: NodeId) -> Json {
    Json::Array(
        doc.tree()
            .children(id)
            .iter()
            .map(|&child| node_json(doc, child))
            .collect(),
    )
}

/// Build a JSON object from ordered key/value pairs.
fn object(pairs: Vec<(&str, Json)>) -> Json {
    Json::Object(
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

/// A virtual clock. Time moves only when [`Clock::advance`] says so.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Clock {
    now_ms: f64,
}

impl Clock {
    /// A clock at time zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// The current time, in milliseconds.
    pub fn now_ms(&self) -> f64 {
        self.now_ms
    }

    /// Advance by `delta_ms`, returning the new time.
    pub fn advance(&mut self, delta_ms: f64) -> f64 {
        self.now_ms += delta_ms;
        self.now_ms
    }

    /// Move directly to absolute time `at_ms`.
    pub fn seek(&mut self, at_ms: f64) {
        self.now_ms = at_ms;
    }

    /// Reset to time zero.
    pub fn reset(&mut self) {
        self.now_ms = 0.0;
    }
}

/// One scheduled event in a [`Script`].
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// When, in milliseconds on the [`Clock`].
    pub at_ms: f64,
    /// The node the event targets.
    pub target: NodeId,
    /// The event to dispatch.
    pub event: Event,
}

/// A deterministic input script: events at times, driven by a [`Clock`].
#[derive(Debug, Default, Clone)]
pub struct Script {
    steps: Vec<Step>,
}

impl Script {
    /// An empty script.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a step (builder style).
    pub fn at(&mut self, at_ms: f64, target: NodeId, event: Event) -> &mut Self {
        self.steps.push(Step {
            at_ms,
            target,
            event,
        });
        self
    }

    /// The scheduled steps, in insertion order.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// Number of steps.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Whether the script has no steps.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Run the script, advancing `clock` to each step's time and dispatching.
    ///
    /// Steps are dispatched in insertion order; if a step's time is in the
    /// past, the clock is left alone (time never runs backwards).
    pub fn run(&self, doc: &Document, clock: &mut Clock, listener: &impl EventListener) {
        for step in &self.steps {
            if step.at_ms > clock.now_ms() {
                clock.advance(step.at_ms - clock.now_ms());
            }
            dispatch(doc.tree(), step.target, &step.event, listener);
        }
    }
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;
    use noonoo_events::{EventPhase, FnListener, Propagation};
    use std::cell::RefCell;

    fn sample() -> Document {
        let mut doc = Document::new();
        let root = doc.root();
        let body = doc.create_element("body");
        doc.mutate().append_child(root, body).unwrap();
        doc.mutate().set_attribute(body, "class", "page").unwrap();
        let text = doc.create_text("hi \"there\"");
        doc.mutate().append_child(body, text).unwrap();
        doc
    }

    #[test]
    fn json_shape_is_stable() {
        let doc = sample();
        let json = tree_json(&doc);
        assert!(json.starts_with("{\"kind\":\"document\",\"children\":"));
        assert!(
            json.contains(
                "{\"kind\":\"element\",\"name\":\"body\",\"attrs\":{\"class\":\"page\"},"
            )
        );
        assert!(json.contains("{\"kind\":\"text\",\"text\":\"hi \\\"there\\\"\"}"));
    }

    #[test]
    fn clock_only_moves_when_told() {
        let mut clock = Clock::new();
        assert_eq!(clock.now_ms(), 0.0);
        assert_eq!(clock.advance(16.0), 16.0);
        assert_eq!(clock.now_ms(), 16.0);
    }

    #[test]
    fn script_dispatches_in_time_and_order() {
        let doc = sample();
        let target = doc.tree().children(doc.root())[0];
        let mut script = Script::new();
        script.at(10.0, target, Event::Input { value: "a".into() });
        script.at(30.0, target, Event::Input { value: "b".into() });

        let seen = RefCell::new(Vec::new());
        let listener = FnListener(|_node: NodeId, event: &Event, phase: EventPhase| {
            if phase == EventPhase::Target
                && let Event::Input { value } = event
            {
                seen.borrow_mut().push(value.clone());
            }
            Propagation::Continue
        });

        let mut clock = Clock::new();
        script.run(&doc, &mut clock, &listener);
        assert_eq!(seen.into_inner(), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(clock.now_ms(), 30.0);
    }
}
