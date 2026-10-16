//! Reading a Part 21 file: one pass to index it, then entities decoded on demand.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

use crate::decode::{
    Fault, Parse, expect, fault, instance_name, keyword, list, skip_binary, skip_comment,
    skip_space, skip_string,
};
use crate::value::{EntityId, Value};

/// What is wrong with a file, or with a lookup in it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReadErrorKind {
    /// The text doesn't follow the Part 21 grammar. The message says what
    /// was expected or what is malformed, such as "expected `;`".
    Syntax(&'static str),
    /// A string, comment, record or binary value runs to the end of the file.
    Unterminated(&'static str),
    /// Lists and typed values nest deeper than [`MAX_DEPTH`](crate::MAX_DEPTH).
    TooDeep,
    /// An INTEGER or instance name that doesn't fit in 64 bits, or a REAL
    /// too large for an `f64`.
    NumberOutOfRange,
    /// The instance name is used twice. The error's line is the second use.
    Duplicate {
        /// The line of the first use.
        first_line: usize,
    },
    /// Valid Part 21 that this reader doesn't decode, such as binary values.
    Unsupported(&'static str),
    /// [`StepReader::get`] was asked for an instance the file doesn't have,
    /// such as the target of a dangling reference.
    NotFound,
}

impl fmt::Display for ReadErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadErrorKind::Syntax(what) => f.write_str(what),
            ReadErrorKind::Unterminated(what) => write!(f, "unterminated {what}"),
            ReadErrorKind::TooDeep => write!(
                f,
                "lists and typed values nested deeper than {}",
                crate::MAX_DEPTH
            ),
            ReadErrorKind::NumberOutOfRange => f.write_str("a number out of range"),
            ReadErrorKind::Duplicate { first_line } => {
                write!(f, "the instance name was already used on line {first_line}")
            }
            ReadErrorKind::Unsupported(what) => write!(f, "{what} aren't supported"),
            ReadErrorKind::NotFound => f.write_str("no such instance in the file"),
        }
    }
}

/// An error reading a file: what is wrong, on which line, in which entity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadError {
    kind: ReadErrorKind,
    line: Option<usize>,
    entity: Option<EntityId>,
}

impl ReadError {
    fn new(data: &[u8], fault: Fault, entity: Option<EntityId>) -> Self {
        ReadError {
            kind: fault.kind,
            line: Some(line_of(data, fault.at)),
            entity,
        }
    }

    /// What is wrong.
    pub fn kind(&self) -> &ReadErrorKind {
        &self.kind
    }

    /// The line, counted from 1, where the problem is. `None` only for
    /// [`ReadErrorKind::NotFound`], which is about a lookup, not the text.
    pub fn line(&self) -> Option<usize> {
        self.line
    }

    /// The entity instance being read, if the problem is inside one.
    pub fn entity(&self) -> Option<EntityId> {
        self.entity
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.entity) {
            (Some(line), Some(id)) => write!(f, "line {line}, {id}: {}", self.kind),
            (Some(line), None) => write!(f, "line {line}: {}", self.kind),
            (None, Some(id)) => write!(f, "{id}: {}", self.kind),
            (None, None) => write!(f, "{}", self.kind),
        }
    }
}

impl std::error::Error for ReadError {}

/// The header section as read from a file: the owned counterpart of
/// [`Header`](crate::Header), field for field.
///
/// A header record that is missing, or holds something other than strings
/// where strings belong, reads as empty, so a sloppy header doesn't stop
/// the data from being read. A list holding one empty string, `('')`, reads
/// as an empty list, as the writer writes an empty list that way.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileHeader {
    /// `FILE_DESCRIPTION.description`.
    pub description: Vec<String>,
    /// `FILE_DESCRIPTION.implementation_level`, such as `2;1`.
    pub implementation_level: String,
    /// `FILE_NAME.name`.
    pub file_name: String,
    /// `FILE_NAME.time_stamp`.
    pub time_stamp: String,
    /// `FILE_NAME.author`.
    pub author: Vec<String>,
    /// `FILE_NAME.organization`.
    pub organization: Vec<String>,
    /// `FILE_NAME.preprocessor_version`.
    pub preprocessor_version: String,
    /// `FILE_NAME.originating_system`, such as `Autodesk Revit 2025 (ENU)`.
    pub originating_system: String,
    /// `FILE_NAME.authorization`.
    pub authorization: String,
    /// `FILE_SCHEMA.schema_identifiers`, such as `["IFC4"]`.
    pub schema_identifiers: Vec<String>,
}

impl FileHeader {
    /// Takes the fields of one header record. Other records are ignored.
    fn set(&mut self, name: &str, attributes: Vec<Value>) {
        let mut a = attributes.into_iter();
        match name {
            "FILE_DESCRIPTION" => {
                self.description = text_list(a.next());
                self.implementation_level = text(a.next());
            }
            "FILE_NAME" => {
                self.file_name = text(a.next());
                self.time_stamp = text(a.next());
                self.author = text_list(a.next());
                self.organization = text_list(a.next());
                self.preprocessor_version = text(a.next());
                self.originating_system = text(a.next());
                self.authorization = text(a.next());
            }
            "FILE_SCHEMA" => self.schema_identifiers = strings(a.next()),
            _ => {}
        }
    }
}

fn text(value: Option<Value>) -> String {
    match value {
        Some(Value::String(s)) => s,
        _ => String::new(),
    }
}

fn strings(value: Option<Value>) -> Vec<String> {
    match value {
        Some(Value::List(items)) => items
            .into_iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// [`strings`], with `('')` read as an empty list.
fn text_list(value: Option<Value>) -> Vec<String> {
    let list = strings(value);
    if list == [""] { Vec::new() } else { list }
}

/// One decoded entity instance, `#id=NAME(attributes);`.
#[derive(Clone, Debug, PartialEq)]
pub struct Entity<'a> {
    /// Its instance name.
    pub id: EntityId,
    /// The entity type name as written, such as `IFCWALL`.
    ///
    /// Empty for a complex instance, `#1=(A(…)B(…));`, whose attributes are
    /// then its parts in order, each a [`Value::Typed`] holding the part's
    /// name and its attribute list.
    pub name: &'a str,
    /// The attribute values, in order.
    pub attributes: Vec<Value>,
}

/// Where one entity instance is: its name and the offset of its `#`.
#[derive(Clone, Copy, Debug)]
struct Slot {
    id: EntityId,
    start: usize,
}

/// A Part 21 file, indexed and ready to decode entity by entity.
///
/// [`new`](Self::new) reads the header and scans the data sections once,
/// recording where every entity instance starts and grouping the instances
/// by type name; that costs 24 bytes per instance on top of the file.
/// [`get`](Self::get) decodes one instance's attributes when they're needed.
///
/// The reader borrows the file's bytes and is written for untrusted input:
/// malformed bytes give a [`ReadError`] with the line and entity, never a
/// panic; nesting is bounded by [`MAX_DEPTH`](crate::MAX_DEPTH); and nothing
/// is allocated in proportion to a number in the file.
///
/// What it reads back from the writer's output is what was written, with one
/// exception that Part 21 itself can't tell apart: the enumeration items `T`
/// and `F` read as [`Value::Boolean`].
pub struct StepReader<'a> {
    data: &'a [u8],
    header: FileHeader,
    /// Every instance, sorted by instance name.
    slots: Vec<Slot>,
    /// The entity type names, sorted, each with its instances in id order.
    types: Vec<(&'a str, Vec<EntityId>)>,
}

impl fmt::Debug for StepReader<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StepReader")
            .field("header", &self.header)
            .field("entity_count", &self.slots.len())
            .finish_non_exhaustive()
    }
}

/// Collects the slots and the type index while the data sections are scanned.
#[derive(Default)]
struct Index<'a> {
    slots: Vec<Slot>,
    types: Vec<(&'a str, Vec<EntityId>)>,
    type_index: HashMap<&'a str, usize>,
    /// The last type seen: files often hold runs of one type.
    last_type: Option<(&'a str, usize)>,
    sorted: bool,
}

impl<'a> Index<'a> {
    fn add(&mut self, id: EntityId, name: &'a str, start: usize) {
        self.sorted &= self.slots.last().is_none_or(|s| s.id < id);
        self.slots.push(Slot { id, start });
        let t = match self.last_type {
            Some((last, t)) if last == name => t,
            _ => {
                let types = &mut self.types;
                let t = *self.type_index.entry(name).or_insert_with(|| {
                    types.push((name, Vec::new()));
                    types.len() - 1
                });
                self.last_type = Some((name, t));
                t
            }
        };
        self.types[t].1.push(id);
    }
}

impl<'a> StepReader<'a> {
    /// Reads the header of `data` and indexes every entity instance in its
    /// data sections, without decoding their attributes.
    ///
    /// The whole structure is checked: the sections, every instance's name
    /// and `=`, balanced parentheses, terminated strings and comments, and
    /// the `;` after each instance. Values inside an instance are checked
    /// when it's decoded.
    pub fn new(data: &'a [u8]) -> Result<Self, ReadError> {
        let in_file = |f| ReadError::new(data, f, None);
        let mut p = 0;
        if !word(data, &mut p, b"ISO-10303-21").map_err(in_file)? {
            return Err(in_file(Fault {
                at: p,
                kind: ReadErrorKind::Syntax("expected ISO-10303-21;"),
            }));
        }
        expect(data, &mut p, b';', "expected ISO-10303-21;").map_err(in_file)?;
        let header = read_header(data, &mut p).map_err(in_file)?;

        let mut index = Index {
            sorted: true,
            ..Index::default()
        };
        loop {
            if word(data, &mut p, b"DATA").map_err(in_file)? {
                // Part 21 edition 3 names each data section: `DATA('a',('IFC4'));`.
                skip_space(data, &mut p).map_err(in_file)?;
                if data.get(p) == Some(&b'(') {
                    list(data, &mut p, 0).map_err(in_file)?;
                }
                expect(data, &mut p, b';', "expected `;`").map_err(in_file)?;
                read_data_section(data, &mut p, &mut index)?;
            } else if word(data, &mut p, b"END-ISO-10303-21").map_err(in_file)? {
                expect(data, &mut p, b';', "expected `;`").map_err(in_file)?;
                // Whatever follows the end of the file is ignored.
                break;
            } else {
                return Err(in_file(Fault {
                    at: p,
                    kind: ReadErrorKind::Syntax("expected DATA or END-ISO-10303-21"),
                }));
            }
        }

        let Index {
            mut slots,
            mut types,
            sorted,
            ..
        } = index;
        if !sorted {
            slots.sort_unstable_by_key(|s| (s.id, s.start));
            if let Some(w) = slots.windows(2).find(|w| w[0].id == w[1].id) {
                return Err(ReadError {
                    kind: ReadErrorKind::Duplicate {
                        first_line: line_of(data, w[0].start),
                    },
                    line: Some(line_of(data, w[1].start)),
                    entity: Some(w[1].id),
                });
            }
            for (_, ids) in &mut types {
                ids.sort_unstable();
            }
        }
        types.sort_unstable_by_key(|&(name, _)| name);
        Ok(StepReader {
            data,
            header,
            slots,
            types,
        })
    }

    /// The header section.
    pub fn header(&self) -> &FileHeader {
        &self.header
    }

    /// How many entity instances the file has.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether the file has no entity instances.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Every instance name, in increasing order.
    pub fn ids(&self) -> impl ExactSizeIterator<Item = EntityId> + '_ {
        self.slots.iter().map(|s| s.id)
    }

    /// The type name of instance `id` as written, such as `IFCWALL`, or
    /// `None` if the file has no such instance. Empty for a complex instance.
    pub fn type_name(&self, id: EntityId) -> Option<&'a str> {
        let mut p = self.slot(id)?.start;
        instance_name(self.data, &mut p).ok()?;
        expect(self.data, &mut p, b'=', "").ok()?;
        skip_space(self.data, &mut p).ok()?;
        Some(keyword(self.data, &mut p).unwrap_or(""))
    }

    /// The instances of the type `name`, in increasing order. Names are
    /// compared as written, so IFC's are uppercase: `IFCWALL`.
    pub fn instances_of(&self, name: &str) -> &[EntityId] {
        match self.types.binary_search_by_key(&name, |&(n, _)| n) {
            Ok(i) => &self.types[i].1,
            Err(_) => &[],
        }
    }

    /// Every type name in the file, sorted.
    pub fn type_names(&self) -> impl ExactSizeIterator<Item = &'a str> + '_ {
        self.types.iter().map(|&(name, _)| name)
    }

    /// Decodes instance `id`.
    pub fn get(&self, id: EntityId) -> Result<Entity<'a>, ReadError> {
        let slot = self.slot(id).ok_or(ReadError {
            kind: ReadErrorKind::NotFound,
            line: None,
            entity: Some(id),
        })?;
        decode_entity(self.data, slot).map_err(|f| ReadError::new(self.data, f, Some(id)))
    }

    fn slot(&self, id: EntityId) -> Option<Slot> {
        // Most files number their instances 1, 2, 3, ... without gaps.
        let guess = usize::try_from(id.get() - 1).ok();
        if let Some(&slot) = guess.and_then(|i| self.slots.get(i))
            && slot.id == id
        {
            return Some(slot);
        }
        let i = self.slots.binary_search_by_key(&id, |s| s.id).ok()?;
        Some(self.slots[i])
    }
}

/// The line, counted from 1, of the byte at offset `at`.
fn line_of(data: &[u8], at: usize) -> usize {
    let before = data.get(..at).unwrap_or(data);
    1 + before.iter().filter(|&&b| b == b'\n').count()
}

/// Skips whitespace, then reads `w` if it's next.
fn word(data: &[u8], p: &mut usize, w: &[u8]) -> Parse<bool> {
    skip_space(data, p)?;
    let found = data.get(*p..).is_some_and(|rest| rest.starts_with(w));
    if found {
        *p += w.len();
    }
    Ok(found)
}

/// `HEADER; FILE_DESCRIPTION(...); FILE_NAME(...); FILE_SCHEMA(...); ENDSEC;`
fn read_header(data: &[u8], p: &mut usize) -> Parse<FileHeader> {
    if !word(data, p, b"HEADER")? {
        return fault(*p, ReadErrorKind::Syntax("expected HEADER;"));
    }
    expect(data, p, b';', "expected HEADER;")?;
    let mut header = FileHeader::default();
    loop {
        if word(data, p, b"ENDSEC")? {
            expect(data, p, b';', "expected `;`")?;
            return Ok(header);
        }
        let Some(name) = keyword(data, p) else {
            return fault(
                *p,
                ReadErrorKind::Syntax("expected a header entity or ENDSEC"),
            );
        };
        skip_space(data, p)?;
        let attributes = list(data, p, 0)?;
        expect(data, p, b';', "expected `;`")?;
        header.set(name, attributes);
    }
}

/// The instances of one data section, up to and including its `ENDSEC;`.
fn read_data_section<'a>(
    data: &'a [u8],
    p: &mut usize,
    index: &mut Index<'a>,
) -> Result<(), ReadError> {
    let in_file = |f| ReadError::new(data, f, None);
    loop {
        skip_space(data, p).map_err(in_file)?;
        if data.get(*p) != Some(&b'#') {
            if word(data, p, b"ENDSEC").map_err(in_file)? {
                return expect(data, p, b';', "expected `;`").map_err(in_file);
            }
            return Err(in_file(Fault {
                at: *p,
                kind: ReadErrorKind::Syntax("expected `#` or ENDSEC"),
            }));
        }
        let start = *p;
        let id = instance_name(data, p).map_err(in_file)?;
        let in_entity = |f| ReadError::new(data, f, Some(id));
        expect(data, p, b'=', "expected `=`").map_err(in_entity)?;
        skip_space(data, p).map_err(in_entity)?;
        // A complex instance, `#1=(A(...)B(...));`, has no name of its own.
        let name = if data.get(*p) == Some(&b'(') {
            ""
        } else {
            let name = keyword(data, p).ok_or(Fault {
                at: *p,
                kind: ReadErrorKind::Syntax("expected an entity name"),
            });
            let name = name.map_err(in_entity)?;
            skip_space(data, p).map_err(in_entity)?;
            name
        };
        *p = skip_record(data, *p).map_err(in_entity)?;
        index.add(id, name, start);
    }
}

/// The bytes [`skip_record`] has to look at; it jumps over everything else.
const SPECIAL: [bool; 256] = {
    let mut special = [false; 256];
    let mut i = 0;
    while i < 6 {
        special[b"'\"/();"[i] as usize] = true;
        i += 1;
    }
    special
};

/// From the `(` that opens an instance's attributes, returns the offset just
/// past its closing `;`, without decoding anything. Strings, binaries and
/// comments are skipped whole, so a `;` or `)` inside them doesn't count.
fn skip_record(data: &[u8], open: usize) -> Parse<usize> {
    if data.get(open) != Some(&b'(') {
        return fault(open, ReadErrorKind::Syntax("expected `(`"));
    }
    let mut depth = 0usize;
    let mut p = open;
    loop {
        let rest = data.get(p..).unwrap_or_default();
        p += rest
            .iter()
            .position(|&b| SPECIAL[usize::from(b)])
            .unwrap_or(rest.len());
        match data.get(p) {
            None => return fault(open, ReadErrorKind::Unterminated("record")),
            Some(b'\'') => p = skip_string(data, p)?,
            Some(b'"') => p = skip_binary(data, p)?,
            Some(b'/') if data.get(p + 1) == Some(&b'*') => p = skip_comment(data, p)?,
            Some(b'(') => {
                depth += 1;
                p += 1;
            }
            Some(b')') => {
                depth -= 1;
                p += 1;
                if depth == 0 {
                    break;
                }
            }
            // Outside a string, `;` can only end the instance.
            Some(b';') => return fault(p, ReadErrorKind::Syntax("expected `,` or `)`")),
            Some(_) => p += 1,
        }
    }
    expect(data, &mut p, b';', "expected `;`")?;
    Ok(p)
}

/// Decodes the instance at `slot`, which the index pass has already checked
/// for structure.
fn decode_entity(data: &[u8], slot: Slot) -> Parse<Entity<'_>> {
    let mut p = slot.start;
    let id = instance_name(data, &mut p)?;
    expect(data, &mut p, b'=', "expected `=`")?;
    skip_space(data, &mut p)?;
    let (name, attributes) = if data.get(p) == Some(&b'(') {
        p += 1;
        let mut parts = Vec::new();
        loop {
            skip_space(data, &mut p)?;
            if data.get(p) == Some(&b')') {
                p += 1;
                break;
            }
            let Some(name) = keyword(data, &mut p) else {
                return fault(p, ReadErrorKind::Syntax("expected an entity name"));
            };
            skip_space(data, &mut p)?;
            let attributes = list(data, &mut p, 1)?;
            parts.push(Value::Typed(
                Cow::Owned(name.to_owned()),
                Box::new(Value::List(attributes)),
            ));
        }
        ("", parts)
    } else {
        let Some(name) = keyword(data, &mut p) else {
            return fault(p, ReadErrorKind::Syntax("expected an entity name"));
        };
        skip_space(data, &mut p)?;
        (name, list(data, &mut p, 0)?)
    };
    expect(data, &mut p, b';', "expected `;`")?;
    Ok(Entity {
        id,
        name,
        attributes,
    })
}
