//! Streaming a Part 21 file: the header, then the data section entity by entity.

use std::fmt::{self, Write as _};
use std::io::{self, BufWriter, Write};

use crate::encode::{is_keyword, push_real, push_string};
use crate::value::{EntityId, Value};

/// The size of the output buffer. Entities are small, so this batches a few
/// hundred of them into each write to the underlying writer.
const BUFFER_SIZE: usize = 64 * 1024;

/// What can go wrong while writing.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The underlying writer failed.
    Io(io::Error),
    /// A REAL was NaN or infinite, which Part 21 can't write.
    NonFiniteReal(f64),
    /// An entity, enumeration or type name isn't a keyword
    /// ([`is_keyword`](crate::is_keyword)).
    InvalidKeyword(String),
    /// The header names no schema; `FILE_SCHEMA` needs at least one.
    MissingSchema,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "writing the STEP file failed: {e}"),
            Error::NonFiniteReal(x) => write!(f, "a STEP REAL must be finite, got {x}"),
            Error::InvalidKeyword(name) => write!(
                f,
                "`{name}` isn't a STEP keyword (uppercase letters, digits and `_`, \
                 not starting with a digit)"
            ),
            Error::MissingSchema => write!(f, "the STEP header names no schema"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

/// The header section of a Part 21 file.
///
/// Every field is written as given, with strings escaped. Empty lists are
/// written as a list of one empty string, because Part 21 requires at least
/// one item, except `schema_identifiers`, which must name a schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header<'a> {
    /// `FILE_DESCRIPTION.description`, such as
    /// `["ViewDefinition [ReferenceView_V1.2]"]` for IFC.
    pub description: &'a [&'a str],
    /// `FILE_DESCRIPTION.implementation_level`. `"2;1"` (the default) is
    /// Part 21 edition 2, conformance class 1.
    pub implementation_level: &'a str,
    /// `FILE_NAME.name`.
    pub file_name: &'a str,
    /// `FILE_NAME.time_stamp`, an ISO 8601 date and time such as
    /// `2026-10-15T12:00:00`. The caller supplies it, so output can be
    /// reproducible.
    pub time_stamp: &'a str,
    /// `FILE_NAME.author`.
    pub author: &'a [&'a str],
    /// `FILE_NAME.organization`.
    pub organization: &'a [&'a str],
    /// `FILE_NAME.preprocessor_version`: the program and version writing the file.
    pub preprocessor_version: &'a str,
    /// `FILE_NAME.originating_system`: the program the data comes from.
    pub originating_system: &'a str,
    /// `FILE_NAME.authorization`.
    pub authorization: &'a str,
    /// `FILE_SCHEMA.schema_identifiers`, such as `["IFC4"]`.
    pub schema_identifiers: &'a [&'a str],
}

impl Default for Header<'_> {
    fn default() -> Self {
        Header {
            description: &[],
            implementation_level: "2;1",
            file_name: "",
            time_stamp: "",
            author: &[],
            organization: &[],
            preprocessor_version: "",
            originating_system: "",
            authorization: "",
            schema_identifiers: &[],
        }
    }
}

impl Header<'_> {
    fn encode(&self) -> Result<String, Error> {
        if self.schema_identifiers.is_empty() {
            return Err(Error::MissingSchema);
        }
        let mut out = String::from("ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(");
        push_string_list(&mut out, self.description);
        out.push(',');
        push_quoted(&mut out, self.implementation_level);
        out.push_str(");\nFILE_NAME(");
        push_quoted(&mut out, self.file_name);
        out.push(',');
        push_quoted(&mut out, self.time_stamp);
        out.push(',');
        push_string_list(&mut out, self.author);
        out.push(',');
        push_string_list(&mut out, self.organization);
        for s in [
            self.preprocessor_version,
            self.originating_system,
            self.authorization,
        ] {
            out.push(',');
            push_quoted(&mut out, s);
        }
        out.push_str(");\nFILE_SCHEMA(");
        push_string_list(&mut out, self.schema_identifiers);
        out.push_str(");\nENDSEC;\nDATA;\n");
        Ok(out)
    }
}

/// Writes a Part 21 file to `W` as it goes, numbering entities from `#1`.
///
/// [`new`](Self::new) writes the header, each [`add`](Self::add) one entity
/// line, and [`finish`](Self::finish) the end of the file. Output goes
/// through a buffer, so a file or socket can be passed directly; nothing
/// holds the whole file in memory. A writer dropped without `finish` leaves
/// an incomplete file.
#[must_use = "call `finish` to end the file"]
pub struct StepWriter<W: Write> {
    out: BufWriter<W>,
    /// The entity line being encoded, reused so each line doesn't allocate.
    line: String,
    count: u64,
}

impl<W: Write> fmt::Debug for StepWriter<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StepWriter")
            .field("entity_count", &self.count)
            .finish_non_exhaustive()
    }
}

impl<W: Write> StepWriter<W> {
    /// Starts a file: writes the header and opens the data section.
    pub fn new(out: W, header: &Header<'_>) -> Result<Self, Error> {
        let mut out = BufWriter::with_capacity(BUFFER_SIZE, out);
        out.write_all(header.encode()?.as_bytes())?;
        Ok(StepWriter {
            out,
            line: String::new(),
            count: 0,
        })
    }

    /// Writes the entity instance `#n=NAME(attributes);` and returns `#n`.
    ///
    /// The name and attributes are checked before anything is written: if
    /// they hold a non-finite real or a name that isn't a keyword, nothing is
    /// written and the number isn't used.
    pub fn add(&mut self, name: &str, attributes: &[Value]) -> Result<EntityId, Error> {
        check_keyword(name)?;
        let id = EntityId::new(self.count + 1);
        self.line.clear();
        write!(self.line, "{id}={name}(").expect("writing to a String can't fail");
        push_list(&mut self.line, attributes)?;
        self.line.push_str(");\n");
        self.out.write_all(self.line.as_bytes())?;
        self.count += 1;
        Ok(id)
    }

    /// How many entities have been written.
    pub fn entity_count(&self) -> u64 {
        self.count
    }

    /// Closes the data section and the file, flushes, and returns the writer.
    pub fn finish(mut self) -> Result<W, Error> {
        self.out.write_all(b"ENDSEC;\nEND-ISO-10303-21;\n")?;
        self.out.into_inner().map_err(|e| Error::Io(e.into_error()))
    }
}

fn check_keyword(name: &str) -> Result<(), Error> {
    if is_keyword(name) {
        Ok(())
    } else {
        Err(Error::InvalidKeyword(name.to_owned()))
    }
}

fn push_list(out: &mut String, items: &[Value]) -> Result<(), Error> {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_value(out, item)?;
    }
    Ok(())
}

fn push_value(out: &mut String, value: &Value) -> Result<(), Error> {
    match value {
        Value::Null => out.push('$'),
        Value::Derived => out.push('*'),
        Value::Integer(n) => write!(out, "{n}").expect("writing to a String can't fail"),
        Value::Real(x) => {
            if !push_real(out, *x) {
                return Err(Error::NonFiniteReal(*x));
            }
        }
        Value::String(s) => push_quoted(out, s),
        Value::Enumeration(name) => {
            check_keyword(name)?;
            out.push('.');
            out.push_str(name);
            out.push('.');
        }
        Value::Boolean(b) => out.push_str(if *b { ".T." } else { ".F." }),
        Value::Reference(id) => write!(out, "{id}").expect("writing to a String can't fail"),
        Value::List(items) => {
            out.push('(');
            push_list(out, items)?;
            out.push(')');
        }
        Value::Typed(name, inner) => {
            check_keyword(name)?;
            out.push_str(name);
            out.push('(');
            push_value(out, inner)?;
            out.push(')');
        }
    }
    Ok(())
}

fn push_quoted(out: &mut String, s: &str) {
    out.push('\'');
    push_string(out, s);
    out.push('\'');
}

/// `('a','b')`, or `('')` for an empty list.
fn push_string_list(out: &mut String, items: &[&str]) {
    out.push('(');
    if items.is_empty() {
        out.push_str("''");
    }
    for (i, s) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_quoted(out, s);
    }
    out.push(')');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args;

    fn header() -> Header<'static> {
        Header {
            description: &["x"],
            file_name: "f",
            time_stamp: "2026-10-08T00:00:00",
            schema_identifiers: &["IFC4"],
            ..Header::default()
        }
    }

    // Carried over from spike #3 (tag spike/3-ifc-export, src/step.rs).
    #[test]
    fn entity_line() {
        let mut w = StepWriter::new(Vec::new(), &header()).unwrap();
        let p = w
            .add("IFCCARTESIANPOINT", &args![vec![0.0, 1.5, 3.0]])
            .unwrap();
        let l = w
            .add(
                "IFCLABEL",
                &args![
                    Value::Null,
                    Value::Derived,
                    p,
                    Value::enumeration("T"),
                    true,
                    5i64
                ],
            )
            .unwrap();
        assert_eq!(p, EntityId::new(1));
        assert_eq!(l, EntityId::new(2));
        let out = String::from_utf8(w.finish().unwrap()).unwrap();
        assert!(out.contains("#1=IFCCARTESIANPOINT((0.,1.5,3.));\n"));
        assert!(out.contains("#2=IFCLABEL($,*,#1,.T.,.T.,5);\n"));
    }
}
