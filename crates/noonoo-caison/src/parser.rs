//! The CAISON parser: documents into the node model.

use super::{Annotation, Document, Key, Node, Value};
use noonoo_parse::{Cursor, ParseError, ParseResult};
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

pub(crate) fn parse(input: &str) -> ParseResult<Document> {
    Parser::new(input).parse_document()
}
