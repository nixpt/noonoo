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

use noonoo_json::Json;
use noonoo_parse::{Cursor, ParseError, ParseResult};

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
        Parser::new(input).parse_document()
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

// --- parsing ---------------------------------------------------------------

struct Parser<'a> {
    cursor: Cursor<'a>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            cursor: Cursor::new(input),
        }
    }

    fn parse_document(&mut self) -> ParseResult<Document> {
        let mut root: Vec<(Key, Node)> = Vec::new();
        let mut current_section: Option<String> = None;
        let mut pending: Vec<Annotation> = Vec::new();
        let mut version = "1.0".to_owned();

        while !self.cursor.is_eof() {
            self.skip_trivia();
            if self.cursor.is_eof() {
                break;
            }

            if self.cursor.peek() == Some('@') {
                let annotation = self.parse_annotation()?;
                if annotation.name == "caison" || annotation.name == "cson" {
                    if let Some(v) = annotation.property("version") {
                        version = v.to_owned();
                    } else if let Some(args) = &annotation.args {
                        version = args.trim().trim_matches('"').to_owned();
                    }
                    continue;
                }
                pending.push(annotation);
                continue;
            }

            if self.cursor.peek() == Some('[') {
                self.cursor.bump();
                let start = self.cursor.position();
                self.cursor.skip_while(|ch| ch != ']' && ch != '\n');
                if !self.cursor.eat(']') {
                    return Err(ParseError::new(start, "Unclosed section: expected `]`"));
                }
                let name = self.cursor.input()[start..self.cursor.position() - 1]
                    .trim()
                    .to_owned();
                current_section = Some(name);
                continue;
            }

            let Some((key, mut node)) = self.parse_pair()? else {
                break;
            };
            // Pending annotations were declared before this pair; append after
            // the pair's own inline annotations (spec order).
            node.annotations.append(&mut pending);

            match &current_section {
                Some(section) => {
                    let section = section.clone();
                    let entry = root
                        .iter_mut()
                        .find(|(k, _)| k.text() == section)
                        .map(|(_, n)| n);
                    let object = match entry {
                        Some(node) => match &mut node.value {
                            Value::Object(pairs) => pairs,
                            _ => unreachable!("section is always an object"),
                        },
                        None => {
                            root.push((
                                Key::Exact(section),
                                Node {
                                    value: Value::Object(Vec::new()),
                                    confidence: None,
                                    annotations: Vec::new(),
                                },
                            ));
                            match &mut root.last_mut().unwrap().1.value {
                                Value::Object(pairs) => pairs,
                                _ => unreachable!(),
                            }
                        }
                    };
                    push_pair(object, key, node)?;
                }
                None => push_pair(&mut root, key, node)?,
            }
        }

        Ok(Document {
            version,
            root: Node {
                value: Value::Object(root),
                confidence: None,
                annotations: pending,
            },
        })
    }

    /// Parse `key : node`, or return `None` at a structural boundary.
    fn parse_pair(&mut self) -> ParseResult<Option<(Key, Node)>> {
        self.skip_trivia();
        match self.cursor.peek() {
            None | Some('[') | Some('@') | Some('}') | Some(']') => return Ok(None),
            _ => {}
        }

        let semantic = self.cursor.eat('~');
        let key_text = if self.cursor.peek() == Some('"') {
            self.parse_string()?
        } else {
            let start = self.cursor.position();
            self.cursor.skip_while(|ch| ch != ':' && ch != '\n');
            let text = self.cursor.input()[start..self.cursor.position()]
                .trim()
                .to_owned();
            if self.cursor.peek() != Some(':') {
                return Err(self.cursor.error("Missing colon in key/value pair"));
            }
            text
        };

        self.skip_trivia();
        if !self.cursor.eat(':') {
            return Err(self.cursor.error("Expected `:` after key"));
        }
        self.skip_trivia();

        let node = self.parse_node()?;
        let key = if semantic {
            Key::Semantic(key_text)
        } else {
            Key::Exact(key_text)
        };
        Ok(Some((key, node)))
    }

    /// Parse a value plus its trailing metadata (confidence / annotations).
    fn parse_node(&mut self) -> ParseResult<Node> {
        let value = self.parse_value()?;
        let mut confidence = self.parse_confidence()?;
        let mut annotations = Vec::new();
        loop {
            self.skip_trivia();
            if self.cursor.peek() == Some('@') && !self.cursor.starts_with("@synthesize") {
                annotations.push(self.parse_annotation()?);
                continue;
            }
            match self.parse_confidence()? {
                Some(found) => {
                    if confidence.is_some() {
                        return Err(self.cursor.error("Duplicate confidence on a node"));
                    }
                    confidence = Some(found);
                }
                None => break,
            }
        }
        Ok(Node {
            value,
            confidence,
            annotations,
        })
    }

    fn parse_value(&mut self) -> ParseResult<Value> {
        self.skip_trivia();
        match self.cursor.peek() {
            None => Err(self.cursor.error("Unexpected end of input in value")),
            Some('@') => {
                let annotation = self.parse_annotation()?;
                if annotation.name == "synthesize" {
                    let args = annotation.args.unwrap_or_default();
                    Ok(Value::Synthesize(args.trim().trim_matches('"').to_owned()))
                } else {
                    Err(self.cursor.error("Only @synthesize is a value"))
                }
            }
            Some('"') => Ok(Value::String(self.parse_string()?)),
            Some('{') => self.parse_object(),
            Some('[') => self.parse_array(),
            Some(_) => self.parse_bare(),
        }
    }

    fn parse_object(&mut self) -> ParseResult<Value> {
        self.cursor.bump(); // '{'
        let mut pairs = Vec::new();
        loop {
            self.skip_trivia();
            if self.cursor.is_eof() {
                return Err(self.cursor.error("Unclosed object"));
            }
            if self.cursor.eat('}') {
                break;
            }
            match self.parse_pair()? {
                Some((key, node)) => push_pair(&mut pairs, key, node)?,
                None => return Err(self.cursor.error("Unclosed object")),
            }
            self.skip_trivia();
            self.cursor.eat(',');
        }
        Ok(Value::Object(pairs))
    }

    fn parse_array(&mut self) -> ParseResult<Value> {
        self.cursor.bump(); // '['
        let mut items = Vec::new();
        loop {
            self.skip_trivia();
            if self.cursor.is_eof() {
                return Err(self.cursor.error("Unclosed array"));
            }
            if self.cursor.eat(']') {
                break;
            }
            items.push(self.parse_node()?);
            self.skip_trivia();
            self.cursor.eat(',');
        }
        Ok(Value::Array(items))
    }

    /// A bare token: number, `true`/`false`, `null`, or an unquoted string.
    fn parse_bare(&mut self) -> ParseResult<Value> {
        let start = self.cursor.position();
        while let Some(ch) = self.cursor.peek() {
            if matches!(ch, '\n' | ',' | '}' | ']' | '~' | '#') {
                break;
            }
            if ch == ' ' || ch == '\t' {
                let after = self.cursor.remaining().trim_start();
                if after.starts_with('@') || after.starts_with('#') || after.starts_with("//") {
                    break;
                }
            }
            if self.cursor.starts_with("//") {
                break;
            }
            self.cursor.bump();
        }
        let raw = self.cursor.input()[start..self.cursor.position()].trim();
        if raw.is_empty() {
            return Err(ParseError::new(start, "Expected a value"));
        }
        Ok(match raw {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            "null" => Value::Null,
            _ => match raw.parse::<f64>() {
                Ok(n) => Value::Number(n),
                Err(_) => Value::String(raw.to_owned()),
            },
        })
    }

    /// Parse `~<number>` when present (but not `~"…"`, a semantic key).
    fn parse_confidence(&mut self) -> ParseResult<Option<f64>> {
        if self.cursor.peek() != Some('~') || self.cursor.peek_at(1) == Some('"') {
            return Ok(None);
        }
        let start = self.cursor.position();
        self.cursor.bump(); // '~'
        let num_start = self.cursor.position();
        self.cursor
            .skip_while(|ch| !ch.is_whitespace() && !matches!(ch, ',' | '}' | ']' | '~'));
        let raw = &self.cursor.input()[num_start..self.cursor.position()];
        let value: f64 = raw
            .trim()
            .parse()
            .map_err(|_| ParseError::new(start, format!("invalid confidence `~{raw}`")))?;
        if !(0.0..=1.0).contains(&value) {
            return Err(ParseError::new(
                start,
                format!("confidence {value} outside [0.0, 1.0]"),
            ));
        }
        Ok(Some(value))
    }

    fn parse_annotation(&mut self) -> ParseResult<Annotation> {
        let start = self.cursor.position();
        self.cursor.bump(); // '@'
        let name_start = self.cursor.position();
        self.cursor
            .skip_while(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-');
        let name = self.cursor.input()[name_start..self.cursor.position()].to_owned();
        if name.is_empty() {
            return Err(ParseError::new(start, "Annotation needs a name"));
        }

        let mut args = None;
        self.skip_spaces();
        if self.cursor.eat('(') {
            let arg_start = self.cursor.position();
            self.cursor.skip_while(|ch| ch != ')');
            if !self.cursor.eat(')') {
                return Err(ParseError::new(arg_start, "Unclosed annotation arguments"));
            }
            args = Some(self.cursor.input()[arg_start..self.cursor.position() - 1].to_owned());
        }

        let mut properties = Vec::new();
        self.skip_spaces();
        if self.cursor.eat('{') {
            loop {
                self.skip_trivia();
                if self.cursor.eat('}') {
                    break;
                }
                let prop_start = self.cursor.position();
                self.cursor
                    .skip_while(|ch| ch != ':' && ch != '\n' && ch != '}');
                if self.cursor.peek() != Some(':') {
                    return Err(self.cursor.error("Missing colon in annotation property"));
                }
                let prop_name = self.cursor.input()[prop_start..self.cursor.position()]
                    .trim()
                    .to_owned();
                self.cursor.bump(); // ':'
                self.skip_trivia();
                if self.cursor.peek() != Some('"') {
                    return Err(self
                        .cursor
                        .error("Annotation property needs a string value"));
                }
                let prop_value = self.parse_string()?;
                properties.push((prop_name, prop_value));
                self.skip_trivia();
                self.cursor.eat(',');
            }
        }

        Ok(Annotation {
            name,
            args,
            properties,
        })
    }

    fn parse_string(&mut self) -> ParseResult<String> {
        let start = self.cursor.position();
        self.cursor.bump(); // opening quote
        let mut out = String::new();
        loop {
            match self.cursor.bump() {
                None => return Err(ParseError::new(start, "Unclosed string")),
                Some('"') => return Ok(out),
                Some('\\') => match self.cursor.bump() {
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some(other) => out.push(other),
                    None => return Err(ParseError::new(start, "Unclosed string")),
                },
                Some(ch) => out.push(ch),
            }
        }
    }

    fn skip_trivia(&mut self) {
        loop {
            self.cursor.skip_while(|ch| ch.is_whitespace());
            if self.cursor.starts_with("#") || self.cursor.starts_with("//") {
                self.cursor.skip_while(|ch| ch != '\n');
                continue;
            }
            break;
        }
    }

    fn skip_spaces(&mut self) {
        self.cursor.skip_while(|ch| ch == ' ' || ch == '\t');
    }
}

fn push_pair(pairs: &mut Vec<(Key, Node)>, key: Key, node: Node) -> ParseResult<()> {
    if pairs.iter().any(|(k, _)| k.text() == key.text()) {
        return Err(ParseError::new(0, format!("Duplicate key: {}", key.text())));
    }
    pairs.push((key, node));
    Ok(())
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
