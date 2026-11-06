//! # noonoo-caison
//!
//! A parser for **CAISON** — Crush AI-native Semantic Object Notation — the
//! JSON-shaped format that adds four AI-native primitives: **semantic keys**
//! (`~"intent": v`), **confidence** (`v ~0.95`), **annotations**
//! (`v @name(args) { k: v }`), and **synthesized values**
//! (`@synthesize("…")`).
//!
//! It follows the format spec's grammar (§5) and its **JSON projection** (§6),
//! so any JSON tooling can consume a CAISON document. The projection is
//! lossless in meaning: an unstated confidence is *not* `1.0`, and the two stay
//! distinguishable.
//!
//! Like the other Noonoo parsers this is dependency-free apart from
//! [`noonoo_parse`] and [`noonoo_json`] (used for the projection).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod parser;

use noonoo_json::Json;
use noonoo_parse::{ParseError, ParseResult};

/// A CAISON key: either an exact name or a semantic (fuzzy-match) intent.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Key {
    /// An exact key (`name:` or `"two words":`).
    Exact(String),
    /// A semantic key (`~"intent text":`).
    Semantic(String),
}

impl Key {
    /// The key's text, verbatim.
    pub fn text(&self) -> &str {
        match self {
            Self::Exact(text) | Self::Semantic(text) => text,
        }
    }

    /// Whether the key is semantic.
    pub fn is_semantic(&self) -> bool {
        matches!(self, Self::Semantic(_))
    }
}

/// A CAISON value.
#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    /// `null`.
    Null,
    /// `true` / `false`.
    Bool(bool),
    /// A number (IEEE-754 double).
    Number(f64),
    /// A string.
    String(String),
    /// An object, keys in document order.
    Object(Vec<(Key, Node)>),
    /// An array of nodes.
    Array(Vec<Node>),
    /// A synthesized value: a description to be generated.
    Synthesize(String),
}

/// A node: a value plus optional confidence and annotations.
#[derive(Clone, PartialEq, Debug)]
pub struct Node {
    /// The value.
    pub value: Value,
    /// The producer's confidence, if stated.
    pub confidence: Option<f64>,
    /// Annotations attached to this node.
    pub annotations: Vec<Annotation>,
}

impl Node {
    /// Whether this node carries no metadata.
    pub fn is_bare(&self) -> bool {
        self.confidence.is_none() && self.annotations.is_empty()
    }
}

/// An annotation: `@name(args) { k: v }`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Annotation {
    /// The annotation name, without the leading `@`.
    pub name: String,
    /// Raw parenthesized args, if present.
    pub args: Option<String>,
    /// `key: "value"` properties, in order.
    pub properties: Vec<(String, String)>,
}

impl Annotation {
    /// Look up a property.
    pub fn property(&self, name: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// A parsed CAISON document.
#[derive(Clone, PartialEq, Debug)]
pub struct Document {
    /// The declared format version (default `"1.0"`).
    pub version: String,
    /// The document object.
    pub root: Node,
}

impl Document {
    /// Parse a CAISON document.
    pub fn parse(input: &str) -> ParseResult<Document> {
        parser::parse(input)
    }

    /// Project this document to JSON (spec §6).
    ///
    /// Errors if an object uses a reserved `$`-prefixed bare key.
    pub fn to_json(&self) -> ParseResult<Json> {
        let value = project_value(&self.root, true)?;
        Ok(value)
    }
}

// --- projection ------------------------------------------------------------

fn project_value(node: &Node, top_level: bool) -> ParseResult<Json> {
    let inner = match &node.value {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Number(n) => Json::Number(*n),
        Value::String(s) => Json::String(s.clone()),
        Value::Synthesize(desc) => {
            Json::Object(vec![("$synthesize".to_owned(), Json::String(desc.clone()))])
        }
        Value::Object(pairs) => {
            let mut out = Vec::with_capacity(pairs.len());
            for (key, child) in pairs {
                let name = key.text();
                if top_level && name.starts_with('$') {
                    return Err(ParseError::new(
                        0,
                        format!("key `{name}` uses the reserved `$` prefix"),
                    ));
                }
                out.push((name.to_owned(), project_value(child, false)?));
            }
            Json::Object(out)
        }
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(project_value(item, false)?);
            }
            Json::Array(out)
        }
    };

    if node.is_bare() {
        return Ok(inner);
    }
    let mut wrapper = vec![("$value".to_owned(), inner)];
    if let Some(confidence) = node.confidence {
        wrapper.push(("$confidence".to_owned(), Json::Number(confidence)));
    }
    if !node.annotations.is_empty() {
        wrapper.push((
            "$annotations".to_owned(),
            project_annotations(&node.annotations),
        ));
    }
    Ok(Json::Object(wrapper))
}

fn project_annotations(annotations: &[Annotation]) -> Json {
    Json::Array(
        annotations
            .iter()
            .map(|ann| {
                let args = match &ann.args {
                    Some(raw) => Json::String(raw.clone()),
                    None => Json::Null,
                };
                let properties = Json::Object(
                    ann.properties
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::String(v.clone())))
                        .collect(),
                );
                Json::Object(vec![
                    ("name".to_owned(), Json::String(ann.name.clone())),
                    ("args".to_owned(), args),
                    ("properties".to_owned(), properties),
                ])
            })
            .collect(),
    )
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    fn json(input: &str) -> Json {
        Document::parse(input).unwrap().to_json().unwrap()
    }

    fn expect(input: &str, expected: &str) {
        let got = json(input);
        let want = Json::parse(expected).unwrap();
        assert_eq!(got, want, "\n got: {got}\nwant: {want}");
    }

    #[test]
    fn basic() {
        expect(
            "# a basic CAISON document\nname: \"avalanche\"\nport: 8080\nenabled: true\nextra: null\n",
            r#"{"name":"avalanche","port":8080.0,"enabled":true,"extra":null}"#,
        );
    }

    #[test]
    fn sections() {
        expect(
            "[server]\nhost: \"localhost\"\nport: 8080\n\n[client]\ntimeout: 30\n",
            r#"{"server":{"host":"localhost","port":8080.0},"client":{"timeout":30.0}}"#,
        );
    }

    #[test]
    fn semantic_key() {
        expect(
            "~\"billing or refund issues\": \"route to support\"\n",
            r#"{"billing or refund issues":"route to support"}"#,
        );
    }

    #[test]
    fn synthesize() {
        expect(
            "accent: @synthesize(\"a complimentary color to #3366cc\")\n",
            r#"{"accent":{"$synthesize":"a complimentary color to #3366cc"}}"#,
        );
    }

    #[test]
    fn annotations() {
        expect(
            "port: 8080 @wip { owner: \"foreman\" }\nhost: \"localhost\" @temporary\n",
            r#"{"port":{"$value":8080.0,"$annotations":[{"name":"wip","args":null,"properties":{"owner":"foreman"}}]},"host":{"$value":"localhost","$annotations":[{"name":"temporary","args":null,"properties":{}}]}}"#,
        );
    }

    #[test]
    fn nested_metadata_recurses() {
        expect(
            "server: { host: \"localhost\" ~0.9, port: 8080 }\n",
            r#"{"server":{"host":{"$value":"localhost","$confidence":0.9},"port":8080.0}}"#,
        );
    }

    #[test]
    fn array_elements_are_nodes() {
        expect(
            "tags: [1 ~0.5, 2, \"x\" @note]\n",
            r#"{"tags":[{"$value":1.0,"$confidence":0.5},2.0,{"$value":"x","$annotations":[{"name":"note","args":null,"properties":{}}]}]}"#,
        );
    }

    #[test]
    fn absent_confidence_differs_from_one() {
        expect(
            "unstated: 1\ncertain: 1 ~1.0\n",
            r#"{"unstated":1.0,"certain":{"$value":1.0,"$confidence":1.0}}"#,
        );
    }

    #[test]
    fn metadata_order_is_free() {
        expect(
            "spec_order: 1 ~0.5 @note\nreversed: 2 @note ~0.5\n",
            r#"{"spec_order":{"$value":1.0,"$confidence":0.5,"$annotations":[{"name":"note","args":null,"properties":{}}]},"reversed":{"$value":2.0,"$confidence":0.5,"$annotations":[{"name":"note","args":null,"properties":{}}]}}"#,
        );
    }

    #[test]
    fn document_version() {
        assert_eq!(
            Document::parse("@caison { version: \"1.0\" }\nkey: \"v\"\n")
                .unwrap()
                .version,
            "1.0"
        );
        assert_eq!(
            Document::parse("@cson { version: \"2.0\" }\nkey: \"v\"\n")
                .unwrap()
                .version,
            "2.0"
        );
        assert_eq!(
            Document::parse("@caison(\"1.5\")\nk: 1\n").unwrap().version,
            "1.5"
        );
        assert_eq!(Document::parse("k: 1\n").unwrap().version, "1.0");
    }

    #[test]
    fn invalid_documents_are_rejected() {
        let cases = [
            ("a: 1 ~-0.5\n", "outside"),
            ("a: 1 ~1.5\n", "outside"),
            ("a: 1 ~0.5 ~0.6\n", "confidence"),
            ("a: 1\na: 2\n", "Duplicate key"),
            ("port 8080\n", "colon"),
            ("x: { a: 1\n", "Unclosed"),
            ("[unclosed\nx: 1\n", "Unclosed section"),
            ("x: @synthesize(\"no close\"\n", "Unclosed"),
        ];
        for (input, needle) in cases {
            let err = Document::parse(input).unwrap_err();
            assert!(
                err.message.contains(needle) || err.to_string().contains(needle),
                "input {input:?}: expected error containing {needle:?}, got {err:?}"
            );
        }
    }
}
