use std::collections::HashMap;

use hito_core::Dimension;

use crate::expr::evaluate;
use crate::{NumberFormat, ParseError, UnitCatalogue};

/// The unit a dimension is shown in, and how many decimals: a user
/// preference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayUnit {
    /// A unit expression, such as `cm`, `kN/m` or `kg/m³`.
    pub unit: String,
    /// Decimals shown.
    pub precision: u8,
}

/// Reads typed quantities and shows stored ones, with a unit catalogue, the
/// user's display units and a locale's number format.
///
/// Values go in and out in coherent SI units, as the core stores them.
#[derive(Debug)]
pub struct Units {
    catalogue: UnitCatalogue,
    numbers: NumberFormat,
    /// Each display unit with its factor to SI.
    display: HashMap<Dimension, (DisplayUnit, f64)>,
}

impl Units {
    /// Units with no display preferences: every dimension shows in SI.
    pub fn new(catalogue: UnitCatalogue, numbers: NumberFormat) -> Self {
        Self {
            catalogue,
            numbers,
            display: HashMap::new(),
        }
    }

    pub fn catalogue(&self) -> &UnitCatalogue {
        &self.catalogue
    }

    pub fn numbers(&self) -> &NumberFormat {
        &self.numbers
    }

    /// Shows quantities of `dimension` in `unit`, a unit expression of the
    /// catalogue's units such as `cm` or `kN/m`, with `precision` decimals.
    /// Numbers typed without a unit are read in it too.
    pub fn set_display(
        &mut self,
        dimension: Dimension,
        unit: &str,
        precision: u8,
    ) -> Result<(), ParseError> {
        let evaluated = evaluate(unit, &self.catalogue, &self.numbers)?;
        let found = evaluated.quantity.dimension;
        if !evaluated.has_units || found != dimension {
            return Err(self.wrong_dimension(dimension, found));
        }
        let display = DisplayUnit {
            unit: unit.trim().to_owned(),
            precision,
        };
        self.display
            .insert(dimension, (display, evaluated.quantity.value));
        Ok(())
    }

    /// Goes back to showing `dimension` in SI.
    pub fn clear_display(&mut self, dimension: Dimension) {
        self.display.remove(&dimension);
    }

    /// The display unit set for `dimension`, if any.
    pub fn display(&self, dimension: Dimension) -> Option<&DisplayUnit> {
        self.display.get(&dimension).map(|(d, _)| d)
    }

    /// Reads typed input as a quantity of `dimension`, in SI units.
    ///
    /// The input may name units (`3,5 m`, `350 cm`) and combine values with
    /// `+ − × ÷` and parentheses (`3,5 m + 20 cm`). Input without any unit
    /// (`350`, `300 + 50`) is in the display unit. Numbers are written as
    /// the locale writes them.
    pub fn parse(&self, input: &str, dimension: Dimension) -> Result<f64, ParseError> {
        let evaluated = evaluate(input, &self.catalogue, &self.numbers)?;
        let mut q = evaluated.quantity;
        if !evaluated.has_units && q.dimension == Dimension::NONE {
            q.value *= self.display.get(&dimension).map_or(1.0, |(_, f)| *f);
            q.dimension = dimension;
        }
        if q.dimension != dimension {
            return Err(self.wrong_dimension(dimension, q.dimension));
        }
        if !q.value.is_finite() {
            return Err(ParseError::NotFinite);
        }
        Ok(q.value)
    }

    /// Shows an SI value of `dimension` in its display unit, rounded to its
    /// precision, such as `3,70 m`. Without a display unit, the value is
    /// shown in SI with as many digits as it takes to read it back exactly.
    pub fn format(&self, value: f64, dimension: Dimension) -> String {
        if !value.is_finite() {
            return value.to_string();
        }
        let (number, unit) = match self.display.get(&dimension) {
            Some((display, factor)) => {
                let precision = usize::from(display.precision);
                (
                    format!("{:.precision$}", value / factor),
                    display.unit.clone(),
                )
            }
            None => (value.to_string(), self.catalogue.si_symbol(dimension)),
        };
        // Rounding can leave a negative zero, such as -0.00.
        let number = match number.strip_prefix('-') {
            Some(abs) if abs.chars().all(|c| matches!(c, '0' | '.')) => abs,
            _ => &number,
        };
        let number = self.numbers.format_decimal(number);
        match unit.chars().next() {
            None => number,
            // Angles and feet and inches go right after the number.
            Some('°' | '\'' | '"' | '′' | '″') => format!("{number}{unit}"),
            Some(_) => format!("{number} {unit}"),
        }
    }

    fn wrong_dimension(&self, expected: Dimension, found: Dimension) -> ParseError {
        ParseError::WrongDimension {
            expected,
            found,
            expected_unit: self.catalogue.si_symbol(expected),
            found_unit: self.catalogue.si_symbol(found),
        }
    }
}
