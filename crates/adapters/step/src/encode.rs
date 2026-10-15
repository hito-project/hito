//! The Part 21 encodings of single values: reals, strings and keywords.

use std::fmt::Write as _;

/// Encodes a REAL: always with a decimal point, the exponent written `E`,
/// such as `3.`, `0.2`, `-0.` or `1.E-5`.
///
/// It uses Rust's shortest representation that reads back as the same
/// number, so reading the text as an `f64` gives back the same bits.
/// Returns `None` for NaN and infinities, which Part 21 can't write.
pub fn format_real(x: f64) -> Option<String> {
    let mut out = String::new();
    push_real(&mut out, x).then_some(out)
}

/// Appends `x` encoded as a REAL. Returns false, appending nothing, if it
/// isn't finite.
pub(crate) fn push_real(out: &mut String, x: f64) -> bool {
    if !x.is_finite() {
        return false;
    }
    let start = out.len();
    // Debug is the shortest round-trip form: "0.2", "3.0", "1e-5", "1.5e300".
    write!(out, "{x:?}").expect("writing to a String can't fail");
    let exponent = out[start..].find('e').map(|i| start + i);
    let mantissa_end = exponent.unwrap_or(out.len());
    let mantissa = &out[start..mantissa_end];
    if let Some(stripped) = mantissa.strip_suffix(".0") {
        // "3.0" -> "3."
        let trailing_zero = start + stripped.len() + 1;
        out.remove(trailing_zero);
    } else if !mantissa.contains('.') {
        // "1e-5" -> "1.e-5"
        out.insert(mantissa_end, '.');
    }
    if let Some(e) = out[start..].find('e') {
        out.replace_range(start + e..=start + e, "E");
    }
    true
}

/// Encodes the contents of a STRING, without the enclosing quotes.
///
/// Printable ASCII is written as is, except that `'` and `\` are doubled.
/// Anything else (accents, `ñ`, `²`, control characters) is written as
/// hexadecimal runs: `\X2\…\X0\` with four digits per character in the
/// Basic Multilingual Plane, `\X4\…\X0\` with eight digits beyond it.
/// So `Hormigón` becomes `Hormig\X2\00F3\X0\n`, and any reader that follows
/// Part 21 gets the same text back.
pub fn encode_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    push_string(&mut out, s);
    out
}

/// Appends the encoded contents of a STRING, without the quotes.
pub(crate) fn push_string(out: &mut String, s: &str) {
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if is_printable(c) {
            match c {
                '\'' => out.push_str("''"),
                '\\' => out.push_str("\\\\"),
                _ => out.push(c),
            }
            continue;
        }
        // A run of escaped characters of the same width shares one directive.
        let wide = is_wide(c);
        out.push_str(if wide { "\\X4\\" } else { "\\X2\\" });
        push_hex(out, c, wide);
        while let Some(&next) = chars.peek() {
            if is_printable(next) || is_wide(next) != wide {
                break;
            }
            push_hex(out, next, wide);
            chars.next();
        }
        out.push_str("\\X0\\");
    }
}

fn is_printable(c: char) -> bool {
    (' '..='~').contains(&c)
}

fn is_wide(c: char) -> bool {
    u32::from(c) > 0xFFFF
}

fn push_hex(out: &mut String, c: char, wide: bool) {
    let code = u32::from(c);
    let written = if wide {
        write!(out, "{code:08X}")
    } else {
        write!(out, "{code:04X}")
    };
    written.expect("writing to a String can't fail");
}

/// Whether `name` is a standard keyword, the form of entity, type and
/// enumeration names: an uppercase letter or `_`, then uppercase letters,
/// digits or `_`. `IFCWALL` and `NOTDEFINED` are keywords; `IfcWall` isn't.
pub fn is_keyword(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_uppercase() || b == b'_')
        && bytes.all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    // Carried over from spike #3 (tag spike/3-ifc-export, src/step.rs).
    #[test]
    fn reals() {
        let real = |x| format_real(x).unwrap();
        assert_eq!(real(3.0), "3.");
        assert_eq!(real(0.2), "0.2");
        assert_eq!(real(-0.0), "-0.");
        assert_eq!(real(1e-5), "1.E-5");
        assert_eq!(real(6_000_000.125), "6000000.125");
        assert_eq!(real(1.5e300), "1.5E300");
        for x in [
            0.1 + 0.2,
            std::f64::consts::PI,
            1.0 / 3.0,
            6_123_456.789_012_3,
        ] {
            let s = real(x).replace('E', "e");
            assert_eq!(s.parse::<f64>().unwrap().to_bits(), x.to_bits());
        }
    }

    #[test]
    fn real_extremes() {
        let real = |x| format_real(x).unwrap();
        assert_eq!(real(1e16), "1.E16");
        assert_eq!(real(-2.5e-7), "-2.5E-7");
        assert_eq!(real(f64::MAX), "1.7976931348623157E308");
        assert_eq!(real(f64::MIN_POSITIVE), "2.2250738585072014E-308");
        assert_eq!(real(5e-324), "5.E-324");
        assert_eq!(format_real(f64::NAN), None);
        assert_eq!(format_real(f64::INFINITY), None);
        assert_eq!(format_real(f64::NEG_INFINITY), None);
    }

    // Carried over from spike #3 (tag spike/3-ifc-export, src/step.rs).
    #[test]
    fn strings() {
        assert_eq!(encode_string("Planta baja"), "Planta baja");
        assert_eq!(encode_string("O'Higgins"), "O''Higgins");
        assert_eq!(encode_string("a\\b"), "a\\\\b");
        assert_eq!(encode_string("Hormigón"), "Hormig\\X2\\00F3\\X0\\n");
        assert_eq!(
            encode_string("Ñandú"),
            "\\X2\\00D1\\X0\\and\\X2\\00FA\\X0\\"
        );
        assert_eq!(encode_string("m²"), "m\\X2\\00B2\\X0\\");
        assert_eq!(encode_string("🏗"), "\\X4\\0001F3D7\\X0\\");
    }

    #[test]
    fn string_runs_and_controls() {
        assert_eq!(encode_string(""), "");
        // One directive for a run, a new one when the width changes.
        assert_eq!(encode_string("ñó"), "\\X2\\00F100F3\\X0\\");
        assert_eq!(encode_string("ñ🏗"), "\\X2\\00F1\\X0\\\\X4\\0001F3D7\\X0\\");
        assert_eq!(encode_string("a\nb"), "a\\X2\\000A\\X0\\b");
        assert_eq!(encode_string("\u{7F}"), "\\X2\\007F\\X0\\");
        assert_eq!(encode_string("''"), "''''");
    }

    #[test]
    fn keywords() {
        for ok in [
            "IFCWALL",
            "T",
            "NOTDEFINED",
            "IFCPOSITIVELENGTHMEASURE",
            "_X",
            "A1_B",
        ] {
            assert!(is_keyword(ok), "{ok}");
        }
        for bad in ["", "IfcWall", "1A", ".T.", "A B", "!USER", "AÑO", "A-B"] {
            assert!(!is_keyword(bad), "{bad}");
        }
    }
}
