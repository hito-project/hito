//! Property values and their data types.

use crate::Dimension;

/// The kind of value a property holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataType {
    /// True or false.
    Boolean,
    /// A whole number, such as a count.
    Integer,
    /// A pure number, such as a ratio.
    Real,
    /// A physical quantity, stored as a [`Value::Real`] in coherent SI units
    /// (see [`Dimension`]).
    Quantity(Dimension),
    /// Text, such as a concrete grade's name. Text values are data, not
    /// interface text, so they aren't translated.
    Text,
}

impl DataType {
    /// Whether a property of this type can hold `value`. Numbers must be
    /// finite.
    pub fn accepts(self, value: &Value) -> bool {
        match (self, value) {
            (Self::Boolean, Value::Boolean(_))
            | (Self::Integer, Value::Integer(_))
            | (Self::Text, Value::Text(_)) => true,
            (Self::Real | Self::Quantity(_), Value::Real(x)) => x.is_finite(),
            _ => false,
        }
    }
}

/// A property's value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Boolean(bool),
    Integer(i64),
    /// A pure number, or a quantity in coherent SI units.
    Real(f64),
    Text(String),
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Self::Real(value)
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantities_hold_finite_reals_only() {
        let length = DataType::Quantity(Dimension::LENGTH);
        assert!(length.accepts(&Value::Real(3.0)));
        assert!(!length.accepts(&Value::Integer(3)));
        assert!(!length.accepts(&Value::Real(f64::NAN)));
        assert!(!length.accepts(&Value::Real(f64::INFINITY)));
        assert!(DataType::Text.accepts(&"H-25".into()));
        assert!(!DataType::Boolean.accepts(&"true".into()));
    }
}
