//! Reading the answer to a prompt from what was typed on the command line.
//!
//! Quantities are read by [`hito_units`], so they can name units and add
//! them up (`3,5 m + 20 cm`), and numbers are written as the locale writes
//! them. A point is its coordinates, separated as in AutoCAD by a comma
//! (`3.5,2`), or by a semicolon in locales that write decimals with a
//! comma (`3,5;2`).

use hito_core::Dimension;
use hito_units::{NumberFormat, UnitCatalogue, Units};

use super::{Input, Wants};
use crate::i18n::Locale;

/// How typed answers are read.
#[derive(Debug)]
pub struct InputFormat {
    units: Units,
    separator: char,
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

impl InputFormat {
    /// Reads numbers as `locale` writes them, and values without units in
    /// SI units until the project's units exist.
    pub fn new(locale: Locale) -> Self {
        let numbers = NumberFormat::new(locale.tag()).expect("the UI locales are valid tags");
        Self::with_units(Units::new(UnitCatalogue::standard(), numbers))
    }

    pub fn with_units(units: Units) -> Self {
        let decimal_comma = units.format(0.5, Dimension::NONE).contains(',');
        let separator = if decimal_comma { ';' } else { ',' };
        Self { units, separator }
    }

    /// What separates a point's coordinates.
    pub fn separator(&self) -> char {
        self.separator
    }

    /// Reads `text` as the answer to a prompt that `wants` it.
    pub fn parse(&self, text: &str, wants: Wants) -> Result<Input, InputError> {
        if text.trim().is_empty() {
            return Err(InputError::Empty);
        }
        match wants {
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
        let parts: Vec<&str> = text.split(self.separator).collect();
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
    fn points_as_in_autocad_or_with_semicolons_where_decimals_take_commas() {
        let en = InputFormat::new(Locale::En);
        assert_eq!(en.separator(), ',');
        assert_eq!(
            en.parse("3.5, 2", Wants::Point),
            Ok(Input::Point([3.5, 2.0, 0.0]))
        );
        assert_eq!(
            en.parse("1,2 m,300 cm", Wants::Point),
            Ok(Input::Point([1.0, 2.0, 3.0]))
        );

        let es = InputFormat::new(Locale::EsAr);
        assert_eq!(es.separator(), ';');
        assert_eq!(
            es.parse("3,5; 2", Wants::Point),
            Ok(Input::Point([3.5, 2.0, 0.0]))
        );

        for bad in ["3", "1,2,3,4", "a,b", "1 kN,2"] {
            assert_eq!(
                en.parse(bad, Wants::Point),
                Err(InputError::NotAPoint),
                "{bad}"
            );
        }
    }

    #[test]
    fn quantities_with_or_without_units() {
        let es = InputFormat::new(Locale::EsAr);
        let length = Wants::Quantity(Dimension::LENGTH);
        let parsed = |text| match es.parse(text, length) {
            Ok(Input::Quantity(value)) => value,
            other => panic!("{text}: {other:?}"),
        };
        assert!((parsed("3,5 m + 20 cm") - 3.7).abs() < 1e-12);
        assert_eq!(parsed("2"), 2.0);
        assert_eq!(es.parse("2 kN", length), Err(InputError::NotAQuantity));
        assert_eq!(es.parse("  ", length), Err(InputError::Empty));
    }
}
