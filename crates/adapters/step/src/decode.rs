//! Decoding Part 21 text: whitespace and comments, keywords, values and
//! strings.
//!
//! Every function takes the whole file and a position in it, and returns a
//! [`Fault`] with the byte offset where something is wrong. The reader turns
//! offsets into line numbers, only when there is an error. Nothing here
//! indexes out of bounds or allocates in proportion to a number in the file.

use std::borrow::Cow;

use crate::reader::ReadErrorKind;
use crate::value::{EntityId, Value};

/// How deeply lists and typed values may nest, counting the attribute list
/// itself. IFC needs four at most (a list of lists of points, in an entity's
/// attribute list); this bound keeps a hostile file from exhausting the stack.
pub const MAX_DEPTH: usize = 32;

/// What is wrong, and where: a byte offset in the file.
#[derive(Debug)]
pub(crate) struct Fault {
    pub at: usize,
    pub kind: ReadErrorKind,
}

pub(crate) type Parse<T> = Result<T, Fault>;

pub(crate) fn fault<T>(at: usize, kind: ReadErrorKind) -> Parse<T> {
    Err(Fault { at, kind })
}

fn syntax<T>(at: usize, what: &'static str) -> Parse<T> {
    fault(at, ReadErrorKind::Syntax(what))
}

/// Skips whitespace and `/* comments */`.
pub(crate) fn skip_space(data: &[u8], p: &mut usize) -> Parse<()> {
    loop {
        match data.get(*p) {
            Some(b) if b.is_ascii_whitespace() => *p += 1,
            Some(b'/') if data.get(*p + 1) == Some(&b'*') => *p = skip_comment(data, *p)?,
            _ => return Ok(()),
        }
    }
}

/// From the `/` of a comment, returns the offset just past its `*/`.
pub(crate) fn skip_comment(data: &[u8], open: usize) -> Parse<usize> {
    let body = data.get(open + 2..).unwrap_or_default();
    match body.windows(2).position(|w| w == b"*/") {
        Some(k) => Ok(open + 2 + k + 2),
        None => fault(open, ReadErrorKind::Unterminated("comment")),
    }
}

/// Skips whitespace, then expects `byte`.
pub(crate) fn expect(data: &[u8], p: &mut usize, byte: u8, what: &'static str) -> Parse<()> {
    skip_space(data, p)?;
    if data.get(*p) == Some(&byte) {
        *p += 1;
        Ok(())
    } else {
        syntax(*p, what)
    }
}

/// Reads a keyword at `p`: a letter or `_`, then letters, digits or `_`, with
/// a `!` in front for user-defined ones. Lowercase is accepted, though
/// standard keywords are uppercase. Returns `None`, moving nothing, if there
/// is no keyword at `p`.
pub(crate) fn keyword<'a>(data: &'a [u8], p: &mut usize) -> Option<&'a str> {
    let start = *p;
    let mut i = start + usize::from(data.get(start) == Some(&b'!'));
    if !data
        .get(i)
        .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
    {
        return None;
    }
    while data
        .get(i)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        i += 1;
    }
    // ASCII by construction, so this never fails.
    let name = std::str::from_utf8(&data[start..i]).ok()?;
    *p = i;
    Some(name)
}

/// Reads an unsigned decimal number. `None` if there are no digits; an error
/// if it doesn't fit in a `u64`.
pub(crate) fn unsigned(data: &[u8], p: &mut usize) -> Parse<Option<u64>> {
    let start = *p;
    let mut n: u64 = 0;
    while let Some(&b) = data.get(*p).filter(|b| b.is_ascii_digit()) {
        n = match n
            .checked_mul(10)
            .and_then(|n| n.checked_add(u64::from(b - b'0')))
        {
            Some(n) => n,
            None => return fault(start, ReadErrorKind::NumberOutOfRange),
        };
        *p += 1;
    }
    Ok((*p > start).then_some(n))
}

/// Reads an instance name, `#123`, from its `#` at `p`.
pub(crate) fn instance_name(data: &[u8], p: &mut usize) -> Parse<EntityId> {
    let hash = *p;
    *p += 1;
    match unsigned(data, p)? {
        Some(0) => syntax(hash, "instance names start at #1"),
        Some(n) => Ok(EntityId::new(n)),
        None => syntax(hash, "expected an instance name after `#`"),
    }
}

/// From an opening quote, returns the offset just past the closing one.
/// A doubled quote `''` is part of the string.
pub(crate) fn skip_string(data: &[u8], open: usize) -> Parse<usize> {
    let mut i = open + 1;
    loop {
        let rest = data.get(i..).unwrap_or_default();
        match rest.iter().position(|&b| b == b'\'') {
            None => return fault(open, ReadErrorKind::Unterminated("string")),
            Some(k) if data.get(i + k + 1) == Some(&b'\'') => i += k + 2,
            Some(k) => return Ok(i + k + 1),
        }
    }
}

/// From an opening `"`, returns the offset just past the closing one.
pub(crate) fn skip_binary(data: &[u8], open: usize) -> Parse<usize> {
    let rest = data.get(open + 1..).unwrap_or_default();
    match rest.iter().position(|&b| b == b'"') {
        Some(k) => Ok(open + 1 + k + 1),
        None => fault(open, ReadErrorKind::Unterminated("binary")),
    }
}

/// Reads a parenthesized list of values, `(a,b,c)`, from its `(` at `p`.
/// `depth` is the number of lists and typed values around it.
pub(crate) fn list(data: &[u8], p: &mut usize, depth: usize) -> Parse<Vec<Value>> {
    let open = *p;
    if data.get(open) != Some(&b'(') {
        return syntax(open, "expected `(`");
    }
    if depth >= MAX_DEPTH {
        return fault(open, ReadErrorKind::TooDeep);
    }
    *p += 1;
    let mut items = Vec::new();
    skip_space(data, p)?;
    if data.get(*p) == Some(&b')') {
        *p += 1;
        return Ok(items);
    }
    loop {
        items.push(value(data, p, depth + 1)?);
        skip_space(data, p)?;
        match data.get(*p) {
            Some(b',') => *p += 1,
            Some(b')') => {
                *p += 1;
                return Ok(items);
            }
            _ => return syntax(*p, "expected `,` or `)`"),
        }
    }
}

/// Reads one value at `p`, after any whitespace.
fn value(data: &[u8], p: &mut usize, depth: usize) -> Parse<Value> {
    skip_space(data, p)?;
    let at = *p;
    match data.get(at) {
        Some(b'(') => list(data, p, depth).map(Value::List),
        Some(b'$') => {
            *p += 1;
            Ok(Value::Null)
        }
        Some(b'*') => {
            *p += 1;
            Ok(Value::Derived)
        }
        Some(b'#') => instance_name(data, p).map(Value::Reference),
        Some(b'\'') => {
            let end = skip_string(data, at)?;
            *p = end;
            decode_string(data, at + 1, end - 1).map(Value::String)
        }
        Some(b'"') => fault(at, ReadErrorKind::Unsupported("binary values")),
        Some(b'.') => enumeration(data, p),
        Some(b'+' | b'-' | b'0'..=b'9') => number(data, p),
        Some(_) => match keyword(data, p) {
            Some(name) => typed(data, p, name, depth),
            None => syntax(at, "expected a value"),
        },
        None => syntax(at, "expected a value"),
    }
}

/// `.NAME.`, with `.T.` and `.F.` read as booleans.
fn enumeration(data: &[u8], p: &mut usize) -> Parse<Value> {
    let at = *p;
    *p += 1;
    let name = match keyword(data, p) {
        Some(name) if !name.starts_with('!') && data.get(*p) == Some(&b'.') => name,
        _ => return syntax(at, "a malformed enumeration"),
    };
    *p += 1;
    Ok(match name {
        "T" => Value::Boolean(true),
        "F" => Value::Boolean(false),
        _ => Value::Enumeration(Cow::Owned(name.to_owned())),
    })
}

/// `NAME(value)`, after the name.
fn typed(data: &[u8], p: &mut usize, name: &str, depth: usize) -> Parse<Value> {
    skip_space(data, p)?;
    if data.get(*p) != Some(&b'(') {
        return syntax(*p, "expected `(`");
    }
    if depth >= MAX_DEPTH {
        return fault(*p, ReadErrorKind::TooDeep);
    }
    *p += 1;
    let inner = value(data, p, depth + 1)?;
    expect(data, p, b')', "expected `)`")?;
    Ok(Value::Typed(Cow::Owned(name.to_owned()), Box::new(inner)))
}

/// An INTEGER, `[sign] digits`, or a REAL, `[sign] digits . [digits]
/// [E [sign] digits]`. An exponent without a decimal point is accepted as a
/// REAL too.
fn number(data: &[u8], p: &mut usize) -> Parse<Value> {
    let start = *p;
    let digits = |i: &mut usize| {
        let from = *i;
        while data.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i > from
    };
    let mut i = start + usize::from(matches!(data.get(start), Some(b'+' | b'-')));
    let mut ok = digits(&mut i);
    let mut real = false;
    if data.get(i) == Some(&b'.') {
        real = true;
        i += 1;
        digits(&mut i);
    }
    if matches!(data.get(i), Some(b'E' | b'e')) {
        real = true;
        i += 1;
        i += usize::from(matches!(data.get(i), Some(b'+' | b'-')));
        ok &= digits(&mut i);
    }
    // A number runs into the next token only if it's malformed: `1.2.3`, `1A`.
    if !ok
        || data
            .get(i)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'.')
    {
        return syntax(start, "a malformed number");
    }
    *p = i;
    // Signs, digits, `.` and `E`, so ASCII.
    let text = std::str::from_utf8(&data[start..i]).unwrap_or_default();
    if real {
        match text.parse::<f64>() {
            Ok(x) if x.is_finite() => Ok(Value::Real(x)),
            Ok(_) => fault(start, ReadErrorKind::NumberOutOfRange),
            Err(_) => syntax(start, "a malformed number"),
        }
    } else {
        // The syntax is right, so the only way to fail is to overflow.
        text.parse::<i64>()
            .map(Value::Integer)
            .or_else(|_| fault(start, ReadErrorKind::NumberOutOfRange))
    }
}

/// Decodes the contents of a STRING, `data[start..end]`, between its quotes.
///
/// It reads what [`encode_string`](crate::encode_string) writes (`''`, `\\`,
/// `\X2\…\X0\` and `\X4\…\X0\`), and what other exporters write:
///
/// - `\X2\` runs holding UTF-16 surrogate pairs, for characters beyond the
///   Basic Multilingual Plane.
/// - `\X\hh` and `\S\c`, 8-bit characters in ISO 8859-1. Code page switches
///   (`\PA\` to `\PI\`) are skipped, so `\S\` is always read as ISO 8859-1.
/// - Bytes above ASCII, which the standard doesn't allow but some exporters
///   write as UTF-8. Invalid UTF-8 becomes U+FFFD.
/// - A backslash that starts no escape, such as in a Windows path, is kept.
///
/// A malformed `\X` escape (bad hex digits, a lone surrogate, a code point
/// beyond Unicode, a run without its `\X0\`) is an error: it means the file is
/// damaged, and guessing would silently change the text.
fn decode_string(data: &[u8], start: usize, end: usize) -> Parse<String> {
    let raw = data.get(start..end).unwrap_or_default();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    let bad = |i: usize| syntax(start + i, "a malformed string escape");
    while let Some(&b) = raw.get(i) {
        match b {
            // `skip_string` only lets doubled quotes through.
            b'\'' => {
                out.push('\'');
                i += 2;
            }
            b'\\' => {
                let rest = &raw[i..];
                if rest.starts_with(b"\\\\") {
                    out.push('\\');
                    i += 2;
                } else if rest.starts_with(b"\\X2\\") || rest.starts_with(b"\\X4\\") {
                    let width = if rest[2] == b'2' { 4 } else { 8 };
                    match hex_run(raw, i + 4, width, &mut out) {
                        Some(next) => i = next,
                        None => return bad(i),
                    }
                } else if rest.starts_with(b"\\X\\") {
                    match rest.get(3..5).and_then(hex) {
                        Some(code) => out.push(char::from(code as u8)),
                        None => return bad(i),
                    }
                    i += 5;
                } else if rest.starts_with(b"\\S\\") {
                    match rest.get(3) {
                        Some(&c @ b' '..=b'~') => {
                            out.push(char::from(c + 0x80));
                            // A quote is doubled, like anywhere in a string.
                            i += if c == b'\'' { 5 } else { 4 };
                        }
                        _ => return bad(i),
                    }
                } else if rest.len() >= 4
                    && rest[1] == b'P'
                    && (b'A'..=b'I').contains(&rest[2])
                    && rest[3] == b'\\'
                {
                    i += 4;
                } else {
                    out.push('\\');
                    i += 1;
                }
            }
            0..0x80 => {
                out.push(char::from(b));
                i += 1;
            }
            _ => {
                let run = raw[i..]
                    .iter()
                    .position(|&b| b < 0x80)
                    .map_or(raw.len(), |k| i + k);
                out.push_str(&String::from_utf8_lossy(&raw[i..run]));
                i = run;
            }
        }
    }
    Ok(out)
}

/// Decodes the hex digits of a `\X2\` (`width` 4, UTF-16) or `\X4\` (`width`
/// 8, UTF-32) run from `i` and its closing `\X0\`. Returns the offset after
/// the `\X0\`, or `None` if the run is malformed.
fn hex_run(raw: &[u8], mut i: usize, width: usize, out: &mut String) -> Option<usize> {
    let mut high_surrogate = None;
    loop {
        let rest = raw.get(i..)?;
        if rest.starts_with(b"\\X0\\") {
            return high_surrogate.is_none().then_some(i + 4);
        }
        let code = hex(rest.get(..width)?)?;
        i += width;
        let c = match (width, high_surrogate.take()) {
            (8, _) => char::from_u32(code)?,
            (_, None) if (0xD800..0xDC00).contains(&code) => {
                high_surrogate = Some(code);
                continue;
            }
            (_, None) => char::from_u32(code)?,
            (_, Some(high)) if (0xDC00..0xE000).contains(&code) => {
                char::from_u32(0x10000 + ((high - 0xD800) << 10) + (code - 0xDC00))?
            }
            (_, Some(_)) => return None,
        };
        out.push(c);
    }
}

/// Up to 8 hex digits as a number; `None` if any isn't a hex digit.
fn hex(digits: &[u8]) -> Option<u32> {
    digits.iter().try_fold(0u32, |n, &d| {
        let v = char::from(d).to_digit(16)?;
        Some((n << 4) | v)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(s: &str) -> Option<String> {
        decode_string(s.as_bytes(), 0, s.len()).ok()
    }

    #[test]
    fn surrogates_must_pair() {
        assert_eq!(decode("\\X2\\D83CDFD7\\X0\\").as_deref(), Some("🏗"));
        assert_eq!(decode("\\X2\\DFD7D83C\\X0\\"), None);
        assert_eq!(decode("\\X2\\D83C0041\\X0\\"), None);
        assert_eq!(decode("\\X2\\D83CD83C\\X0\\"), None);
    }

    #[test]
    fn hex_digits() {
        assert_eq!(hex(b"00F3"), Some(0xF3));
        assert_eq!(hex(b"ffffFFFF"), Some(u32::MAX));
        assert_eq!(hex(b"0x12"), None);
        assert_eq!(hex(b"+1"), None);
    }

    #[test]
    fn keywords() {
        let read = |s: &str| {
            let mut p = 0;
            keyword(s.as_bytes(), &mut p).map(|k| (k.to_owned(), p))
        };
        assert_eq!(read("IFCWALL("), Some(("IFCWALL".into(), 7)));
        assert_eq!(read("!USER_1 "), Some(("!USER_1".into(), 7)));
        assert_eq!(read("_a"), Some(("_a".into(), 2)));
        assert_eq!(read("1A"), None);
        assert_eq!(read("!"), None);
        assert_eq!(read(""), None);
    }
}
