//! Locale-aware numbers, from Unicode CLDR data through ICU4X.

use std::fmt;

use icu_decimal::DecimalFormatter;
use icu_decimal::input::Decimal;
use icu_decimal::options::{DecimalFormatterOptions, GroupingStrategy};
use icu_locale_core::Locale;

/// Writes and reads numbers the way a locale does: its decimal separator,
/// digit grouping and digits. `1234.5` is `1.234,5` in es-AR, `1,234.5` in
/// en and `١٬٢٣٤٫٥` in ar-EG.
#[derive(Debug)]
pub struct NumberFormat {
    locale: Locale,
    formatter: DecimalFormatter,
    /// What the locale writes between the integer and fraction digits.
    pub(crate) decimal: Vec<char>,
    /// What the locale writes between digit groups.
    pub(crate) group: Vec<char>,
    /// The locale's digits zero to nine. Plain ASCII digits are read too.
    pub(crate) digits: [char; 10],
}

/// A locale tag that isn't valid BCP 47, such as `es_AR!`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidLocale(pub String);

impl fmt::Display for InvalidLocale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` is not a valid locale tag", self.0)
    }
}

impl std::error::Error for InvalidLocale {}

impl NumberFormat {
    /// The number format of a BCP 47 locale tag, such as `es-AR`. A locale
    /// without CLDR data falls back to its parent, and in the end to CLDR's
    /// root locale.
    pub fn new(tag: &str) -> Result<Self, InvalidLocale> {
        let invalid = || InvalidLocale(tag.to_owned());
        let locale: Locale = tag.parse().map_err(|_| invalid())?;
        let formatter = DecimalFormatter::try_new((&locale).into(), Default::default())
            .map_err(|_| invalid())?;

        // ICU4X doesn't parse numbers, so the symbols are read back from
        // formatted ones.
        let mut options = DecimalFormatterOptions::default();
        options.grouping_strategy = Some(GroupingStrategy::Always);
        let grouping =
            DecimalFormatter::try_new((&locale).into(), options).map_err(|_| invalid())?;
        let shown = |f: &DecimalFormatter, n: &str| -> Vec<char> {
            let decimal: Decimal = n.parse().expect("a valid decimal");
            f.format(&decimal)
                .to_string()
                .chars()
                .filter(|&c| !is_ignorable(c))
                .collect()
        };
        let mut digits = ['0'; 10];
        for (n, digit) in digits.iter_mut().enumerate() {
            if let [c] = shown(&formatter, &n.to_string())[..] {
                *digit = c;
            }
        }
        let is_digit = |c: &char| digits.contains(c);
        // `0.5` is a zero, the separator and a five; `1000000` is digits with
        // a separator after the first one.
        let decimal = shown(&formatter, "0.5")
            .into_iter()
            .filter(|c| !is_digit(c))
            .collect();
        let group = shown(&grouping, "1000000")
            .into_iter()
            .skip_while(is_digit)
            .take_while(|c| !is_digit(c))
            .collect();
        Ok(Self {
            locale,
            formatter,
            decimal,
            group,
            digits,
        })
    }

    /// The locale this format is for.
    pub fn locale(&self) -> &Locale {
        &self.locale
    }

    /// Formats a plain decimal number, such as `-1234.50`, in this locale.
    /// Its digits are kept as given: rounding is up to the caller.
    pub(crate) fn format_decimal(&self, decimal: &str) -> String {
        match decimal.parse::<Decimal>() {
            Ok(d) => self.formatter.format(&d).to_string(),
            Err(_) => decimal.to_owned(),
        }
    }

    /// The value of a locale or ASCII digit.
    pub(crate) fn digit_value(&self, c: char) -> Option<u32> {
        c.to_digit(10)
            .or_else(|| self.digits.iter().position(|&d| d == c).map(|n| n as u32))
    }
}

/// Characters that only steer text direction. Some locales put them around
/// numbers (ar writes a minus sign as U+061C U+002D), and typed input may
/// carry them, so they are skipped.
pub(crate) fn is_ignorable(c: char) -> bool {
    matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_come_from_cldr() {
        let es = NumberFormat::new("es-AR").unwrap();
        assert_eq!(
            (es.decimal.as_slice(), es.group.as_slice()),
            (&[','][..], &['.'][..])
        );
        let en = NumberFormat::new("en").unwrap();
        assert_eq!(
            (en.decimal.as_slice(), en.group.as_slice()),
            (&['.'][..], &[','][..])
        );
        let fr = NumberFormat::new("fr").unwrap();
        assert_eq!(fr.group, ['\u{202f}']);
        let ar = NumberFormat::new("ar-EG").unwrap();
        assert_eq!(ar.digits[7], '٧');
        assert_eq!(ar.digit_value('٧'), Some(7));
        assert_eq!(ar.digit_value('7'), Some(7));
        assert_eq!(es.locale().to_string(), "es-AR");
    }

    #[test]
    fn decimals_are_formatted_as_given() {
        let es = NumberFormat::new("es-AR").unwrap();
        assert_eq!(es.format_decimal("-1234.50"), "-1.234,50");
    }
}
