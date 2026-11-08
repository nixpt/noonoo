//! Editing an inline `style` attribute one property at a time, as CSSOM's
//! `style.setProperty` / `style.removeProperty` do.

use crate::property::shorthand_longhands;
use crate::{RawDeclaration, TokenKind, parse_declaration, parse_declarations, tokenize};

/// Why [`set_inline_property`] refused an edit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InlineStyleError {
    /// The name is not a single CSS identifier.
    InvalidName,
    /// The value does not parse, or would end its declaration early
    /// (a `;`, `{`, `}`, `:` or `!`).
    InvalidValue,
}

impl std::fmt::Display for InlineStyleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidName => "invalid CSS property name",
            Self::InvalidValue => "invalid CSS property value",
        })
    }
}

impl std::error::Error for InlineStyleError {}

/// `style` with `name` set to `value`. An empty (or all-whitespace) value
/// removes the property instead.
///
/// An existing declaration is replaced where it stands, unless a later
/// shorthand covers it, in which case the new one moves to the end so it
/// still wins. Setting a shorthand drops the longhands it covers.
pub fn set_inline_property(
    style: &str,
    name: &str,
    value: &str,
    important: bool,
) -> Result<String, InlineStyleError> {
    if !is_property_name(name) {
        return Err(InlineStyleError::InvalidName);
    }
    let value = value.trim();
    if value.is_empty() {
        return Ok(remove_inline_property(style, name));
    }
    if !is_plain_value(value) || parse_declaration(name, value, important).is_err() {
        return Err(InlineStyleError::InvalidValue);
    }
    let new = RawDeclaration {
        name: if name.starts_with("--") {
            name.to_owned()
        } else {
            name.to_ascii_lowercase()
        },
        value: value.to_owned(),
        important,
    };
    let longhands = longhand_names(name);
    let mut declarations: Vec<RawDeclaration> = parse_declarations(style)
        .into_iter()
        .filter(|d| !longhands.iter().any(|l| names_match(&d.name, l)))
        .collect();
    let existing: Vec<usize> = (0..declarations.len())
        .filter(|&i| names_match(&declarations[i].name, name))
        .collect();
    let in_place = existing.first().copied().filter(|&first| {
        !declarations[first + 1..].iter().any(|later| {
            longhand_names(&later.name)
                .iter()
                .any(|l| names_match(l, name))
        })
    });
    match in_place {
        Some(first) => {
            for &i in existing[1..].iter().rev() {
                declarations.remove(i);
            }
            declarations[first] = new;
        }
        None => {
            declarations.retain(|d| !names_match(&d.name, name));
            declarations.push(new);
        }
    }
    Ok(serialize(&declarations))
}

/// `style` without any declaration of `name`, nor of the longhands it
/// covers when `name` is a shorthand.
pub fn remove_inline_property(style: &str, name: &str) -> String {
    let longhands = longhand_names(name);
    let declarations: Vec<RawDeclaration> = parse_declarations(style)
        .into_iter()
        .filter(|d| {
            !names_match(&d.name, name) && !longhands.iter().any(|l| names_match(&d.name, l))
        })
        .collect();
    serialize(&declarations)
}

fn serialize(declarations: &[RawDeclaration]) -> String {
    declarations
        .iter()
        .map(|d| {
            let priority = if d.important { " !important" } else { "" };
            format!("{}: {}{priority}", d.name, d.value)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Custom properties (`--x`) are case-sensitive; others are ASCII
/// case-insensitive.
fn names_match(a: &str, b: &str) -> bool {
    if a.starts_with("--") || b.starts_with("--") {
        a == b
    } else {
        a.eq_ignore_ascii_case(b)
    }
}

fn longhand_names(name: &str) -> Vec<String> {
    if name.starts_with("--") {
        return Vec::new();
    }
    shorthand_longhands(&name.to_ascii_lowercase())
        .unwrap_or_default()
        .iter()
        .map(|p| p.name())
        .collect()
}

fn is_property_name(name: &str) -> bool {
    let tokens = tokenize(name);
    matches!(tokens.as_slice(), [t] if matches!(t.kind, TokenKind::Ident(_)) && t.start == 0 && t.end == name.len())
}

fn is_plain_value(value: &str) -> bool {
    !tokenize(value).iter().any(|t| {
        matches!(
            t.kind,
            TokenKind::Semicolon
                | TokenKind::Colon
                | TokenKind::OpenBrace
                | TokenKind::CloseBrace
                | TokenKind::Delim('!')
                | TokenKind::BadStr
                | TokenKind::BadUrl
                | TokenKind::Cdo
                | TokenKind::Cdc
        )
    })
}
