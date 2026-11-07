//! `calc()` evaluation.
//!
//! `calc()` is a typed expression language: lengths and percentages can be added
//! and subtracted, and multiplied or divided by unitless numbers, but a bare
//! number cannot be added to a length. Because `em`/`rem` depend on the computed
//! font size, evaluation happens at computed-value time, next to `var()`.
//!
//! The result keeps a *percentage* component separate from the absolute part, so
//! `calc(100% - 20px)` stays expressible until layout knows the containing block.
//! A purely absolute or purely percentage result is normalised to the existing
//! [`Dimension`](crate::Dimension)/[`Length`](crate::Length) variants by the
//! caller.

use noonoo_css::value::{Length as CssLength, LengthUnit as CssLengthUnit, Value};

/// A typed `calc()` result.
#[derive(Clone, Copy, PartialEq, Debug)]
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
            Value::Function { name, args } if name.eq_ignore_ascii_case("calc") => {
                evaluate(args, self.font_size, self.root)
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

    fn calc(source: &str, font_size: f32, root: f32) -> Option<CalcValue> {
        let values = parse_values(&format!("calc({source})")).ok()?;
        let [Value::Function { args, .. }] = values.as_slice() else {
            return None;
        };
        evaluate(args, font_size, root)
    }

    fn length(px: f32, percent: f32) -> Option<CalcValue> {
        Some(CalcValue::Length { px, percent })
    }

    #[test]
    fn adds_and_subtracts_absolute_lengths() {
        assert_eq!(calc("1px + 2px", 16.0, 16.0), length(3.0, 0.0));
        assert_eq!(calc("10px - 2px", 16.0, 16.0), length(8.0, 0.0));
    }

    #[test]
    fn precedence_and_grouping() {
        assert_eq!(calc("1px + 2px * 3", 16.0, 16.0), length(7.0, 0.0));
        assert_eq!(calc("(1px + 2px) * 3", 16.0, 16.0), length(9.0, 0.0));
    }

    #[test]
    fn relative_units_use_the_font() {
        assert_eq!(calc("2em + 1rem", 10.0, 16.0), length(36.0, 0.0));
    }

    #[test]
    fn keeps_percentages_separate() {
        assert_eq!(calc("100% - 20px", 16.0, 16.0), length(-20.0, 100.0));
        assert_eq!(calc("50% * 2", 16.0, 16.0), length(0.0, 100.0));
    }

    #[test]
    fn unitless_numbers() {
        assert_eq!(calc("2 * 3", 16.0, 16.0), Some(CalcValue::Number(6.0)));
        assert_eq!(calc("10 / 4", 16.0, 16.0), Some(CalcValue::Number(2.5)));
    }

    #[test]
    fn a_number_cannot_be_added_to_a_length() {
        assert_eq!(calc("1px + 2", 16.0, 16.0), None);
    }

    #[test]
    fn division_by_zero_is_invalid() {
        assert_eq!(calc("10px / 0", 16.0, 16.0), None);
    }

    #[test]
    fn nested_calc_and_negative_terms() {
        assert_eq!(calc("calc(1px) + -2px", 16.0, 16.0), length(-1.0, 0.0));
    }
}
