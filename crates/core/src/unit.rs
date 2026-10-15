//! Physical dimensions of property values
//! ([ADR 0008](https://github.com/hito-project/hito/blob/main/docs/decisions/0008-argentina-first-international-by-design.md)).
//!
//! The core stores every quantity as a plain number in **coherent SI units**:
//! metres, kilograms, seconds, amperes, kelvin, moles, candelas and radians.
//! A length of 3 m is stored as `3.0`, a concrete strength of 25 MPa as
//! `25e6` (pascals). Showing values in the user's units (millimetres,
//! MPa, kgf/cm²) is a display preference, never part of the stored data.
//!
//! A property's [`Dimension`] says what kind of quantity it holds, so values
//! can be converted and checked without the core knowing any unit names.

/// The physical dimension of a quantity, as exponents of the SI base
/// quantities plus plane angle.
///
/// Angle is kept as its own base, although SI treats it as a ratio, so a
/// rotation and a slope stay different kinds of value.
///
/// Domains build any dimension they need with [`Dimension::mul`],
/// [`Dimension::div`] and [`Dimension::pow`]; the constants are shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Dimension {
    pub length: i8,
    pub mass: i8,
    pub time: i8,
    pub current: i8,
    pub temperature: i8,
    pub amount: i8,
    pub luminous_intensity: i8,
    pub angle: i8,
}

impl Dimension {
    /// A pure number, such as a ratio or a count.
    pub const NONE: Self = Self {
        length: 0,
        mass: 0,
        time: 0,
        current: 0,
        temperature: 0,
        amount: 0,
        luminous_intensity: 0,
        angle: 0,
    };
    /// Metres.
    pub const LENGTH: Self = Self {
        length: 1,
        ..Self::NONE
    };
    /// Kilograms.
    pub const MASS: Self = Self {
        mass: 1,
        ..Self::NONE
    };
    /// Seconds.
    pub const TIME: Self = Self {
        time: 1,
        ..Self::NONE
    };
    /// Kelvin.
    pub const TEMPERATURE: Self = Self {
        temperature: 1,
        ..Self::NONE
    };
    /// Radians.
    pub const ANGLE: Self = Self {
        angle: 1,
        ..Self::NONE
    };
    /// Square metres.
    pub const AREA: Self = Self::LENGTH.pow(2);
    /// Cubic metres.
    pub const VOLUME: Self = Self::LENGTH.pow(3);
    /// Newtons: kg·m/s².
    pub const FORCE: Self = Self::MASS.mul(Self::LENGTH).div(Self::TIME.pow(2));
    /// Pascals: N/m². Also stress.
    pub const PRESSURE: Self = Self::FORCE.div(Self::AREA);
    /// Newton metres. Also energy.
    pub const MOMENT: Self = Self::FORCE.mul(Self::LENGTH);
    /// kg/m³.
    pub const DENSITY: Self = Self::MASS.div(Self::VOLUME);

    /// The dimension of a product of quantities with these dimensions.
    pub const fn mul(self, other: Self) -> Self {
        Self {
            length: self.length + other.length,
            mass: self.mass + other.mass,
            time: self.time + other.time,
            current: self.current + other.current,
            temperature: self.temperature + other.temperature,
            amount: self.amount + other.amount,
            luminous_intensity: self.luminous_intensity + other.luminous_intensity,
            angle: self.angle + other.angle,
        }
    }

    /// The dimension of a quotient of quantities with these dimensions.
    pub const fn div(self, other: Self) -> Self {
        self.mul(other.pow(-1))
    }

    /// The dimension of this quantity raised to a power.
    pub const fn pow(self, n: i8) -> Self {
        Self {
            length: self.length * n,
            mass: self.mass * n,
            time: self.time * n,
            current: self.current * n,
            temperature: self.temperature * n,
            amount: self.amount * n,
            luminous_intensity: self.luminous_intensity * n,
            angle: self.angle * n,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_dimensions_match_their_si_definitions() {
        // A pascal is kg·m⁻¹·s⁻².
        let pascal = Dimension {
            mass: 1,
            length: -1,
            time: -2,
            ..Dimension::NONE
        };
        assert_eq!(Dimension::PRESSURE, pascal);
        assert_eq!(Dimension::MOMENT.div(Dimension::LENGTH), Dimension::FORCE);
        assert_eq!(Dimension::AREA.div(Dimension::AREA), Dimension::NONE);
        assert_ne!(Dimension::ANGLE, Dimension::NONE);
    }
}
