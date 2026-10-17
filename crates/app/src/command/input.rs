//! Reading the answer to a prompt from what was typed on the command line.
//!
//! The command line reads input as AutoCAD does, in every UI language: a
//! point is the decimal separator and a comma separates a point's
//! coordinates (`3.5,2`). Quantities are read by [`hito_units`], so they can
//! name units and add them up (`3.5 m + 20 cm`).

use hito_core::Dimension;
use hito_units::{NumberFormat, UnitCatalogue, Units};

use super::{Input, Wants};

/// What separates a point's coordinates.
const SEPARATOR: char = ',';

/// How typed answers are read.
#[derive(Debug)]
pub struct InputFormat {
    units: Units,
}

/// Typed text that doesn't answer the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    /// Nothing was typed.
    Empty,
    NotAPoint,
    NotAQuantity,
}

impl InputError {
    /// The message key of what the command line says.
    pub fn message(self) -> &'static str {
        match self {
            InputError::Empty => "input-empty",
            InputError::NotAPoint => "input-not-a-point",
            InputError::NotAQuantity => "input-not-a-value",
        }
    }
}

impl Default for InputFormat {
    fn default() -> Self {
        Self::new()
    }
}

impl InputFormat {
    /// Reads numbers with a decimal point, as CLDR's root locale writes
    /// them, and values without units in SI units until the project's units
    /// exist.
    pub fn new() -> Self {
        let numbers = NumberFormat::new("und").expect("the root locale is a valid tag");
        Self {
            units: Units::new(UnitCatalogue::standard(), numbers),
        }
    }

    /// Reads `text` as the answer to a prompt that `wants` it.
    pub fn parse(&self, text: &str, wants: Wants) -> Result<Input, InputError> {
        if text.trim().is_empty() {
            return Err(InputError::Empty);
        }
        match wants {
            // A comma only separates coordinates, so it never groups
            // thousands in a value.
            Wants::Quantity(_) if text.contains(SEPARATOR) => Err(InputError::NotAQuantity),
            Wants::Quantity(dimension) => self
                .units
                .parse(text, dimension)
                .map(Input::Quantity)
                .map_err(|_| InputError::NotAQuantity),
            Wants::Point => self.point(text),
        }
    }

    fn point(&self, text: &str) -> Result<Input, InputError> {
        let mut point = [0.0; 3];
        let parts: Vec<&str> = text.split(SEPARATOR).collect();
        if !(2..=3).contains(&parts.len()) {
            return Err(InputError::NotAPoint);
        }
        for (coordinate, part) in point.iter_mut().zip(parts) {
            *coordinate = self
                .units
                .parse(part, Dimension::LENGTH)
                .map_err(|_| InputError::NotAPoint)?;
        }
        Ok(Input::Point(point))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_as_in_autocad() {
        let input = InputFormat::new();
        assert_eq!(
            input.parse("3.5, 2", Wants::Point),
            Ok(Input::Point([3.5, 2.0, 0.0]))
        );
        assert_eq!(
            input.parse("1,2 m,300 cm", Wants::Point),
            Ok(Input::Point([1.0, 2.0, 3.0]))
        );
        for bad in ["3", "3.5", "1,2,3,4", "a,b", "1 kN,2", "3;2"] {
            assert_eq!(
                input.parse(bad, Wants::Point),
                Err(InputError::NotAPoint),
                "{bad}"
            );
        }
    }

    #[test]
    fn quantities_with_or_without_units() {
        let input = InputFormat::new();
        let length = Wants::Quantity(Dimension::LENGTH);
        let parsed = |text| match input.parse(text, length) {
            Ok(Input::Quantity(value)) => value,
            other => panic!("{text}: {other:?}"),
        };
        assert!((parsed("3.5 m + 20 cm") - 3.7).abs() < 1e-12);
        assert_eq!(parsed("2"), 2.0);
        assert_eq!(parsed("1500 mm"), 1.5);
        for bad in ["3,5", "1,000", "2 kN"] {
            assert_eq!(
                input.parse(bad, length),
                Err(InputError::NotAQuantity),
                "{bad}"
            );
        }
        assert_eq!(input.parse("  ", length), Err(InputError::Empty));
    }
}
