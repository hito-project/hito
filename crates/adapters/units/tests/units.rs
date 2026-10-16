//! Acceptance tests for typed quantity input and display (issue #55).

use hito_core::Dimension;
use hito_units::{NumberFormat, ParseError, UnitCatalogue, Units};

fn units(locale: &str) -> Units {
    Units::new(
        UnitCatalogue::standard(),
        NumberFormat::new(locale).unwrap(),
    )
}

/// Metres with two decimals, kilonewtons and megapascals: a typical
/// structural setup, chosen here by the test as a user would in settings.
fn structural(locale: &str) -> Units {
    let mut u = units(locale);
    u.set_display(Dimension::LENGTH, "m", 2).unwrap();
    u.set_display(Dimension::FORCE, "kN", 1).unwrap();
    u.set_display(Dimension::PRESSURE, "MPa", 1).unwrap();
    u
}

#[track_caller]
fn assert_close(actual: f64, expected: f64) {
    let tolerance = 1e-12 * expected.abs().max(1.0);
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} is not {expected}"
    );
}

#[track_caller]
fn parses_to(u: &Units, input: &str, dimension: Dimension, expected: f64) {
    match u.parse(input, dimension) {
        Ok(value) => assert_close(value, expected),
        Err(e) => panic!("`{input}` didn't parse: {e}"),
    }
}

// The core stores values in SI. Display units and precision are user
// preferences.

#[test]
fn values_are_parsed_into_si_whatever_the_display_unit() {
    let mut u = units("en");
    u.set_display(Dimension::LENGTH, "cm", 1).unwrap();
    parses_to(&u, "350", Dimension::LENGTH, 3.5);
    parses_to(&u, "3.5 m", Dimension::LENGTH, 3.5);
    parses_to(&u, "25 MPa", Dimension::PRESSURE, 25e6);
    parses_to(&u, "45°", Dimension::ANGLE, std::f64::consts::FRAC_PI_4);
}

#[test]
fn display_unit_and_precision_are_preferences() {
    let mut u = units("en");
    u.set_display(Dimension::LENGTH, "m", 2).unwrap();
    assert_eq!(u.format(3.7, Dimension::LENGTH), "3.70 m");
    u.set_display(Dimension::LENGTH, "cm", 0).unwrap();
    assert_eq!(u.format(3.7, Dimension::LENGTH), "370 cm");
    u.set_display(Dimension::LENGTH, "mm", 1).unwrap();
    assert_eq!(u.format(3.7, Dimension::LENGTH), "3,700.0 mm");

    let display = u.display(Dimension::LENGTH).unwrap();
    assert_eq!((display.unit.as_str(), display.precision), ("mm", 1));
    assert_eq!(u.display(Dimension::TIME), None);
}

#[test]
fn display_units_can_be_compound() {
    let mut u = units("en");
    u.set_display(Dimension::FORCE.div(Dimension::LENGTH), "kN/m", 1)
        .unwrap();
    u.set_display(Dimension::DENSITY, "kg/m³", 0).unwrap();
    u.set_display(Dimension::MOMENT, "kN·m", 2).unwrap();
    assert_eq!(
        u.format(12_500.0, Dimension::FORCE.div(Dimension::LENGTH)),
        "12.5 kN/m"
    );
    assert_eq!(u.format(2400.0, Dimension::DENSITY), "2,400 kg/m³");
    assert_eq!(u.format(1500.0, Dimension::MOMENT), "1.50 kN·m");
}

#[test]
fn a_display_unit_must_be_known_and_of_the_right_dimension() {
    let mut u = units("en");
    assert!(matches!(
        u.set_display(Dimension::LENGTH, "furlong", 2),
        Err(ParseError::UnknownUnit { .. })
    ));
    assert!(matches!(
        u.set_display(Dimension::LENGTH, "kN", 2),
        Err(ParseError::WrongDimension { .. })
    ));
    assert_eq!(u.display(Dimension::LENGTH), None);
}

#[test]
fn without_a_display_unit_values_show_in_si() {
    let u = units("en");
    assert_eq!(u.format(3.7, Dimension::LENGTH), "3.7 m");
    assert_eq!(u.format(25e6, Dimension::PRESSURE), "25,000,000 Pa");
    assert_eq!(u.format(1500.0, Dimension::MOMENT), "1,500 kg·m²·s⁻²");
    assert_eq!(u.format(0.25, Dimension::NONE), "0.25");
    parses_to(&u, "1,500 kg·m²·s⁻²", Dimension::MOMENT, 1500.0);
}

// Input parsing accepts units (`3.5m`, `350cm`, `350`) and expressions
// (`3.5m + 20cm`).

#[test]
fn input_accepts_units() {
    let u = structural("en");
    for input in ["3.5m", "350cm", "3500 mm", "3.5 m", "35 dm"] {
        parses_to(&u, input, Dimension::LENGTH, 3.5);
    }
    parses_to(&u, "350", Dimension::LENGTH, 350.0);
    parses_to(&u, "25 kN/m", Dimension::FORCE.div(Dimension::LENGTH), 25e3);
    parses_to(&u, "2400 kg/m^3", Dimension::DENSITY, 2400.0);
    parses_to(&u, "2400 kg/m³", Dimension::DENSITY, 2400.0);
    parses_to(&u, "10 kgf/cm²", Dimension::PRESSURE, 10.0 * 9.80665e4);
    parses_to(&u, "12 kN m", Dimension::MOMENT, 12e3);
    parses_to(&u, "6 kN·m⁻¹", Dimension::FORCE.div(Dimension::LENGTH), 6e3);
    parses_to(
        &u,
        "6 kN m^-1",
        Dimension::FORCE.div(Dimension::LENGTH),
        6e3,
    );
    parses_to(&u, "5 %", Dimension::NONE, 0.05);
}

#[test]
fn a_bare_number_is_in_the_display_unit() {
    let mut u = structural("en");
    parses_to(&u, "350", Dimension::LENGTH, 350.0);
    parses_to(&u, "25", Dimension::PRESSURE, 25e6);
    parses_to(&u, "(300 + 50) / 2", Dimension::FORCE, 175e3);
    u.set_display(Dimension::LENGTH, "cm", 0).unwrap();
    parses_to(&u, "350", Dimension::LENGTH, 3.5);
    // With no display unit, a bare number is in SI.
    parses_to(&u, "2", Dimension::TIME, 2.0);
}

#[test]
fn input_accepts_expressions() {
    let u = structural("en");
    parses_to(&u, "3.5m + 20cm", Dimension::LENGTH, 3.7);
    parses_to(&u, "3.5m - 20cm", Dimension::LENGTH, 3.3);
    parses_to(&u, "(3m + 1m) / 2", Dimension::LENGTH, 2.0);
    parses_to(&u, "2 × 1.5 m", Dimension::LENGTH, 3.0);
    parses_to(&u, "2 * 1.5 m ÷ 3", Dimension::LENGTH, 1.0);
    parses_to(&u, "-1 m + 3 m", Dimension::LENGTH, 2.0);
    parses_to(&u, "−(1 m)", Dimension::LENGTH, -1.0);
    parses_to(&u, "(300 + 50) cm", Dimension::LENGTH, 3.5);
    parses_to(&u, "3 kN·m / 2 m", Dimension::FORCE, 1500.0);
    parses_to(&u, "1 m × 2 m", Dimension::AREA, 2.0);
    parses_to(&u, "3 m / 50 cm", Dimension::NONE, 6.0);
}

#[test]
fn feet_and_inches_can_be_typed() {
    let u = structural("en");
    let ten_six = 10.5 * 0.3048;
    parses_to(&u, "10' 6\"", Dimension::LENGTH, ten_six);
    parses_to(&u, "10'6\"", Dimension::LENGTH, ten_six);
    parses_to(&u, "10 ft 6 in", Dimension::LENGTH, ten_six);
    parses_to(&u, "-10' 6\"", Dimension::LENGTH, -ten_six);
    parses_to(&u, "1 m 20 cm", Dimension::LENGTH, 1.2);
    assert!(matches!(
        u.parse("10' 6", Dimension::LENGTH),
        Err(ParseError::MissingUnit { at: 4 })
    ));
}

#[test]
fn adding_different_dimensions_is_an_error() {
    let u = structural("en");
    let error = u.parse("3.5 m + 2 kN", Dimension::LENGTH).unwrap_err();
    assert_eq!(
        error,
        ParseError::Incompatible {
            at: 6,
            operator: '+',
            left: Dimension::LENGTH,
            right: Dimension::FORCE,
            left_unit: "m".into(),
            right_unit: "N".into(),
        }
    );
    assert_eq!(
        error.to_string(),
        "can't add a value in N to a value in m: they measure different things"
    );

    let error = u.parse("3.5 m - 2", Dimension::LENGTH).unwrap_err();
    assert_eq!(
        error.to_string(),
        "can't subtract a number without units from a value in m: \
         they measure different things"
    );
}

#[test]
fn the_result_must_have_the_properties_dimension() {
    let u = structural("en");
    let error = u.parse("3 kN", Dimension::LENGTH).unwrap_err();
    assert!(matches!(
        error,
        ParseError::WrongDimension {
            expected: Dimension::LENGTH,
            found: Dimension::FORCE,
            ..
        }
    ));
    assert_eq!(error.to_string(), "expected a value in m, got one in N");
    assert!(u.parse("3 m / 2 m", Dimension::LENGTH).is_err());
}

#[test]
fn bad_input_is_reported_with_its_position() {
    let u = structural("en");
    assert_eq!(u.parse("  ", Dimension::LENGTH), Err(ParseError::Empty));
    assert_eq!(
        u.parse("3 parsecs", Dimension::LENGTH),
        Err(ParseError::UnknownUnit {
            at: 2,
            symbol: "parsecs".into()
        })
    );
    assert_eq!(
        u.parse("3 m)", Dimension::LENGTH),
        Err(ParseError::Unexpected {
            at: 3,
            found: ")".into()
        })
    );
    assert_eq!(
        u.parse("(3 m", Dimension::LENGTH),
        Err(ParseError::UnexpectedEnd)
    );
    assert_eq!(
        u.parse("3 m $", Dimension::LENGTH),
        Err(ParseError::Unexpected {
            at: 4,
            found: "$".into()
        })
    );
    assert_eq!(
        u.parse("1 m / 0", Dimension::LENGTH),
        Err(ParseError::NotFinite)
    );
    assert!(matches!(
        u.parse("m^200", Dimension::LENGTH),
        Err(ParseError::ExponentOutOfRange { at: 1 })
    ));
    assert!(u.parse("m^100 * m^100", Dimension::LENGTH).is_err());
}

#[test]
fn the_catalogue_is_data() {
    // A vara, the old Spanish colonial unit, as a user-defined unit.
    let catalogue = UnitCatalogue::standard().with("vara", Dimension::LENGTH, 0.866);
    let u = Units::new(catalogue, NumberFormat::new("es-AR").unwrap());
    parses_to(&u, "10 vara", Dimension::LENGTH, 8.66);
    assert_eq!(u.catalogue().get("vara").unwrap().factor, 0.866);

    let empty = Units::new(UnitCatalogue::new(), NumberFormat::new("en").unwrap());
    assert!(matches!(
        empty.parse("1 m", Dimension::LENGTH),
        Err(ParseError::UnknownUnit { .. })
    ));
}

// Number formatting follows the locale (for example a decimal comma in es-AR).

#[test]
fn numbers_follow_the_locale() {
    let es = structural("es-AR");
    assert_eq!(es.format(1234.5, Dimension::LENGTH), "1.234,50 m");
    assert_eq!(es.format(-0.001, Dimension::LENGTH), "0,00 m");
    parses_to(&es, "3,5m + 20cm", Dimension::LENGTH, 3.7);
    parses_to(&es, "1.234,5 m", Dimension::LENGTH, 1234.5);
    parses_to(&es, "3.500 m", Dimension::LENGTH, 3500.0);
    // In es-AR the point groups thousands, so `3.5` is a mistake, not 3.5.
    assert_eq!(
        es.parse("3.5 m", Dimension::LENGTH),
        Err(ParseError::BadNumber {
            at: 0,
            text: "3.5".into()
        })
    );

    let en = structural("en");
    assert_eq!(en.format(1234.5, Dimension::LENGTH), "1,234.50 m");
    parses_to(&en, "1,234.5 m", Dimension::LENGTH, 1234.5);
    assert!(en.parse("3,5 m", Dimension::LENGTH).is_err());

    let fr = structural("fr");
    assert_eq!(fr.format(1234.5, Dimension::LENGTH), "1\u{202f}234,50 m");
    parses_to(&fr, "1\u{202f}234,5 m", Dimension::LENGTH, 1234.5);
    // A point isn't a group separator in fr, so it can only be a decimal.
    parses_to(&fr, "3.5 m", Dimension::LENGTH, 3.5);
    assert_eq!(es.format(3.0, Dimension::ANGLE), "3 rad");
}

#[test]
fn locales_with_their_own_digits_round_trip() {
    let mut ar = units("ar-EG");
    ar.set_display(Dimension::LENGTH, "m", 2).unwrap();
    let shown = ar.format(1234.5, Dimension::LENGTH);
    assert_eq!(shown, "١٬٢٣٤٫٥٠ m");
    parses_to(&ar, &shown, Dimension::LENGTH, 1234.5);
    parses_to(&ar, "-1234.5 m", Dimension::LENGTH, -1234.5);
    assert_close(
        ar.parse(&ar.format(-2.25, Dimension::LENGTH), Dimension::LENGTH)
            .unwrap(),
        -2.25,
    );
}

#[test]
fn an_invalid_locale_is_an_error() {
    assert!(NumberFormat::new("not a locale!").is_err());
    // An unknown but well-formed locale falls back to CLDR's root data.
    assert!(NumberFormat::new("xx").is_ok());
}

// Round trips over many generated values (property-style, with a small
// deterministic generator so the tests need no extra dependencies).

/// A xorshift generator: deterministic and good enough to spread test cases.
struct Cases(u64);

impl Cases {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A value with a random sign and magnitude between 1e-6 and 1e9.
    fn value(&mut self) -> f64 {
        let mantissa = (self.next() % 1_000_000) as f64 / 1e5;
        let exponent = (self.next() % 12) as i32 - 6;
        let sign = if self.next().is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        sign * mantissa * 10f64.powi(exponent)
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next() % items.len() as u64) as usize]
    }
}

const LOCALES: &[&str] = &["en", "es-AR", "de", "fr", "hi", "ar-EG", "de-CH"];
const LENGTH_UNITS: &[&str] = &["m", "cm", "mm", "km", "ft", "in"];

#[test]
fn formatted_values_parse_back_to_the_rounded_value() {
    let mut cases = Cases(0x5eed);
    for _ in 0..2000 {
        let locale = *cases.pick(LOCALES);
        let unit = *cases.pick(LENGTH_UNITS);
        let precision = (cases.next() % 5) as u8;
        let value = cases.value();

        let mut u = units(locale);
        u.set_display(Dimension::LENGTH, unit, precision).unwrap();
        let shown = u.format(value, Dimension::LENGTH);
        let parsed = u
            .parse(&shown, Dimension::LENGTH)
            .unwrap_or_else(|e| panic!("{locale}: `{shown}` didn't parse back: {e}"));

        // The parsed value is the original one, off by at most half the
        // last shown digit in the display unit.
        let factor = u.catalogue().get(unit).unwrap().factor;
        let half_step = 0.5 * 10f64.powi(-i32::from(precision)) * factor;
        let slack = 1e-9 * value.abs().max(factor);
        assert!(
            (parsed - value).abs() <= half_step + slack,
            "{locale}: {value} shown as `{shown}` parsed back as {parsed}"
        );
        // Showing it again gives the same text.
        assert_eq!(u.format(parsed, Dimension::LENGTH), shown, "{locale}");
    }
}

#[test]
fn values_without_a_display_unit_round_trip_exactly() {
    let mut cases = Cases(0xfeed);
    let dimensions = [
        Dimension::LENGTH,
        Dimension::PRESSURE,
        Dimension::MOMENT,
        Dimension::DENSITY,
        Dimension::NONE,
        Dimension::ANGLE,
    ];
    for _ in 0..2000 {
        let locale = *cases.pick(LOCALES);
        let dimension = *cases.pick(&dimensions);
        let value = cases.value();
        let u = units(locale);
        let shown = u.format(value, dimension);
        assert_eq!(u.parse(&shown, dimension), Ok(value), "{locale}: `{shown}`");
    }
}

#[test]
fn typed_values_parse_in_every_locale() {
    let mut cases = Cases(0xbeef);
    for _ in 0..1000 {
        let locale = *cases.pick(LOCALES);
        let u = units(locale);
        let a = (cases.next() % 100_000) as f64 / 100.0;
        let b = cases.next() % 1000;
        let typed = |x: f64| u.format(x, Dimension::NONE);
        let input = format!("{} m + {} cm", typed(a), typed(b as f64));
        parses_to(&u, &input, Dimension::LENGTH, a + b as f64 / 100.0);
    }
}
