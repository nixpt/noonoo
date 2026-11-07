//! # noonoo-headless
//!
//! Run Noonoo with no display. Three deterministic capabilities:
//!
//! 1. **Tree dump** — the tree as JSON, so an agent or a test can assert on
//!    structure without a screenshot ([`tree_json`]). The accessibility tree has
//!    its own dump ([`a11y_json`]) with the same guarantee.
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

use noonoo_a11y::{A11yNode, A11yTree, States};
use noonoo_dom::{Document, NodeData, NodeId};
use noonoo_events::{Event, EventListener, dispatch};
use noonoo_json::Json;
use noonoo_layout::LayoutTree;
use noonoo_style::Styles;

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

/// Build a document's accessibility tree and serialize it to JSON.
///
/// The shape is stable and deterministic: `{"root": …, "focus": …}`, with each
/// node carrying `id`, `role`, `name`, `description`, `value`, `level`, `node`
/// (the source DOM index), `bounds`, `states`, and `children`, in that order.
/// Passing a layout tree adds bounds in page coordinates.
pub fn a11y_json(doc: &Document, styles: &Styles, layout: Option<&LayoutTree>) -> String {
    let tree = noonoo_a11y::build_in(doc, styles, layout, None);
    a11y_tree_json(&tree)
}

/// Serialize an already-built accessibility tree to JSON.
pub fn a11y_tree_json(tree: &A11yTree) -> String {
    host_object(tree).to_string()
}

fn host_object(tree: &A11yTree) -> Json {
    object(vec![
        ("root", a11y_node_json(&tree.root)),
        ("focus", optional_number(tree.focus.map(|id| id as f64))),
    ])
}

fn a11y_node_json(node: &A11yNode) -> Json {
    object(vec![
        ("id", Json::Number(node.id as f64)),
        ("role", Json::String(node.role.as_str().to_owned())),
        ("name", Json::String(node.name.clone())),
        ("description", Json::String(node.description.clone())),
        ("value", optional_string(&node.value)),
        ("level", optional_number(node.level.map(f64::from))),
        (
            "node",
            optional_number(node.node.map(|node| node.index() as f64)),
        ),
        ("bounds", bounds_json(node.bounds)),
        ("states", states_json(&node.states)),
        (
            "children",
            Json::Array(node.children.iter().map(a11y_node_json).collect()),
        ),
    ])
}

fn bounds_json(bounds: Option<noonoo_geometry::Rect>) -> Json {
    match bounds {
        None => Json::Null,
        Some(rect) => object(vec![
            ("x", Json::Number(f64::from(rect.x))),
            ("y", Json::Number(f64::from(rect.y))),
            ("width", Json::Number(f64::from(rect.width))),
            ("height", Json::Number(f64::from(rect.height))),
        ]),
    }
}

fn states_json(states: &States) -> Json {
    object(vec![
        ("disabled", Json::Bool(states.disabled)),
        ("focused", Json::Bool(states.focused)),
        ("selected", Json::Bool(states.selected)),
        ("required", Json::Bool(states.required)),
        ("read-only", Json::Bool(states.read_only)),
        ("busy", Json::Bool(states.busy)),
        (
            "expanded",
            match states.expanded {
                Some(value) => Json::Bool(value),
                None => Json::Null,
            },
        ),
        (
            "checked",
            match states.checked {
                Some(value) => Json::String(value.as_str().to_owned()),
                None => Json::Null,
            },
        ),
    ])
}

fn optional_string(value: &Option<String>) -> Json {
    match value {
        Some(value) => Json::String(value.clone()),
        None => Json::Null,
    }
}

fn optional_number(value: Option<f64>) -> Json {
    match value {
        Some(value) => Json::Number(value),
        None => Json::Null,
    }
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
    fn a11y_json_shape_is_stable() {
        let doc = noonoo_html::parse("<h1 id=\"h\">Hi</h1><button disabled>Save</button>");
        let styles = noonoo_style::compute(&doc, &[]);
        let json = a11y_json(&doc, &styles, None);
        assert!(json.starts_with("{\"root\":{\"id\":1.0,\"role\":\"document\""));
        assert!(json.contains("\"role\":\"heading\""));
        assert!(json.contains("\"level\":1.0"));
        assert!(json.contains("\"disabled\":true"));
        assert!(json.contains("\"focus\":null"));
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
