//! Script-side style edits: `el.style.setProperty` / `removeProperty`.
//!
//! The cascade reads the inline `style` attribute on every compute, so an
//! edit is just a rewrite of that attribute; the next compute (and layout)
//! picks it up.

use core::fmt;

use noonoo_css::{InlineStyleError, remove_inline_property, set_inline_property};
use noonoo_dom::{Document, MutError, NodeId};

/// Why [`set_style_property`] refused an edit.
#[derive(Debug, PartialEq, Eq)]
pub enum StyleEditError {
    /// The name or value was not valid CSS.
    Style(InlineStyleError),
    /// The target is not an element in the document.
    Mutation(MutError),
}

impl fmt::Display for StyleEditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Style(error) => write!(f, "{error}"),
            Self::Mutation(error) => write!(f, "mutation rejected: {error:?}"),
        }
    }
}

impl std::error::Error for StyleEditError {}

/// Set `name` to `value` in `element`'s inline style. An empty value
/// removes the property.
pub fn set_style_property(
    document: &mut Document,
    element: NodeId,
    name: &str,
    value: &str,
    important: bool,
) -> Result<(), StyleEditError> {
    let style = inline_style(document, element).map_err(StyleEditError::Mutation)?;
    let style =
        set_inline_property(&style, name, value, important).map_err(StyleEditError::Style)?;
    write_style(document, element, &style).map_err(StyleEditError::Mutation)
}

/// Remove `name` (and, for a shorthand, its longhands) from `element`'s
/// inline style. Removing the last property drops the attribute.
pub fn remove_style_property(
    document: &mut Document,
    element: NodeId,
    name: &str,
) -> Result<(), MutError> {
    let style = inline_style(document, element)?;
    write_style(document, element, &remove_inline_property(&style, name))
}

fn inline_style(document: &Document, element: NodeId) -> Result<String, MutError> {
    let node = document
        .node(element)
        .ok_or(MutError::NoSuchNode(element))?;
    let data = node.as_element().ok_or(MutError::NotAnElement(element))?;
    Ok(data.attr("style").unwrap_or_default().to_owned())
}

fn write_style(document: &mut Document, element: NodeId, style: &str) -> Result<(), MutError> {
    if style.is_empty() {
        document.mutate().remove_attribute(element, "style")
    } else {
        document.mutate().set_attribute(element, "style", style)
    }
}
