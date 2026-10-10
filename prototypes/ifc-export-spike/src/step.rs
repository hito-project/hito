//! A minimal ISO 10303-21 (STEP physical file) writer.
//!
//! It knows the encoding, not the schema: entity names and attribute order
//! come from the caller (`ifc.rs`). Schema mistakes are caught by validation
//! (IfcOpenShell), not by this module.

use std::fmt::Write as _;

/// An entity instance name, `#123`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Id(pub u64);

/// One attribute value.
#[derive(Clone, Debug)]
pub enum V {
    /// `$`: an unset optional attribute.
    Null,
    /// `*`: an attribute redeclared as derived in a subtype.
    Derived,
    Int(i64),
    Real(f64),
    Str(String),
    /// `.NAME.`
    Enum(&'static str),
    /// `.T.` / `.F.`
    Bool(bool),
    Ref(Id),
    List(Vec<V>),
    /// A typed value inside a SELECT, e.g. `IFCLABEL('x')`.
    Typed(&'static str, Box<V>),
}

impl From<f64> for V {
    fn from(x: f64) -> Self {
        V::Real(x)
    }
}
impl From<i64> for V {
    fn from(x: i64) -> Self {
        V::Int(x)
    }
}
impl From<&str> for V {
    fn from(s: &str) -> Self {
        V::Str(s.to_owned())
    }
}
impl From<String> for V {
    fn from(s: String) -> Self {
        V::Str(s)
    }
}
impl From<Id> for V {
    fn from(id: Id) -> Self {
        V::Ref(id)
    }
}
impl From<bool> for V {
    fn from(b: bool) -> Self {
        V::Bool(b)
    }
}
impl<T: Into<V>> From<Option<T>> for V {
    fn from(o: Option<T>) -> Self {
        o.map_or(V::Null, Into::into)
    }
}
impl<T: Into<V>> From<Vec<T>> for V {
    fn from(v: Vec<T>) -> Self {
        V::List(v.into_iter().map(Into::into).collect())
    }
}

/// Build an attribute list: `args![a, b, V::Null]`.
#[macro_export]
macro_rules! args {
    ($($x:expr),* $(,)?) => { vec![$($crate::step::V::from($x)),*] };
}

/// The header fields of a Part 21 file.
pub struct Header<'a> {
    pub description: &'a str,
    pub file_name: &'a str,
    pub timestamp: &'a str,
    pub author: &'a str,
    pub organization: &'a str,
    pub preprocessor: &'a str,
    pub originating_system: &'a str,
    pub schema: &'a str,
}

/// Writes entities in creation order, numbering them from #1.
pub struct StepWriter {
    data: String,
    next: u64,
}

impl Default for StepWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl StepWriter {
    pub fn new() -> Self {
        Self { data: String::new(), next: 1 }
    }

    /// Append `#n=NAME(args);` and return `#n`.
    pub fn add(&mut self, name: &str, args: Vec<V>) -> Id {
        let id = Id(self.next);
        self.next += 1;
        write!(self.data, "#{}={}(", id.0, name).unwrap();
        write_list(&mut self.data, &args);
        self.data.push_str(");\n");
        id
    }

    pub fn entity_count(&self) -> u64 {
        self.next - 1
    }

    /// The complete file: header and data sections.
    pub fn finish(self, h: &Header) -> String {
        let mut out = String::with_capacity(self.data.len() + 512);
        out.push_str("ISO-10303-21;\nHEADER;\n");
        writeln!(out, "FILE_DESCRIPTION(('{}'),'2;1');", encode_str(h.description)).unwrap();
        writeln!(
            out,
            "FILE_NAME('{}','{}',('{}'),('{}'),'{}','{}','');",
            encode_str(h.file_name),
            h.timestamp,
            encode_str(h.author),
            encode_str(h.organization),
            encode_str(h.preprocessor),
            encode_str(h.originating_system),
        )
        .unwrap();
        writeln!(out, "FILE_SCHEMA(('{}'));", h.schema).unwrap();
        out.push_str("ENDSEC;\nDATA;\n");
        out.push_str(&self.data);
        out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
        out
    }
}

fn write_list(out: &mut String, items: &[V]) {
    for (i, v) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_value(out, v);
    }
}

fn write_value(out: &mut String, v: &V) {
    match v {
        V::Null => out.push('$'),
        V::Derived => out.push('*'),
        V::Int(i) => write!(out, "{i}").unwrap(),
        V::Real(x) => out.push_str(&format_real(*x)),
        V::Str(s) => {
            out.push('\'');
            out.push_str(&encode_str(s));
            out.push('\'');
        }
        V::Enum(e) => write!(out, ".{e}.").unwrap(),
        V::Bool(b) => out.push_str(if *b { ".T." } else { ".F." }),
        V::Ref(id) => write!(out, "#{}", id.0).unwrap(),
        V::List(items) => {
            out.push('(');
            write_list(out, items);
            out.push(')');
        }
        V::Typed(name, inner) => {
            write!(out, "{name}(").unwrap();
            write_value(out, inner);
            out.push(')');
        }
    }
}

/// Part 21 REAL: always has a decimal point, exponent written `E`.
/// Uses Rust's shortest round-trip representation, so no precision is lost.
pub fn format_real(x: f64) -> String {
    assert!(x.is_finite(), "STEP has no NaN or infinity: {x}");
    let s = format!("{x:?}"); // e.g. "0.2", "3.0", "1e-5", "-1.5e20"
    let (mantissa, exp) = match s.split_once('e') {
        Some((m, e)) => (m, Some(e)),
        None => (s.as_str(), None),
    };
    let mut m = mantissa.to_owned();
    if !m.contains('.') {
        m.push('.');
    } else if m.ends_with(".0") {
        m.truncate(m.len() - 1); // "3.0" -> "3."
    }
    match exp {
        Some(e) => format!("{m}E{e}"),
        None => m,
    }
}

/// Part 21 string encoding: `'` doubled, `\` doubled, everything outside
/// printable ASCII as `\X2\` (UTF-16, BMP) or `\X4\` (UTF-32) hex runs.
pub fn encode_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if (' '..='~').contains(&c) {
            match c {
                '\'' => out.push_str("''"),
                '\\' => out.push_str("\\\\"),
                _ => out.push(c),
            }
            i += 1;
            continue;
        }
        // A run of non-ASCII characters of the same width.
        let wide = (c as u32) > 0xFFFF;
        let start = i;
        while i < chars.len() && !(' '..='~').contains(&chars[i]) && ((chars[i] as u32) > 0xFFFF) == wide {
            i += 1;
        }
        if wide {
            out.push_str("\\X4\\");
            for ch in &chars[start..i] {
                write!(out, "{:08X}", *ch as u32).unwrap();
            }
        } else {
            out.push_str("\\X2\\");
            for ch in &chars[start..i] {
                write!(out, "{:04X}", *ch as u32).unwrap();
            }
        }
        out.push_str("\\X0\\");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reals() {
        assert_eq!(format_real(3.0), "3.");
        assert_eq!(format_real(0.2), "0.2");
        assert_eq!(format_real(-0.0), "-0.");
        assert_eq!(format_real(1e-5), "1.E-5");
        assert_eq!(format_real(6_000_000.125), "6000000.125");
        assert_eq!(format_real(1.5e300), "1.5E300");
        // Shortest round-trip: the parsed value is bit-identical.
        for x in [0.1 + 0.2, std::f64::consts::PI, 1.0 / 3.0, 6_123_456.789_012_3] {
            let s = format_real(x).replace('E', "e");
            assert_eq!(s.parse::<f64>().unwrap().to_bits(), x.to_bits());
        }
    }

    #[test]
    fn strings() {
        assert_eq!(encode_str("Planta baja"), "Planta baja");
        assert_eq!(encode_str("O'Higgins"), "O''Higgins");
        assert_eq!(encode_str("a\\b"), "a\\\\b");
        assert_eq!(encode_str("Hormigón"), "Hormig\\X2\\00F3\\X0\\n");
        assert_eq!(encode_str("Ñandú"), "\\X2\\00D1\\X0\\and\\X2\\00FA\\X0\\");
        assert_eq!(encode_str("m²"), "m\\X2\\00B2\\X0\\");
        assert_eq!(encode_str("🏗"), "\\X4\\0001F3D7\\X0\\");
    }

    #[test]
    fn entity_line() {
        let mut w = StepWriter::new();
        let p = w.add("IFCCARTESIANPOINT", args![vec![0.0, 1.5, 3.0]]);
        let l = w.add("IFCLABEL", args![V::Null, V::Derived, p, V::Enum("T"), true, 5i64]);
        assert_eq!(p, Id(1));
        assert_eq!(l, Id(2));
        let out = w.finish(&Header {
            description: "x",
            file_name: "f",
            timestamp: "2026-10-08T00:00:00",
            author: "",
            organization: "",
            preprocessor: "",
            originating_system: "",
            schema: "IFC4",
        });
        assert!(out.contains("#1=IFCCARTESIANPOINT((0.,1.5,3.));\n"));
        assert!(out.contains("#2=IFCLABEL($,*,#1,.T.,.T.,5);\n"));
    }
}
