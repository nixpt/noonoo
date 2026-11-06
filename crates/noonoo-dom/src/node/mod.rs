//! The node payload model.
//!
//! A node is one of a small closed set of kinds. Elements carry a tag name and
//! an ordered attribute list; text, comments and doctypes carry a string. This
//! is deliberately parser-agnostic and CSS-agnostic — no `stylo` `LocalName`,
//! no namespace machinery — so the tree stands alone.

/// An element attribute, name/value, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    /// The attribute name (lowercased on ingest).
    pub name: String,
    /// The attribute value.
    pub value: String,
}

/// The payload of an element node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementData {
    /// The tag name, lowercased.
    pub name: String,
    /// Attributes, in insertion order.
    pub attrs: Vec<Attribute>,
}

impl ElementData {
    /// Create an element with the given (lowercased) tag name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into().to_ascii_lowercase(),
            attrs: Vec::new(),
        }
    }

    /// The value of `name`, if present.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.value.as_str())
    }

    /// The `id` attribute, if present.
    pub fn id(&self) -> Option<&str> {
        self.attr("id")
    }

    /// The classes in the `class` attribute.
    pub fn classes(&self) -> impl Iterator<Item = &str> {
        self.attr("class")
            .into_iter()
            .flat_map(str::split_whitespace)
    }

    /// Whether the element carries `class`.
    pub fn has_class(&self, class: &str) -> bool {
        self.classes().any(|c| c == class)
    }

    pub(crate) fn set_attr(&mut self, name: &str, value: impl Into<String>) {
        let value = value.into();
        match self.attrs.iter_mut().find(|a| a.name == name) {
            Some(a) => a.value = value,
            None => self.attrs.push(Attribute {
                name: name.to_owned(),
                value,
            }),
        }
    }

    pub(crate) fn remove_attr(&mut self, name: &str) {
        self.attrs.retain(|a| a.name != name);
    }
}

/// The payload of a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeData {
    /// The document root.
    Document,
    /// A doctype declaration.
    Doctype {
        /// The doctype name.
        name: String,
    },
    /// An element.
    Element(ElementData),
    /// A text run.
    Text(String),
    /// A comment.
    Comment(String),
}

impl NodeData {
    /// Borrow this node's element data, if it is an element.
    pub fn as_element(&self) -> Option<&ElementData> {
        match self {
            Self::Element(element) => Some(element),
            _ => None,
        }
    }

    /// Mutably borrow this node's element data, if it is an element.
    pub fn as_element_mut(&mut self) -> Option<&mut ElementData> {
        match self {
            Self::Element(element) => Some(element),
            _ => None,
        }
    }

    /// This node's text, if it is a text node.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    /// Whether this node is an element.
    pub fn is_element(&self) -> bool {
        matches!(self, Self::Element(_))
    }

    /// Whether this node may contain children.
    pub fn can_have_children(&self) -> bool {
        matches!(self, Self::Document | Self::Element(_))
    }
}
