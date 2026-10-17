//! Change records: the unit of a `.hito` file's history
//! ([ADR 0024](https://github.com/hito-project/hito/blob/main/docs/decisions/0024-model-history.md#the-record)).
//!
//! A [`ChangeRecord`] is a committed [`ChangeSet`] with who made it, when,
//! with which schema versions, and where it sits in the history. It's
//! encoded with postcard into the adapter's own versioned format, in
//! [`v1`], not with the core's types: those keep changing, and a record in
//! a file must read the same for decades.

use std::collections::BTreeMap;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use hito_core::{
    Aspect, ChangeSet, ClassId, Element, ElementChange, ElementStore, FieldChange, ModelKind,
    SchemaVersion,
};

mod v1;

/// A committed change set as the file's history stores it.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeRecord {
    /// The records this one follows, when it isn't simply the previous one:
    /// a merge's two parents, or a branch's first record. Empty in a
    /// straight-line history.
    pub parents: Vec<RecordId>,
    /// Who made the change.
    pub author: String,
    pub time: Timestamp,
    /// The versions of the schemas whose classes the change uses, by schema
    /// name, so a later version can migrate it (#48).
    pub schemas: BTreeMap<String, SchemaVersion>,
    /// What the change applies to.
    pub target: Target,
    /// What changed. A record keeps no warnings: they come from the model,
    /// and aren't facts about the change.
    pub changes: ChangeSet,
}

impl ChangeRecord {
    /// A record of `changes`, just committed to `store`, made now by
    /// `author` on the main model, after the previous record. It lists the
    /// version of each schema whose classes the change uses.
    pub fn new(changes: ChangeSet, store: &ElementStore, author: impl Into<String>) -> Self {
        let registry = store.registry();
        let schemas = classes(&changes, store)
            .into_iter()
            .filter_map(|class| {
                let version = registry.schema_version(class.schema())?;
                Some((class.schema().to_owned(), version))
            })
            .collect();
        Self {
            parents: Vec::new(),
            author: author.into(),
            time: Timestamp::now(),
            schemas,
            target: Target::Main,
            changes,
        }
    }

    /// The record in the file's format. Encoding the same record always
    /// gives the same bytes, so they can be hashed.
    ///
    /// Fails only if the record holds something the format can't store
    /// yet, such as a model kind newer than the format.
    pub fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        let record = v1::Versioned::V1(v1::Record::try_from(self)?);
        // postcard fails only on types serde can't describe up front, such
        // as iterators of unknown length; the record has none.
        Ok(postcard::to_stdvec(&record).expect("the record's types always encode"))
    }

    /// Reads a record written by [`encode`](Self::encode), by this or an
    /// earlier version of HITO.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let (record, rest) = postcard::take_from_bytes::<v1::Versioned>(bytes)
            .map_err(|_| DecodeError::Malformed)?;
        if !rest.is_empty() {
            return Err(DecodeError::Malformed);
        }
        match record {
            v1::Versioned::V1(record) => record.try_into(),
        }
    }
}

/// Every class a change set uses, with repeats. A modified element's class,
/// and the class of an aspect whose properties changed, come from the store.
fn classes<'a>(changes: &'a ChangeSet, store: &'a ElementStore) -> Vec<&'a ClassId> {
    let mut classes = Vec::new();
    for change in changes.elements() {
        match change {
            ElementChange::Created { element, .. } | ElementChange::Deleted { element, .. } => {
                classes.push(element.class());
                classes.extend(element.aspects().iter().map(Aspect::class));
            }
            ElementChange::Modified { id, fields } => {
                let element = store.get(*id);
                classes.extend(element.map(Element::class));
                for field in fields {
                    match &field.change {
                        FieldChange::Aspect { before, after, .. } => {
                            classes.extend(before.iter().chain(after).map(Aspect::class));
                        }
                        FieldChange::AspectProperty { aspect, .. } => {
                            let aspects = element.into_iter().flat_map(Element::aspects);
                            let aspect = aspects.filter(|a| a.id() == *aspect);
                            classes.extend(aspect.map(Aspect::class));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    classes.extend(
        changes
            .relationships()
            .iter()
            .map(|c| c.relationship().class()),
    );
    classes
}

/// A record's identity: the BLAKE3 hash of its encoding and its parents'
/// IDs. The file computes it when it appends the record (#47).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RecordId(pub [u8; 32]);

/// A moment in time, to the millisecond, in UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(i64);

impl Timestamp {
    /// Now, by the system clock. A clock set before 1970 gives 1970.
    pub fn now() -> Self {
        let since = SystemTime::now().duration_since(UNIX_EPOCH);
        let ms = since.unwrap_or_default().as_millis();
        Self(i64::try_from(ms).unwrap_or(i64::MAX))
    }

    /// The moment this many milliseconds after 1970-01-01 00:00 UTC, or
    /// before it if negative.
    pub const fn from_millis(ms: i64) -> Self {
        Self(ms)
    }

    pub const fn as_millis(self) -> i64 {
        self.0
    }
}

/// What a change applies to. Branches and design options come later
/// (ADR 0024).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Target {
    /// The main model.
    Main,
}

/// Why [`ChangeRecord::encode`] couldn't write a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// A model kind added to the core after this version of the format.
    /// The format needs a new variant for it.
    UnknownModelKind(ModelKind),
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownModelKind(kind) => {
                write!(f, "change records can't store a {kind:?} model yet")
            }
        }
    }
}

impl std::error::Error for EncodeError {}

/// Why [`ChangeRecord::decode`] couldn't read a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The bytes aren't a record in any known version of the format.
    Malformed,
    /// The record holds an empty code, which isn't a code.
    EmptyCode,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => f.write_str("not a change record"),
            Self::EmptyCode => f.write_str("the change record holds an empty code"),
        }
    }
}

impl std::error::Error for DecodeError {}
