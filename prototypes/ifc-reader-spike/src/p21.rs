//! A minimal STEP Part 21 (ISO 10303-21) reader with no dependencies.
//!
//! Two passes, like the fast readers (ifc-lite, web-ifc):
//!
//! 1. [`File::parse`] scans the whole file once and records, for every
//!    instance, its id, its type name and the byte range of its record. Nothing
//!    is decoded. The index costs 16 bytes per instance.
//! 2. [`File::get`] decodes one record into [`Value`]s on demand. Strings are
//!    borrowed from the file and only unescaped when asked for.
//!
//! The reader is written for untrusted input: it never panics on malformed
//! bytes (every error is a [`Error`]), list nesting is bounded, and nothing is
//! allocated in proportion to a number read from the file.

use std::collections::HashMap;
use std::fmt;

/// Deepest list nesting accepted. IFC needs 3 (lists of lists of points).
pub const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    NotStep,
    Unterminated(&'static str, usize),
    Syntax(&'static str, usize),
    TooDeep(usize),
    NoSuchInstance(u32),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotStep => write!(f, "not a STEP Part 21 file"),
            Error::Unterminated(what, at) => write!(f, "unterminated {what} at byte {at}"),
            Error::Syntax(what, at) => write!(f, "{what} at byte {at}"),
            Error::TooDeep(at) => write!(f, "lists nested deeper than {MAX_DEPTH} at byte {at}"),
            Error::NoSuchInstance(id) => write!(f, "#{id} is not in the file"),
        }
    }
}

impl std::error::Error for Error {}

/// One attribute value, borrowing from the file.
#[derive(Debug, Clone, PartialEq)]
pub enum Value<'a> {
    /// `$`
    Null,
    /// `*`
    Derived,
    Int(i64),
    Real(f64),
    /// Raw string contents between the quotes, still escaped. See [`decode_string`].
    Str(&'a [u8]),
    /// `.NAME.` without the dots
    Enum(&'a [u8]),
    /// `#123`
    Ref(u32),
    /// `"0ABC"` hex digits, without the quotes
    Binary(&'a [u8]),
    List(Vec<Value<'a>>),
    /// `IFCLABEL('x')`: a typed parameter (select of a defined type)
    Typed(&'a [u8], Box<Value<'a>>),
}

impl<'a> Value<'a> {
    pub fn as_ref(&self) -> Option<u32> {
        match self {
            Value::Ref(r) => Some(*r),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Real(r) => Some(*r),
            Value::Int(i) => Some(*i as f64),
            Value::Typed(_, v) => v.as_f64(),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Typed(_, v) => v.as_i64(),
            _ => None,
        }
    }
    pub fn as_list(&self) -> Option<&[Value<'a>]> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }
    pub fn as_enum(&self) -> Option<&'a [u8]> {
        match self {
            Value::Enum(e) => Some(e),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<String> {
        match self {
            Value::Str(s) => Some(decode_string(s)),
            Value::Typed(_, v) => v.as_str(),
            _ => None,
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
}

/// A decoded instance: `#id=TYPE(attrs);`. Complex instances
/// (`#1=(A(...)B(...));`) keep their parts as `Typed` values in `attrs`, with
/// `ty` set to the first part's name.
#[derive(Debug, Clone)]
pub struct Instance<'a> {
    pub id: u32,
    pub ty: &'a [u8],
    pub attrs: Vec<Value<'a>>,
}

impl<'a> Instance<'a> {
    pub fn attr(&self, i: usize) -> &Value<'a> {
        self.attrs.get(i).unwrap_or(&Value::Null)
    }
}

#[derive(Clone, Copy)]
struct Slot {
    id: u32,
    ty: u16,
    start: u64,
    len: u32,
}

pub struct File<'a> {
    pub data: &'a [u8],
    /// FILE_DESCRIPTION, FILE_NAME, FILE_SCHEMA, ... as decoded instances (id 0).
    pub header: Vec<Instance<'a>>,
    slots: Vec<Slot>,
    types: Vec<&'a [u8]>,
    /// Instances whose id appeared twice. The first one wins.
    pub duplicates: usize,
}

impl<'a> File<'a> {
    /// Scan the file and index every instance. Decodes only the header.
    pub fn parse(data: &'a [u8]) -> Result<File<'a>, Error> {
        let mut p = 0usize;
        skip_ws(data, &mut p);
        if !data[p..].starts_with(b"ISO-10303-21") {
            return Err(Error::NotStep);
        }
        let header_at = find_keyword(data, b"HEADER;", p).ok_or(Error::NotStep)?;
        let data_at = find_keyword(data, b"DATA;", header_at)
            .ok_or(Error::Syntax("no DATA section", header_at))?;

        // Header: records of the form NAME(...);
        let mut header = Vec::new();
        let mut q = header_at + 7;
        loop {
            skip_ws(data, &mut q);
            if q >= data_at || data[q..].starts_with(b"ENDSEC") {
                break;
            }
            let (name, attrs, end) = parse_record_body(data, q)?;
            header.push(Instance {
                id: 0,
                ty: name,
                attrs,
            });
            q = end;
        }

        let mut file = File {
            data,
            header,
            slots: Vec::new(),
            types: Vec::new(),
            duplicates: 0,
        };
        let mut type_ids: HashMap<&'a [u8], u16> = HashMap::new();
        let mut p = data_at + 5;
        let mut sorted = true;
        let mut last = 0u32;
        loop {
            skip_ws(data, &mut p);
            if p >= data.len() {
                break;
            }
            match data[p] {
                b'#' => {}
                b'E' if data[p..].starts_with(b"ENDSEC") => break,
                _ => return Err(Error::Syntax("expected #id", p)),
            }
            let start = p;
            p += 1;
            let id = parse_uint(data, &mut p).ok_or(Error::Syntax("bad instance id", start))?;
            skip_ws(data, &mut p);
            if data.get(p) != Some(&b'=') {
                return Err(Error::Syntax("expected =", p));
            }
            p += 1;
            skip_ws(data, &mut p);
            let ty_start = p;
            while p < data.len() && (data[p].is_ascii_alphanumeric() || data[p] == b'_') {
                p += 1;
            }
            let ty = &data[ty_start..p];
            let end = skip_record(data, p)?;
            let tid = match type_ids.get(ty) {
                Some(t) => *t,
                None => {
                    let t = u16::try_from(file.types.len())
                        .map_err(|_| Error::Syntax("too many entity types", ty_start))?;
                    file.types.push(ty);
                    type_ids.insert(ty, t);
                    t
                }
            };
            let len = u32::try_from(end - start)
                .map_err(|_| Error::Syntax("record longer than 4 GB", start))?;
            if id <= last {
                sorted = false;
            }
            last = id;
            file.slots.push(Slot {
                id,
                ty: tid,
                start: start as u64,
                len,
            });
            p = end;
        }
        if !sorted {
            file.slots.sort_by_key(|s| s.id);
            let before = file.slots.len();
            file.slots.dedup_by_key(|s| s.id);
            file.duplicates = before - file.slots.len();
        }
        Ok(file)
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn schema(&self) -> Option<String> {
        let h = self
            .header
            .iter()
            .find(|h| h.ty.eq_ignore_ascii_case(b"FILE_SCHEMA"))?;
        h.attr(0).as_list()?.first()?.as_str()
    }

    /// FILE_NAME's originating system (attribute 5).
    pub fn originating_system(&self) -> Option<String> {
        let h = self
            .header
            .iter()
            .find(|h| h.ty.eq_ignore_ascii_case(b"FILE_NAME"))?;
        h.attr(5).as_str()
    }

    fn slot(&self, id: u32) -> Option<&Slot> {
        self.slots
            .binary_search_by_key(&id, |s| s.id)
            .ok()
            .map(|i| &self.slots[i])
    }

    /// The type name of instance `id`, upper case as written.
    pub fn type_of(&self, id: u32) -> Option<&'a [u8]> {
        self.slot(id).map(|s| self.types[s.ty as usize])
    }

    /// Every instance id with its type name, in id order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &'a [u8])> + '_ {
        self.slots.iter().map(|s| (s.id, self.types[s.ty as usize]))
    }

    /// Decode instance `id`.
    pub fn get(&self, id: u32) -> Result<Instance<'a>, Error> {
        let s = self.slot(id).ok_or(Error::NoSuchInstance(id))?;
        let start = s.start as usize;
        let mut p = start + 1;
        // skip id and '='
        while p < self.data.len() && self.data[p] != b'=' {
            p += 1;
        }
        p += 1;
        skip_ws(self.data, &mut p);
        if self.data.get(p) == Some(&b'(') {
            // complex instance: (A(...)B(...))
            p += 1;
            let mut parts = Vec::new();
            loop {
                skip_ws(self.data, &mut p);
                if self.data.get(p) == Some(&b')') {
                    break;
                }
                let (name, attrs, end) = parse_record_body(self.data, p)?;
                // parse_record_body expects a trailing ';' — complex parts don't have one
                parts.push(Value::Typed(name, Box::new(Value::List(attrs))));
                p = end;
            }
            let ty = match parts.first() {
                Some(Value::Typed(n, _)) => *n,
                _ => b"" as &[u8],
            };
            return Ok(Instance {
                id,
                ty,
                attrs: parts,
            });
        }
        let (ty, attrs, _) = parse_record_body(self.data, p)?;
        Ok(Instance { id, ty, attrs })
    }
}

fn find_keyword(data: &[u8], kw: &[u8], from: usize) -> Option<usize> {
    let mut p = from;
    while p + kw.len() <= data.len() {
        skip_ws(data, &mut p);
        if p >= data.len() {
            return None;
        }
        if data[p..].starts_with(kw) {
            return Some(p);
        }
        // skip a token or a quoted string
        match data[p] {
            b'\'' => {
                p = skip_string(data, p).ok()?;
            }
            _ => p += 1,
        }
    }
    None
}

/// Skip whitespace and `/* comments */`.
#[inline]
fn skip_ws(data: &[u8], p: &mut usize) {
    while *p < data.len() {
        let c = data[*p];
        if c.is_ascii_whitespace() {
            *p += 1;
        } else if c == b'/' && data.get(*p + 1) == Some(&b'*') {
            match find2(data, *p + 2, b'*', b'/') {
                Some(e) => *p = e + 2,
                None => *p = data.len(),
            }
        } else {
            break;
        }
    }
}

fn find2(data: &[u8], from: usize, a: u8, b: u8) -> Option<usize> {
    let mut i = from;
    while i + 1 < data.len() {
        if data[i] == a && data[i + 1] == b {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Returns the index just past the closing quote.
#[inline]
fn skip_string(data: &[u8], open: usize) -> Result<usize, Error> {
    let mut i = open + 1;
    loop {
        match data[i..].iter().position(|&c| c == b'\'') {
            None => return Err(Error::Unterminated("string", open)),
            Some(k) => {
                i += k;
                if data.get(i + 1) == Some(&b'\'') {
                    i += 2; // '' escape
                } else {
                    return Ok(i + 1);
                }
            }
        }
    }
}

/// From just after the type name, find the end of the record (past `;`).
fn skip_record(data: &[u8], mut p: usize) -> Result<usize, Error> {
    let start = p;
    while p < data.len() {
        match data[p] {
            b'\'' => p = skip_string(data, p)?,
            b'"' => match data[p + 1..].iter().position(|&c| c == b'"') {
                Some(k) => p += k + 2,
                None => return Err(Error::Unterminated("binary", p)),
            },
            b'/' if data.get(p + 1) == Some(&b'*') => skip_ws(data, &mut p),
            b';' => return Ok(p + 1),
            _ => p += 1,
        }
    }
    Err(Error::Unterminated("record", start))
}

fn parse_uint(data: &[u8], p: &mut usize) -> Option<u32> {
    let start = *p;
    let mut v: u32 = 0;
    while *p < data.len() && data[*p].is_ascii_digit() {
        v = v.checked_mul(10)?.checked_add((data[*p] - b'0') as u32)?;
        *p += 1;
    }
    (*p > start).then_some(v)
}

/// Parse `NAME(attr, ...)` starting at the name; returns the end just past `)`
/// and an optional `;`.
fn parse_record_body<'a>(
    data: &'a [u8],
    mut p: usize,
) -> Result<(&'a [u8], Vec<Value<'a>>, usize), Error> {
    let name_start = p;
    while p < data.len() && (data[p].is_ascii_alphanumeric() || data[p] == b'_') {
        p += 1;
    }
    if p == name_start {
        return Err(Error::Syntax("expected a type name", p));
    }
    let name = &data[name_start..p];
    skip_ws(data, &mut p);
    if data.get(p) != Some(&b'(') {
        return Err(Error::Syntax("expected (", p));
    }
    let attrs = match parse_value(data, &mut p, 0)? {
        Value::List(l) => l,
        _ => unreachable!("parse_value on '(' returns a list"),
    };
    skip_ws(data, &mut p);
    if data.get(p) == Some(&b';') {
        p += 1;
    }
    Ok((name, attrs, p))
}

fn parse_value<'a>(data: &'a [u8], p: &mut usize, depth: usize) -> Result<Value<'a>, Error> {
    skip_ws(data, p);
    let at = *p;
    let c = *data.get(at).ok_or(Error::Unterminated("value", at))?;
    match c {
        b'(' => {
            if depth >= MAX_DEPTH {
                return Err(Error::TooDeep(at));
            }
            *p += 1;
            let mut items = Vec::new();
            skip_ws(data, p);
            if data.get(*p) == Some(&b')') {
                *p += 1;
                return Ok(Value::List(items));
            }
            loop {
                items.push(parse_value(data, p, depth + 1)?);
                skip_ws(data, p);
                match data.get(*p) {
                    Some(b',') => *p += 1,
                    Some(b')') => {
                        *p += 1;
                        return Ok(Value::List(items));
                    }
                    Some(_) => return Err(Error::Syntax("expected , or )", *p)),
                    None => return Err(Error::Unterminated("list", at)),
                }
            }
        }
        b'$' => {
            *p += 1;
            Ok(Value::Null)
        }
        b'*' => {
            *p += 1;
            Ok(Value::Derived)
        }
        b'#' => {
            *p += 1;
            let id = parse_uint(data, p).ok_or(Error::Syntax("bad reference", at))?;
            Ok(Value::Ref(id))
        }
        b'\'' => {
            let end = skip_string(data, at)?;
            *p = end;
            Ok(Value::Str(&data[at + 1..end - 1]))
        }
        b'"' => {
            let k = data[at + 1..]
                .iter()
                .position(|&c| c == b'"')
                .ok_or(Error::Unterminated("binary", at))?;
            *p = at + k + 2;
            Ok(Value::Binary(&data[at + 1..at + 1 + k]))
        }
        b'.' => {
            let k = data[at + 1..]
                .iter()
                .position(|&c| c == b'.')
                .ok_or(Error::Unterminated("enumeration", at))?;
            *p = at + k + 2;
            Ok(Value::Enum(&data[at + 1..at + 1 + k]))
        }
        b'-' | b'+' | b'0'..=b'9' => parse_number(data, p),
        c if c.is_ascii_alphabetic() => {
            let start = *p;
            while *p < data.len() && (data[*p].is_ascii_alphanumeric() || data[*p] == b'_') {
                *p += 1;
            }
            let name = &data[start..*p];
            skip_ws(data, p);
            if data.get(*p) != Some(&b'(') {
                return Err(Error::Syntax("expected ( after a type name", *p));
            }
            if depth >= MAX_DEPTH {
                return Err(Error::TooDeep(at));
            }
            *p += 1;
            let inner = parse_value(data, p, depth + 1)?;
            skip_ws(data, p);
            if data.get(*p) != Some(&b')') {
                return Err(Error::Syntax("expected ) after a typed value", *p));
            }
            *p += 1;
            Ok(Value::Typed(name, Box::new(inner)))
        }
        _ => Err(Error::Syntax("unexpected character", at)),
    }
}

fn parse_number<'a>(data: &'a [u8], p: &mut usize) -> Result<Value<'a>, Error> {
    let start = *p;
    let mut i = start;
    if matches!(data.get(i), Some(b'-') | Some(b'+')) {
        i += 1;
    }
    let mut real = false;
    while i < data.len() {
        match data[i] {
            b'0'..=b'9' => i += 1,
            b'.' => {
                real = true;
                i += 1
            }
            b'E' | b'e' => {
                real = true;
                i += 1;
                if matches!(data.get(i), Some(b'-') | Some(b'+')) {
                    i += 1;
                }
            }
            _ => break,
        }
    }
    *p = i;
    // ASCII by construction
    let s = std::str::from_utf8(&data[start..i]).map_err(|_| Error::Syntax("bad number", start))?;
    if !real {
        if let Ok(v) = s.parse::<i64>() {
            return Ok(Value::Int(v));
        }
    }
    // Part 21 allows "1." and "1.E3"; Rust's parser accepts both.
    s.parse::<f64>()
        .map(Value::Real)
        .map_err(|_| Error::Syntax("bad number", start))
}

/// Decode a Part 21 string: `''`, `\\`, `\S\`, `\X\hh`, `\X2\...\X0\`, `\X4\...\X0\`.
/// Malformed escapes are kept as written.
pub fn decode_string(raw: &[u8]) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    let hex =
        |b: &[u8]| -> Option<u32> { u32::from_str_radix(std::str::from_utf8(b).ok()?, 16).ok() };
    while i < raw.len() {
        let c = raw[i];
        if c == b'\'' && raw.get(i + 1) == Some(&b'\'') {
            out.push('\'');
            i += 2;
        } else if c == b'\\' {
            let rest = &raw[i..];
            if rest.starts_with(b"\\\\") {
                out.push('\\');
                i += 2;
            } else if rest.starts_with(b"\\X2\\") || rest.starts_with(b"\\X4\\") {
                let w = if rest[2] == b'2' { 4 } else { 8 };
                let mut j = i + 4;
                let mut ok = false;
                while j + w <= raw.len() {
                    if raw[j..].starts_with(b"\\X0\\") {
                        ok = true;
                        break;
                    }
                    match hex(&raw[j..j + w]).and_then(char::from_u32) {
                        Some(ch) => out.push(ch),
                        None => out.push(char::REPLACEMENT_CHARACTER),
                    }
                    j += w;
                }
                if ok {
                    i = j + 4;
                } else {
                    out.push_str(&String::from_utf8_lossy(&raw[i..]));
                    break;
                }
            } else if rest.starts_with(b"\\X\\") && rest.len() >= 5 {
                match hex(&rest[3..5]).and_then(char::from_u32) {
                    Some(ch) => out.push(ch),
                    None => out.push(char::REPLACEMENT_CHARACTER),
                }
                i += 5;
            } else if rest.starts_with(b"\\S\\") && rest.len() >= 4 {
                out.push(char::from_u32(rest[3] as u32 + 128).unwrap_or('?'));
                i += 4;
            } else if rest.starts_with(b"\\P") && rest.len() >= 4 {
                i += 4; // code page switch: ignored
            } else {
                out.push('\\');
                i += 1;
            }
        } else if c < 0x80 {
            out.push(c as char);
            i += 1;
        } else {
            // Not allowed by the standard, but some exporters write UTF-8 directly.
            let end = (i + 1..raw.len())
                .find(|&k| raw[k] < 0x80 || raw[k] >= 0xC0)
                .unwrap_or(raw.len());
            out.push_str(&String::from_utf8_lossy(&raw[i..end]));
            i = end;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = b"ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('ViewDefinition [ReferenceView_V1.2]'),'2;1');
FILE_NAME('a.ifc','2026-10-15T00:00:00',(''),(''),'pre','Autodesk Revit 2025 (ENU)','');
FILE_SCHEMA(('IFC4'));
ENDSEC;
DATA;
#1=IFCCARTESIANPOINT((0.,1.5,-2.E-3));
/* comment ; with semicolon */
#3=IFCWALL('2O2Fr$t4X7Zf8NOew3FLOH',$,'Hormig\\X2\\00F3\\X0\\n ''A''; x',$,$,#2,*,.T.,IFCLABEL('x'));
#2=IFCLOCALPLACEMENT($,#1);
#4=(IFCA(1)IFCB(#1,'s'));
ENDSEC;
END-ISO-10303-21;
";

    #[test]
    fn reads_sample() {
        let f = File::parse(SAMPLE).unwrap();
        assert_eq!(f.len(), 4);
        assert_eq!(f.schema().as_deref(), Some("IFC4"));
        assert_eq!(
            f.originating_system().as_deref(),
            Some("Autodesk Revit 2025 (ENU)")
        );
        let p = f.get(1).unwrap();
        assert_eq!(p.ty, b"IFCCARTESIANPOINT");
        let l = p.attr(0).as_list().unwrap();
        assert_eq!(l[2].as_f64(), Some(-0.002));
        let w = f.get(3).unwrap();
        assert_eq!(w.attr(2).as_str().unwrap(), "Hormigón 'A'; x");
        assert_eq!(w.attr(5).as_ref(), Some(2));
        assert_eq!(w.attr(6), &Value::Derived);
        assert_eq!(w.attr(7).as_enum(), Some(&b"T"[..]));
        assert!(matches!(w.attr(8), Value::Typed(n, _) if *n == b"IFCLABEL"));
        assert_eq!(f.type_of(2), Some(&b"IFCLOCALPLACEMENT"[..]));
        let c = f.get(4).unwrap();
        assert_eq!(c.ty, b"IFCA");
        assert_eq!(c.attrs.len(), 2);
    }

    #[test]
    fn rejects_garbage_without_panicking() {
        for s in [&b""[..], b"ISO-10303-21;", b"ISO-10303-21;HEADER;DATA;#1=X('", b"ISO-10303-21;HEADER;ENDSEC;DATA;#1=X(((((((((((((((((((((((((((((((((((((1)))))))))))))))))))))))))))))))))))));"] {
            if let Ok(f) = File::parse(s) {
                for (id, _) in f.iter() {
                    let _ = f.get(id);
                }
            }
        }
    }
}
