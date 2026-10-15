//! Property tests: random reals and strings are encoded in Part 21 form and
//! read back unchanged. A fixed xorshift sequence keeps them reproducible
//! without a dependency.

use hito_step::{encode_string, format_real};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Reads a Part 21 REAL: `[sign] digit {digit} "." {digit} ["E" [sign] digit {digit}]`.
/// Panics if `s` doesn't follow the grammar.
fn read_real(s: &str) -> f64 {
    let unsigned = s.strip_prefix(['+', '-']).unwrap_or(s);
    let (mantissa, exponent) = match unsigned.split_once('E') {
        Some((m, e)) => (m, Some(e)),
        None => (unsigned, None),
    };
    let (int, frac) = mantissa
        .split_once('.')
        .unwrap_or_else(|| panic!("{s}: no decimal point"));
    assert!(!int.is_empty(), "{s}: no digit before the point");
    assert!(
        int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()),
        "{s}"
    );
    if let Some(e) = exponent {
        let digits = e.strip_prefix(['+', '-']).unwrap_or(e);
        assert!(
            !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()),
            "{s}"
        );
    }
    s.parse().unwrap_or_else(|e| panic!("{s}: {e}"))
}

#[test]
fn reals_read_back_bit_exact() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut checked = 0;
    while checked < 200_000 {
        // Half arbitrary bit patterns (every exponent, subnormals), half
        // model-like values (millimetres to kilometres, a few decimals).
        let x = if checked % 2 == 0 {
            f64::from_bits(rng.next())
        } else {
            let sign = if rng.below(2) == 0 { 1.0 } else { -1.0 };
            sign * rng.below(10_000_000_000) as f64 / 10f64.powi(rng.below(7) as i32)
        };
        let Some(s) = format_real(x) else {
            assert!(!x.is_finite());
            continue;
        };
        assert_eq!(read_real(&s).to_bits(), x.to_bits(), "{x:?} written as {s}");
        checked += 1;
    }
}

#[test]
fn special_reals_read_back_bit_exact() {
    for x in [
        0.0,
        -0.0,
        f64::MIN_POSITIVE,
        5e-324,
        -5e-324,
        f64::MAX,
        f64::MIN,
        f64::EPSILON,
        1e15,
        1e16,
        1e17,
        123_456_789_012_345_680.0,
        0.1,
        0.000_01,
        0.000_1,
    ] {
        let s = format_real(x).unwrap();
        assert_eq!(read_real(&s).to_bits(), x.to_bits(), "{x:?} written as {s}");
    }
}

/// Reads the contents of a Part 21 STRING (without its quotes), following
/// the `''`, `\\`, `\X2\` and `\X4\` rules. Panics on anything else.
fn read_string(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    let hex_run = |i: &mut usize, width: usize, out: &mut String| {
        while !s[*i..].starts_with("\\X0\\") {
            let code = u32::from_str_radix(&s[*i..*i + width], 16).unwrap();
            out.push(char::from_u32(code).unwrap());
            *i += width;
        }
        *i += 4;
    };
    while i < b.len() {
        match b[i] {
            b'\'' => {
                assert_eq!(b[i + 1], b'\'', "lone quote in {s}");
                out.push('\'');
                i += 2;
            }
            b'\\' if s[i..].starts_with("\\\\") => {
                out.push('\\');
                i += 2;
            }
            b'\\' if s[i..].starts_with("\\X2\\") => {
                i += 4;
                hex_run(&mut i, 4, &mut out);
            }
            b'\\' if s[i..].starts_with("\\X4\\") => {
                i += 4;
                hex_run(&mut i, 8, &mut out);
            }
            b' '..=b'~' if b[i] != b'\\' => {
                out.push(char::from(b[i]));
                i += 1;
            }
            other => panic!("unexpected byte {other:#x} in {s}"),
        }
    }
    out
}

#[test]
fn strings_read_back_unchanged() {
    // Characters a model is likely to hold, and the awkward ones.
    let pool: Vec<char> = "aZ09 '\\\"$*#().,;=-_/ñÑáéíóúÁÉÍÓÚüÜ°²³×µ¿¡€\n\t\u{7F}\u{80}\u{FFFF}🏗𝔸"
        .chars()
        .collect();
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    for _ in 0..20_000 {
        let len = rng.below(24) as usize;
        let text: String = (0..len)
            .map(|_| {
                if rng.below(4) == 0 {
                    // Any scalar value at all.
                    loop {
                        if let Some(c) = char::from_u32(rng.below(0x11_0000) as u32) {
                            break c;
                        }
                    }
                } else {
                    pool[rng.below(pool.len() as u64) as usize]
                }
            })
            .collect();
        let encoded = encode_string(&text);
        assert!(
            encoded.bytes().all(|b| (b' '..=b'~').contains(&b)),
            "{encoded}"
        );
        assert_eq!(read_string(&encoded), text, "{text:?} written as {encoded}");
    }
}

#[test]
fn spanish_names_survive() {
    for name in [
        "Hormigón armado H-30",
        "Viga de fundación VF-1",
        "Planta baja — Núcleo",
        "Año 2026, Ñuñoa",
        "Acero ADN 420 (f'y = 420 MPa)",
        "Columna C1 ¿revisar?",
    ] {
        assert_eq!(read_string(&encode_string(name)), name);
    }
}
