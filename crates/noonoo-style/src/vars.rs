//! CSS custom properties (`--x`) and `var()` substitution.
//!
//! Custom properties are *substitution tokens*, not typed values: a declaration
//! like `color: var(--brand)` is replaced with the token list of `--brand`
//! before the property is resolved. That happens at **computed-value time**
//! (after the cascade has picked a winner), which is what lets the value depend
//! on cascade and inheritance.
//!
//! The rules implemented here follow CSS Custom Properties Level 1:
//!
//! - A custom property inherits unless a descendant overrides it.
//! - `var(--x, fallback)` uses `fallback` when `--x` is not defined, is the
//!   guaranteed-invalid value, or resolves to something invalid (e.g. a cycle).
//! - A `var()` with no fallback that cannot be resolved makes the whole
//!   declaration **invalid at computed-value time**: it is dropped, so the
//!   property falls back to its inherited or initial value.
//! - Cycles (`--a: var(--b); --b: var(--a)`) are invalid rather than infinite.

use std::collections::HashMap;

use noonoo_css::value::Value;

/// Custom property name (without the `--` prefix) to its token list.
pub(crate) type CustomProperties = HashMap<String, Vec<Value>>;

/// Substitute every `var()` reference in `values`.
///
/// Returns `None` when a reference cannot be resolved and has no fallback; the
/// caller then drops the declaration.
pub(crate) fn substitute(values: &[Value], props: &CustomProperties) -> Option<Vec<Value>> {
    substitute_with(values, props, &mut Vec::new())
}

fn substitute_with(
    values: &[Value],
    props: &CustomProperties,
    seen: &mut Vec<String>,
) -> Option<Vec<Value>> {
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        match value {
            Value::Function { name, args } if name.eq_ignore_ascii_case("var") => {
                out.extend(resolve(args, props, seen)?);
            }
            // Substitute inside other functions too: `rgb(var(--r), 0, 0)`.
            Value::Function { name, args } => out.push(Value::Function {
                name: name.clone(),
                args: substitute_with(args, props, seen)?,
            }),
            other => out.push(other.clone()),
        }
    }
    Some(out)
}

fn resolve(args: &[Value], props: &CustomProperties, seen: &mut Vec<String>) -> Option<Vec<Value>> {
    let (first, rest) = args.split_first()?;
    let name = first.as_keyword()?.strip_prefix("--")?.to_ascii_lowercase();

    let fallback = match rest.first() {
        Some(Value::Comma) => Some(&rest[1..]),
        _ => None,
    };

    if !seen.contains(&name)
        && let Some(value) = props.get(&name)
    {
        seen.push(name);
        let resolved = substitute_with(value, props, seen);
        seen.pop();
        if let Some(resolved) = resolved {
            return Some(resolved);
        }
        // Defined but invalid (e.g. a cycle): treat as if undefined, so the
        // fallback applies.
    }

    fallback.and_then(|tokens| substitute_with(tokens, props, seen))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyword(name: &str) -> Value {
        Value::Keyword(name.into())
    }

    fn var(name: &str) -> Value {
        Value::Function {
            name: "var".into(),
            args: vec![keyword(name)],
        }
    }

    fn props(pairs: &[(&str, Vec<Value>)]) -> CustomProperties {
        pairs
            .iter()
            .map(|(name, values)| ((*name).to_owned(), values.clone()))
            .collect()
    }

    #[test]
    fn resolves_a_definition() {
        let p = props(&[("brand", vec![keyword("red")])]);
        assert_eq!(
            substitute(&[var("--brand")], &p),
            Some(vec![keyword("red")])
        );
    }

    #[test]
    fn resolves_a_fallback() {
        let p = props(&[]);
        let call = Value::Function {
            name: "var".into(),
            args: vec![keyword("--missing"), Value::Comma, keyword("green")],
        };
        assert_eq!(substitute(&[call], &p), Some(vec![keyword("green")]));
    }

    #[test]
    fn an_undefined_reference_without_fallback_is_invalid() {
        assert_eq!(substitute(&[var("--missing")], &props(&[])), None);
    }

    #[test]
    fn resolves_a_chain() {
        let p = props(&[("a", vec![var("--b")]), ("b", vec![keyword("blue")])]);
        assert_eq!(substitute(&[var("--a")], &p), Some(vec![keyword("blue")]));
    }

    #[test]
    fn a_cycle_is_invalid_and_falls_back() {
        let p = props(&[("a", vec![var("--b")]), ("b", vec![var("--a")])]);
        let call = Value::Function {
            name: "var".into(),
            args: vec![keyword("--a"), Value::Comma, keyword("black")],
        };
        assert_eq!(substitute(&[call], &p), Some(vec![keyword("black")]));
    }

    #[test]
    fn substitutes_inside_other_functions() {
        let p = props(&[("r", vec![Value::Number(0.0)])]);
        let call = Value::Function {
            name: "rgb".into(),
            args: vec![var("--r"), Value::Comma, Value::Number(0.0)],
        };
        assert_eq!(
            substitute(&[call], &p),
            Some(vec![Value::Function {
                name: "rgb".into(),
                args: vec![Value::Number(0.0), Value::Comma, Value::Number(0.0)],
            }])
        );
    }
}
