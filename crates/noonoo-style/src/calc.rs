//! `calc()`, `min()`, `max()`, and `clamp()` evaluation.
//!
//! These functions form a small typed expression language: lengths and
//! percentages can be added and subtracted, and multiplied or divided by
//! unitless numbers, but a bare number cannot be added to a length. Because
//! `em`/`rem` depend on the computed font size, evaluation happens at
//! computed-value time, next to `var()`.
//!
//! A result keeps its *percentage* component separate from the absolute part,
//! so `calc(100% - 20px)` stays expressible until layout knows the containing
//! block. The same is true of `min()`/`max()`/`clamp()`: when their arguments
//! mix percentages with lengths, the comparison itself has to wait for layout
//! (see [`CalcValue::Extreme`] and the `Extreme` variants in `crate::computed`).
//! Arguments that share a percentage can be compared immediately, because the
//! comparison of `px + percent%` is the comparison of `px`.

use crate::computed::{CalcTerm, Extreme};
use noonoo_css::value::{Length as CssLength, LengthUnit as CssLengthUnit, Value};

/// A typed result of a math function.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum CalcValue {
    /// A length: `percent% + px`.
    Length {
        /// The absolute part, in pixels.
        px: f32,
        /// The percentage part (`50.0` for `50%`).
        percent: f32,
    },
    /// A unitless number.
    Number(f32),
    /// A `min()`/`max()`/`clamp()` that cannot be ordered until layout.
    Extreme {
        /// Which extreme to select.
        kind: Extreme,
        /// The length terms, each `px + percent%`.
        terms: Vec<CalcTerm>,
    },
}

impl CalcValue {
    /// Whether the value has no percentage component.
    pub(crate) fn is_absolute(&self) -> bool {
        match self {
            Self::Number(_) => true,
            Self::Length { percent, .. } => *percent == 0.0,
            Self::Extreme { terms, .. } => terms.iter().all(|term| term.percent == 0.0),
        }
    }
}

/// Evaluate the arguments of a `calc()` function.
pub(crate) fn evaluate(args: &[Value], font_size: f32, root: f32) -> Option<CalcValue> {
    let mut parser = Parser {
        values: args,
        pos: 0,
        font_size,
        root,
    };
    let value = parser.sum()?;
    (parser.pos == args.len()).then_some(value)
}

/// Evaluate a math function by name: `calc()`, `min()`, `max()`, or `clamp()`.
pub(crate) fn evaluate_function(
    name: &str,
    args: &[Value],
    font_size: f32,
    root: f32,
) -> Option<CalcValue> {
    if name.eq_ignore_ascii_case("calc") {
        return evaluate(args, font_size, root);
    }
    let kind = if name.eq_ignore_ascii_case("min") {
        Extreme::Min
    } else if name.eq_ignore_ascii_case("max") {
        Extreme::Max
    } else if name.eq_ignore_ascii_case("clamp") {
        Extreme::Clamp
    } else {
        return None;
    };

    let groups = split_commas(args)?;
    if kind == Extreme::Clamp && groups.len() != 3 {
        return None;
    }
    let mut values = Vec::with_capacity(groups.len());
    for group in groups {
        values.push(evaluate(group, font_size, root)?);
    }
    extreme(kind, values)
}

/// Resolve a math value against a reference length (a containing block, or the
/// parent font size).
pub(crate) fn resolve(value: &CalcValue, reference: f32) -> Option<f32> {
    match value {
        CalcValue::Number(number) => Some(*number),
        CalcValue::Length { px, percent } => Some(px + reference * percent / 100.0),
        CalcValue::Extreme { kind, terms } => {
            let values: Vec<f32> = terms
                .iter()
                .map(|term| term.px + reference * term.percent / 100.0)
                .collect();
            select(*kind, &values)
        }
    }
}

/// Convert a CSS length to pixels.
pub(crate) fn to_px(length: &CssLength, font_size: f32, root_font_size: f32) -> f32 {
    let value = length.value;
    match length.unit {
        CssLengthUnit::Px => value,
        CssLengthUnit::Em => value * font_size,
        CssLengthUnit::Rem => value * root_font_size,
        CssLengthUnit::Ex | CssLengthUnit::Ch => value * font_size * 0.5,
        CssLengthUnit::Pt => value * 96.0 / 72.0,
        CssLengthUnit::Pc => value * 16.0,
        CssLengthUnit::In => value * 96.0,
        CssLengthUnit::Cm => value * 96.0 / 2.54,
        CssLengthUnit::Mm => value * 96.0 / 25.4,
        CssLengthUnit::Q => value * 96.0 / 101.6,
        // Viewport-relative units need a viewport; approximate with the root.
        CssLengthUnit::Vw | CssLengthUnit::Vh | CssLengthUnit::Vmin | CssLengthUnit::Vmax => value,
        CssLengthUnit::Other(_) => 0.0,
    }
}

/// Fold a comparison, or defer it when the terms mix percentages and lengths.
fn extreme(kind: Extreme, values: Vec<CalcValue>) -> Option<CalcValue> {
    if values.is_empty() {
        return None;
    }

    // All numbers: compare now.
    if values
        .iter()
        .all(|value| matches!(value, CalcValue::Number(_)))
    {
        let numbers: Vec<f32> = values
            .iter()
            .map(|value| match value {
                CalcValue::Number(number) => *number,
                _ => unreachable!("checked above"),
            })
            .collect();
        return Some(CalcValue::Number(select(kind, &numbers)?));
    }

    // Lengths can be compared now when they share a slope: equal percentages
    // mean the order is the order of the absolute parts, and equal absolute
    // parts mean it is the order of the percentages (references are
    // non-negative). Otherwise the comparison needs the reference.
    let mut pixels = Vec::with_capacity(values.len());
    let mut percents = Vec::with_capacity(values.len());
    for value in &values {
        match value {
            CalcValue::Length { px, percent } => {
                pixels.push(*px);
                percents.push(*percent);
            }
            _ => return None,
        }
    }
    let same_percent = percents.windows(2).all(|pair| pair[0] == pair[1]);
    let same_px = pixels.windows(2).all(|pair| pair[0] == pair[1]);
    if same_percent {
        return Some(CalcValue::Length {
            px: select(kind, &pixels)?,
            percent: percents[0],
        });
    }
    if same_px {
        return Some(CalcValue::Length {
            px: pixels[0],
            percent: select(kind, &percents)?,
        });
    }

    // Defer: every term must be a length.
    let mut terms = Vec::with_capacity(values.len());
    for value in values {
        match value {
            CalcValue::Length { px, percent } => terms.push(CalcTerm { px, percent }),
            _ => return None,
        }
    }
    Some(CalcValue::Extreme { kind, terms })
}

fn select(kind: Extreme, values: &[f32]) -> Option<f32> {
    match kind {
        Extreme::Min => values.iter().copied().reduce(f32::min),
        Extreme::Max => values.iter().copied().reduce(f32::max),
        Extreme::Clamp => {
            let [min, value, max] = values else {
                return None;
            };
            // `clamp(min, val, max)` == `max(min, min(val, max))`; a reversed
            // range yields `min`, per CSS Values.
            Some(min.max(value.min(*max)))
        }
    }
}

/// Split function arguments on top-level commas.
fn split_commas(args: &[Value]) -> Option<Vec<&[Value]>> {
    let mut groups = Vec::new();
    let mut start = 0;
    for (index, value) in args.iter().enumerate() {
        if matches!(value, Value::Comma) {
            groups.push(&args[start..index]);
            start = index + 1;
        }
    }
    groups.push(&args[start..]);
    groups
        .iter()
        .all(|group| !group.is_empty())
        .then_some(groups)
}

struct Parser<'a> {
    values: &'a [Value],
    pos: usize,
    font_size: f32,
    root: f32,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Value> {
        self.values.get(self.pos)
    }

    /// `sum := product (('+' | '-') product)*`
    fn sum(&mut self) -> Option<CalcValue> {
        let mut left = self.product()?;
        loop {
            let sign = match self.peek() {
                Some(Value::Delim('+')) => 1.0,
                Some(Value::Delim('-')) => -1.0,
                _ => break,
            };
            self.pos += 1;
            let right = self.product()?;
            left = add(left, right, sign)?;
        }
        Some(left)
    }

    /// `product := unary (('*' | '/') unary)*`
    fn product(&mut self) -> Option<CalcValue> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Value::Delim('*')) => '*',
                Some(Value::Slash) => '/',
                _ => break,
            };
            self.pos += 1;
            let right = self.unary()?;
            left = if op == '*' {
                multiply(left, right)?
            } else {
                divide(left, right)?
            };
        }
        Some(left)
    }

    fn unary(&mut self) -> Option<CalcValue> {
        match self.peek() {
            Some(Value::Delim('+')) => {
                self.pos += 1;
                self.unary()
            }
            Some(Value::Delim('-')) => {
                self.pos += 1;
                multiply(self.unary()?, CalcValue::Number(-1.0))
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Option<CalcValue> {
        let value = self.values.get(self.pos)?;
        self.pos += 1;
        match value {
            Value::Number(number) => Some(CalcValue::Number(*number)),
            Value::Length(length) => Some(CalcValue::Length {
                px: to_px(length, self.font_size, self.root),
                percent: 0.0,
            }),
            Value::Percentage(percent) => Some(CalcValue::Length {
                px: 0.0,
                percent: *percent,
            }),
            Value::Group(inner) => evaluate(inner, self.font_size, self.root),
            Value::Function { name, args } => {
                evaluate_function(name, args, self.font_size, self.root)
            }
            _ => None,
        }
    }
}

fn add(left: CalcValue, right: CalcValue, sign: f32) -> Option<CalcValue> {
    match (left, right) {
        (CalcValue::Number(a), CalcValue::Number(b)) => Some(CalcValue::Number(a + sign * b)),
        (
            CalcValue::Length {
                px: a_px,
                percent: a_percent,
            },
            CalcValue::Length {
                px: b_px,
                percent: b_percent,
            },
        ) => Some(CalcValue::Length {
            px: a_px + sign * b_px,
            percent: a_percent + sign * b_percent,
        }),
        _ => None,
    }
}

fn multiply(left: CalcValue, right: CalcValue) -> Option<CalcValue> {
    match (left, right) {
        (CalcValue::Number(a), CalcValue::Number(b)) => Some(CalcValue::Number(a * b)),
        (CalcValue::Length { px, percent }, CalcValue::Number(n))
        | (CalcValue::Number(n), CalcValue::Length { px, percent }) => Some(CalcValue::Length {
            px: px * n,
            percent: percent * n,
        }),
        _ => None,
    }
}

fn divide(left: CalcValue, right: CalcValue) -> Option<CalcValue> {
    match (left, right) {
        (CalcValue::Number(a), CalcValue::Number(b)) if b != 0.0 => Some(CalcValue::Number(a / b)),
        (CalcValue::Length { px, percent }, CalcValue::Number(b)) if b != 0.0 => {
            Some(CalcValue::Length {
                px: px / b,
                percent: percent / b,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noonoo_css::value::parse_values;

    fn math(source: &str, font_size: f32, root: f32) -> Option<CalcValue> {
        let values = parse_values(source).ok()?;
        let [Value::Function { name, args }] = values.as_slice() else {
            return None;
        };
        evaluate_function(name, args, font_size, root)
    }

    fn length(px: f32, percent: f32) -> Option<CalcValue> {
        Some(CalcValue::Length { px, percent })
    }

    #[test]
    fn adds_and_subtracts_absolute_lengths() {
        assert_eq!(math("calc(1px + 2px)", 16.0, 16.0), length(3.0, 0.0));
        assert_eq!(math("calc(10px - 2px)", 16.0, 16.0), length(8.0, 0.0));
    }

    #[test]
    fn precedence_and_grouping() {
        assert_eq!(math("calc(1px + 2px * 3)", 16.0, 16.0), length(7.0, 0.0));
        assert_eq!(math("calc((1px + 2px) * 3)", 16.0, 16.0), length(9.0, 0.0));
    }

    #[test]
    fn relative_units_use_the_font() {
        assert_eq!(math("calc(2em + 1rem)", 10.0, 16.0), length(36.0, 0.0));
    }

    #[test]
    fn keeps_percentages_separate() {
        assert_eq!(math("calc(100% - 20px)", 16.0, 16.0), length(-20.0, 100.0));
        assert_eq!(math("calc(50% * 2)", 16.0, 16.0), length(0.0, 100.0));
    }

    #[test]
    fn unitless_numbers() {
        assert_eq!(
            math("calc(2 * 3)", 16.0, 16.0),
            Some(CalcValue::Number(6.0))
        );
        assert_eq!(
            math("calc(10 / 4)", 16.0, 16.0),
            Some(CalcValue::Number(2.5))
        );
    }

    #[test]
    fn a_number_cannot_be_added_to_a_length() {
        assert_eq!(math("calc(1px + 2)", 16.0, 16.0), None);
    }

    #[test]
    fn division_by_zero_is_invalid() {
        assert_eq!(math("calc(10px / 0)", 16.0, 16.0), None);
    }

    #[test]
    fn nested_calc_and_negative_terms() {
        assert_eq!(
            math("calc(calc(1px) + -2px)", 16.0, 16.0),
            length(-1.0, 0.0)
        );
    }

    #[test]
    fn min_and_max_of_absolute_lengths_fold_eagerly() {
        assert_eq!(math("min(3px, 7px, 5px)", 16.0, 16.0), length(3.0, 0.0));
        assert_eq!(math("max(3px, 7px, 5px)", 16.0, 16.0), length(7.0, 0.0));
        assert_eq!(
            math("min(1px, 2em)", 10.0, 16.0),
            length(1.0, 0.0),
            "em resolves first"
        );
    }

    #[test]
    fn min_and_max_of_numbers_fold_eagerly() {
        assert_eq!(
            math("min(2, 5, 3)", 16.0, 16.0),
            Some(CalcValue::Number(2.0))
        );
    }

    #[test]
    fn min_and_max_with_one_shared_percentage_fold_eagerly() {
        assert_eq!(math("min(10%, 50%)", 16.0, 16.0), length(0.0, 10.0));
    }

    #[test]
    fn mixed_percentages_are_deferred_to_layout() {
        let value = math("min(100%, 600px)", 16.0, 16.0).unwrap();
        let CalcValue::Extreme { kind, terms } = value else {
            panic!("expected a deferred comparison");
        };
        assert_eq!(kind, Extreme::Min);
        assert_eq!(
            terms,
            vec![
                CalcTerm {
                    px: 0.0,
                    percent: 100.0
                },
                CalcTerm {
                    px: 600.0,
                    percent: 0.0
                }
            ]
        );
    }

    #[test]
    fn clamp_and_reversed_ranges() {
        assert_eq!(math("clamp(1px, 5px, 10px)", 16.0, 16.0), length(5.0, 0.0));
        assert_eq!(math("clamp(1px, 0px, 10px)", 16.0, 16.0), length(1.0, 0.0));
        assert_eq!(
            math("clamp(1px, 20px, 10px)", 16.0, 16.0),
            length(10.0, 0.0),
            "the maximum caps the value"
        );
        assert_eq!(
            math("clamp(10px, 5px, 1px)", 16.0, 16.0),
            length(10.0, 0.0),
            "a reversed range yields the minimum"
        );
        assert_eq!(math("clamp(1px, 2px)", 16.0, 16.0), None);
    }

    #[test]
    fn arithmetic_with_nested_min() {
        assert_eq!(
            math("calc(min(2px, 5px) + 1px)", 16.0, 16.0),
            length(3.0, 0.0)
        );
    }

    #[test]
    fn resolving_a_deferred_comparison() {
        let value = math("min(100%, 600px)", 16.0, 16.0).unwrap();
        assert_eq!(resolve(&value, 800.0), Some(600.0));
        assert_eq!(resolve(&value, 400.0), Some(400.0));

        let value = math("clamp(20px, 50%, 300px)", 16.0, 16.0).unwrap();
        assert_eq!(resolve(&value, 200.0), Some(100.0));
        assert_eq!(resolve(&value, 1000.0), Some(300.0));
        assert_eq!(resolve(&value, 10.0), Some(20.0));
    }

    #[test]
    fn a_number_and_a_length_cannot_be_compared() {
        assert_eq!(math("min(1px, 2)", 16.0, 16.0), None);
    }
}
