//! Units of measure as data: a symbol, a dimension and a factor to SI.

use hito_core::Dimension;

/// A unit of measure: `factor` of them is one coherent SI unit of
/// `dimension`'s quantity. A centimetre is `Unit { symbol: "cm", dimension:
/// Dimension::LENGTH, factor: 0.01 }`.
///
/// Only units proportional to SI fit, so degrees Celsius don't (see the
/// crate docs).
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub symbol: String,
    pub dimension: Dimension,
    pub factor: f64,
}

/// The units input can name, looked up by symbol. Symbols are
/// case-sensitive, so `mm` and `Mm` differ.
///
/// The catalogue holds base units only: compound units like `kN/m` or
/// `kg/m³` are written as expressions of them.
#[derive(Debug, Clone, Default)]
pub struct UnitCatalogue {
    units: Vec<Unit>,
}

impl UnitCatalogue {
    /// An empty catalogue.
    pub fn new() -> Self {
        Self::default()
    }

    /// The units in [`STANDARD`]: SI and its common multiples, plus the
    /// metric technical and imperial units engineers still use.
    pub fn standard() -> Self {
        STANDARD
            .iter()
            .fold(Self::new(), |c, &(symbol, dimension, factor)| {
                c.with(symbol, dimension, factor)
            })
    }

    /// Adds a unit, replacing any unit with the same symbol.
    pub fn with(mut self, symbol: &str, dimension: Dimension, factor: f64) -> Self {
        self.units.retain(|u| u.symbol != symbol);
        self.units.push(Unit {
            symbol: symbol.to_owned(),
            dimension,
            factor,
        });
        self
    }

    /// The unit with this symbol.
    pub fn get(&self, symbol: &str) -> Option<&Unit> {
        self.units.iter().find(|u| u.symbol == symbol)
    }

    /// Every unit, in the order they were added.
    pub fn iter(&self) -> impl Iterator<Item = &Unit> {
        self.units.iter()
    }

    /// The first unit added that is the coherent SI unit of `dimension`
    /// (factor 1), such as `N` for a force.
    pub(crate) fn coherent(&self, dimension: Dimension) -> Option<&Unit> {
        self.units
            .iter()
            .find(|u| u.dimension == dimension && u.factor == 1.0)
    }

    /// The symbol of `dimension`'s coherent SI unit: a unit's own (`N`), or
    /// else a product of base units (`kg·m⁻¹·s⁻²`) that reads back as
    /// input. Empty for a pure number.
    pub fn si_symbol(&self, dimension: Dimension) -> String {
        if let Some(unit) = self.coherent(dimension) {
            return unit.symbol.clone();
        }
        let exponents = exponents(dimension);
        let mut parts = Vec::new();
        for (i, (base, fallback)) in BASES.iter().enumerate() {
            let n = exponents[i];
            if n == 0 {
                continue;
            }
            let symbol = self.coherent(*base).map_or(*fallback, |u| &u.symbol);
            let power = match n {
                1 => String::new(),
                n => n.to_string().chars().map(superscript).collect(),
            };
            parts.push(format!("{symbol}{power}"));
        }
        parts.join("·")
    }
}

/// The base dimensions in the order SI writes them in a product, with the
/// symbol to use if the catalogue has no coherent unit for one.
const BASES: [(Dimension, &str); 8] = [
    (Dimension::MASS, "kg"),
    (Dimension::LENGTH, "m"),
    (Dimension::TIME, "s"),
    (
        Dimension {
            current: 1,
            ..Dimension::NONE
        },
        "A",
    ),
    (Dimension::TEMPERATURE, "K"),
    (
        Dimension {
            amount: 1,
            ..Dimension::NONE
        },
        "mol",
    ),
    (
        Dimension {
            luminous_intensity: 1,
            ..Dimension::NONE
        },
        "cd",
    ),
    (Dimension::ANGLE, "rad"),
];

/// A dimension's exponents, in the order of [`BASES`].
pub(crate) fn exponents(d: Dimension) -> [i8; 8] {
    [
        d.mass,
        d.length,
        d.time,
        d.current,
        d.temperature,
        d.amount,
        d.luminous_intensity,
        d.angle,
    ]
}

fn from_exponents(e: [i8; 8]) -> Dimension {
    Dimension {
        mass: e[0],
        length: e[1],
        time: e[2],
        current: e[3],
        temperature: e[4],
        amount: e[5],
        luminous_intensity: e[6],
        angle: e[7],
    }
}

/// `a.mul(b)`, or `None` if an exponent overflows. Input can ask for any
/// exponent, so the core's unchecked arithmetic isn't safe here.
pub(crate) fn checked_mul(a: Dimension, b: Dimension) -> Option<Dimension> {
    let (a, b) = (exponents(a), exponents(b));
    let mut e = [0; 8];
    for (e, (a, b)) in e.iter_mut().zip(a.into_iter().zip(b)) {
        *e = a.checked_add(b)?;
    }
    Some(from_exponents(e))
}

/// `d.pow(n)`, or `None` if an exponent overflows.
pub(crate) fn checked_pow(d: Dimension, n: i8) -> Option<Dimension> {
    let mut e = exponents(d);
    for e in &mut e {
        *e = e.checked_mul(n)?;
    }
    Some(from_exponents(e))
}

const SUPERSCRIPTS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];

fn superscript(c: char) -> char {
    match c {
        '-' => '⁻',
        c => c.to_digit(10).map_or(c, |n| SUPERSCRIPTS[n as usize]),
    }
}

/// The digit a superscript stands for.
pub(crate) fn superscript_value(c: char) -> Option<u32> {
    SUPERSCRIPTS.iter().position(|&s| s == c).map(|n| n as u32)
}

const D: Dimension = Dimension::NONE;
const L: Dimension = Dimension::LENGTH;
const M: Dimension = Dimension::MASS;
const T: Dimension = Dimension::TIME;
const F: Dimension = Dimension::FORCE;
const P: Dimension = Dimension::PRESSURE;
const A: Dimension = Dimension::ANGLE;

/// Standard gravity in m/s², which defines the kilogram-force.
const G: f64 = 9.806_65;
const POUND: f64 = 0.453_592_37;
const INCH: f64 = 0.0254;

/// The units of [`UnitCatalogue::standard`], as (symbol, dimension, factor).
/// The coherent SI unit of each dimension comes first, so it names the
/// dimension when no display unit is set.
pub const STANDARD: &[(&str, Dimension, f64)] = &[
    // SI base units, and radians.
    ("m", L, 1.0),
    ("kg", M, 1.0),
    ("s", T, 1.0),
    (
        "A",
        Dimension {
            current: 1,
            ..Dimension::NONE
        },
        1.0,
    ),
    ("K", Dimension::TEMPERATURE, 1.0),
    (
        "mol",
        Dimension {
            amount: 1,
            ..Dimension::NONE
        },
        1.0,
    ),
    (
        "cd",
        Dimension {
            luminous_intensity: 1,
            ..Dimension::NONE
        },
        1.0,
    ),
    ("rad", A, 1.0),
    // Length.
    ("km", L, 1e3),
    ("dm", L, 0.1),
    ("cm", L, 0.01),
    ("mm", L, 1e-3),
    ("µm", L, 1e-6),
    ("in", L, INCH),
    ("\"", L, INCH),
    ("″", L, INCH),
    ("ft", L, 12.0 * INCH),
    ("'", L, 12.0 * INCH),
    ("′", L, 12.0 * INCH),
    ("yd", L, 36.0 * INCH),
    ("mi", L, 63_360.0 * INCH),
    // Area.
    ("ha", Dimension::AREA, 1e4),
    // Volume.
    ("L", Dimension::VOLUME, 1e-3),
    // Mass.
    ("g", M, 1e-3),
    ("t", M, 1e3),
    ("lb", M, POUND),
    // Time.
    ("min", T, 60.0),
    ("h", T, 3600.0),
    ("d", T, 86_400.0),
    // Force.
    ("N", F, 1.0),
    ("kN", F, 1e3),
    ("MN", F, 1e6),
    ("kgf", F, G),
    ("tf", F, 1e3 * G),
    ("lbf", F, POUND * G),
    ("kip", F, 1e3 * POUND * G),
    // Pressure and stress.
    ("Pa", P, 1.0),
    ("kPa", P, 1e3),
    ("MPa", P, 1e6),
    ("GPa", P, 1e9),
    ("bar", P, 1e5),
    ("psi", P, POUND * G / (INCH * INCH)),
    ("ksi", P, 1e3 * POUND * G / (INCH * INCH)),
    // Angle.
    ("°", A, std::f64::consts::PI / 180.0),
    ("deg", A, std::f64::consts::PI / 180.0),
    ("gon", A, std::f64::consts::PI / 200.0),
    // Ratios.
    ("%", D, 0.01),
    ("‰", D, 0.001),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_symbols_are_unique() {
        let catalogue = UnitCatalogue::standard();
        assert_eq!(catalogue.iter().count(), STANDARD.len());
    }

    #[test]
    fn adding_a_symbol_again_replaces_it() {
        let catalogue =
            UnitCatalogue::new()
                .with("x", Dimension::LENGTH, 2.0)
                .with("x", Dimension::MASS, 3.0);
        assert_eq!(catalogue.iter().count(), 1);
        assert_eq!(catalogue.get("x").unwrap().dimension, Dimension::MASS);
    }

    #[test]
    fn coherent_units_name_dimensions() {
        let catalogue = UnitCatalogue::standard();
        assert_eq!(catalogue.coherent(Dimension::FORCE).unwrap().symbol, "N");
        assert!(catalogue.coherent(Dimension::DENSITY).is_none());
    }
}
