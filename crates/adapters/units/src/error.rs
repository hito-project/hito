use std::fmt;

use hito_core::Dimension;

/// Why typed input couldn't be read as a quantity. Positions (`at`) count
/// characters from the start of the input.
///
/// Errors that name dimensions carry them for the UI, which shows localised
/// messages, and their SI unit for the English developer message
/// ([`Display`](fmt::Display)). A number without units has an empty unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Nothing but whitespace.
    Empty,
    /// Something that doesn't fit, such as `$` or a closing parenthesis too
    /// many.
    Unexpected { at: usize, found: String },
    /// The input stopped in the middle, such as after `3 m +`.
    UnexpectedEnd,
    /// Digits and separators that aren't a number in the locale, such as
    /// `3.5` in es-AR, where the point groups thousands.
    BadNumber { at: usize, text: String },
    /// A symbol the unit catalogue doesn't have.
    UnknownUnit { at: usize, symbol: String },
    /// An exponent too large to keep, such as `m^200`.
    ExponentOutOfRange { at: usize },
    /// A number without a unit in a mixed quantity, such as the `6` in
    /// `10' 6`.
    MissingUnit { at: usize },
    /// Adding or subtracting quantities of different dimensions, such as
    /// metres and kilonewtons.
    Incompatible {
        at: usize,
        /// `+` or `-`.
        operator: char,
        left: Dimension,
        right: Dimension,
        left_unit: String,
        right_unit: String,
    },
    /// The input is a quantity, but not of the dimension asked for.
    WrongDimension {
        expected: Dimension,
        found: Dimension,
        expected_unit: String,
        found_unit: String,
    },
    /// The result is infinite or not a number, as after a division by zero.
    NotFinite,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let a_value_in = |unit: &str| match unit {
            "" => "a number without units".to_owned(),
            unit => format!("a value in {unit}"),
        };
        match self {
            Self::Empty => write!(f, "nothing to read"),
            Self::Unexpected { at, found } => write!(f, "unexpected `{found}` at {at}"),
            Self::UnexpectedEnd => write!(f, "the input ends too early"),
            Self::BadNumber { at, text } => {
                write!(f, "`{text}` at {at} isn't a number in this locale")
            }
            Self::UnknownUnit { at, symbol } => write!(f, "unknown unit `{symbol}` at {at}"),
            Self::ExponentOutOfRange { at } => write!(f, "the exponent at {at} is too large"),
            Self::MissingUnit { at } => write!(f, "the number at {at} needs a unit"),
            Self::Incompatible {
                operator,
                left_unit,
                right_unit,
                ..
            } => {
                let (verb, preposition) = match operator {
                    '-' => ("subtract", "from"),
                    _ => ("add", "to"),
                };
                write!(
                    f,
                    "can't {verb} {} {preposition} {}: they measure different things",
                    a_value_in(right_unit),
                    a_value_in(left_unit)
                )
            }
            Self::WrongDimension {
                expected_unit,
                found_unit,
                ..
            } => {
                let found = match found_unit.as_str() {
                    "" => "a number without units".to_owned(),
                    unit => format!("one in {unit}"),
                };
                write!(f, "expected {}, got {found}", a_value_in(expected_unit))
            }
            Self::NotFinite => write!(f, "the result isn't a finite number"),
        }
    }
}

impl std::error::Error for ParseError {}
