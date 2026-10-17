//! The persisted change record: the storage adapter's own versioned type,
//! not the core's `ChangeSet` with serde derives. The core's types keep
//! changing; a record in a file must read the same for decades.

use hito_core::{
    Aspect, ChangeSet, ClassId, Code, CodeSpec, Command, Element, ElementChange, ElementId,
    ExternalSource, FieldChange, Model, ModelId, ModelKind, Relationship, RelationshipChange,
    Value,
};
use serde::{Deserialize, Serialize};

type Id = [u8; 16];

fn id(id: ElementId) -> Id {
    id.as_u128().to_be_bytes()
}

fn element_id(id: Id) -> ElementId {
    ElementId::from_u128(u128::from_be_bytes(id))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeRecord {
    pub command: Option<CommandRec>,
    pub models: Vec<(Id, u8)>,
    pub elements: Vec<ElementChangeRec>,
    pub relationships: Vec<(bool, RelationshipRec)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandRec {
    pub name: String,
    pub parameters: Vec<(String, ValueRec)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ValueRec {
    Boolean(bool),
    Integer(i64),
    Real(f64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassRec(pub String, pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeRec {
    pub spec: String,
    pub scope: Id,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceRec {
    pub source: Id,
    pub identifier: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AspectRec {
    pub class: ClassRec,
    pub properties: Vec<(String, ValueRec)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementRec {
    pub id: Id,
    pub class: ClassRec,
    pub model: Id,
    pub type_id: Option<Id>,
    pub code: Option<CodeRec>,
    pub source: Option<SourceRec>,
    pub properties: Vec<(String, ValueRec)>,
    pub aspects: Vec<AspectRec>,
}

impl ElementRec {
    /// Sets a field to its value after a change, as the core does.
    pub fn patch(&mut self, field: &FieldRec) {
        match field {
            FieldRec::Property(name, _, after) => {
                self.properties.retain(|(k, _)| k != name);
                if let Some(v) = after {
                    let at = self.properties.partition_point(|(k, _)| k < name);
                    self.properties.insert(at, (name.clone(), v.clone()));
                }
            }
            FieldRec::Type(_, after) => self.type_id = *after,
            FieldRec::Code(_, after) => self.code = after.clone(),
            FieldRec::Source(_, after) => self.source = after.clone(),
            FieldRec::Aspects(_, after) => self.aspects = after.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldRec {
    Property(String, Option<ValueRec>, Option<ValueRec>),
    Type(Option<Id>, Option<Id>),
    Code(Option<CodeRec>, Option<CodeRec>),
    Source(Option<SourceRec>, Option<SourceRec>),
    Aspects(Vec<AspectRec>, Vec<AspectRec>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ElementChangeRec {
    Created(ElementRec),
    Modified(Id, Vec<FieldRec>),
    Deleted(ElementRec),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationshipRec {
    pub class: ClassRec,
    pub source: Id,
    pub target: Id,
}

fn value(v: &Value) -> ValueRec {
    match v {
        Value::Boolean(b) => ValueRec::Boolean(*b),
        Value::Integer(i) => ValueRec::Integer(*i),
        Value::Real(x) => ValueRec::Real(*x),
        Value::Text(s) => ValueRec::Text(s.clone()),
    }
}

fn to_value(v: &ValueRec) -> Value {
    match v {
        ValueRec::Boolean(b) => Value::Boolean(*b),
        ValueRec::Integer(i) => Value::Integer(*i),
        ValueRec::Real(x) => Value::Real(*x),
        ValueRec::Text(s) => Value::Text(s.clone()),
    }
}

fn class(c: &ClassId) -> ClassRec {
    ClassRec(c.schema().to_owned(), c.name().to_owned())
}

fn to_class(c: &ClassRec) -> ClassId {
    ClassId::new(c.0.clone(), c.1.clone())
}

fn code(c: &Code) -> CodeRec {
    CodeRec {
        spec: c.spec().name().to_owned(),
        scope: id(c.scope()),
        value: c.value().to_owned(),
    }
}

fn to_code(c: &CodeRec) -> Code {
    Code::new(
        CodeSpec::new(c.spec.clone()),
        element_id(c.scope),
        c.value.clone(),
    )
    .expect("stored codes aren't empty")
}

fn source(s: &ExternalSource) -> SourceRec {
    SourceRec {
        source: id(s.source()),
        identifier: s.identifier().to_owned(),
    }
}

fn to_source(s: &SourceRec) -> ExternalSource {
    ExternalSource::new(element_id(s.source), s.identifier.clone())
}

fn aspect(a: &Aspect) -> AspectRec {
    AspectRec {
        class: class(a.class()),
        properties: a
            .properties()
            .iter()
            .map(|(k, v)| (k.clone(), value(v)))
            .collect(),
    }
}

fn to_aspect(a: &AspectRec) -> Aspect {
    a.properties
        .iter()
        .fold(Aspect::new(to_class(&a.class)), |a, (k, v)| {
            a.with_property(k.clone(), to_value(v))
        })
}

pub fn element(e: &Element) -> ElementRec {
    ElementRec {
        id: id(e.id()),
        class: class(e.class()),
        model: e.model().as_u128().to_be_bytes(),
        type_id: e.type_id().map(id),
        code: e.code().map(code),
        source: e.source().map(source),
        properties: e
            .properties()
            .iter()
            .map(|(k, v)| (k.clone(), value(v)))
            .collect(),
        aspects: e.aspects().iter().map(aspect).collect(),
    }
}

pub fn to_element(e: &ElementRec) -> Element {
    let model = ModelId::from_u128(u128::from_be_bytes(e.model));
    let mut element = Element::with_id(element_id(e.id), to_class(&e.class), model);
    if let Some(t) = e.type_id {
        element = element.with_type(element_id(t));
    }
    if let Some(c) = &e.code {
        element = element.with_code(to_code(c));
    }
    if let Some(s) = &e.source {
        element = element.with_source(to_source(s));
    }
    for (k, v) in &e.properties {
        element = element.with_property(k.clone(), to_value(v));
    }
    for a in &e.aspects {
        element = element.with_aspect(to_aspect(a));
    }
    element
}

fn model_kind(kind: ModelKind) -> u8 {
    match kind {
        ModelKind::Physical => 0,
        ModelKind::Definition => 1,
        ModelKind::Drawing => 2,
        ModelKind::Analytical => 3,
        ModelKind::Linked => 4,
        _ => unreachable!("spike knows every kind"),
    }
}

fn to_model_kind(kind: u8) -> ModelKind {
    [
        ModelKind::Physical,
        ModelKind::Definition,
        ModelKind::Drawing,
        ModelKind::Analytical,
        ModelKind::Linked,
    ][kind as usize]
}

fn relationship(r: &Relationship) -> RelationshipRec {
    RelationshipRec {
        class: class(r.class()),
        source: id(r.source()),
        target: id(r.target()),
    }
}

fn field(f: &FieldChange) -> FieldRec {
    match f {
        FieldChange::Property {
            name,
            before,
            after,
        } => FieldRec::Property(
            name.clone(),
            before.as_ref().map(value),
            after.as_ref().map(value),
        ),
        FieldChange::Type { before, after } => FieldRec::Type(before.map(id), after.map(id)),
        FieldChange::Code { before, after } => {
            FieldRec::Code(before.as_ref().map(code), after.as_ref().map(code))
        }
        FieldChange::Source { before, after } => {
            FieldRec::Source(before.as_ref().map(source), after.as_ref().map(source))
        }
        FieldChange::Aspects { before, after } => FieldRec::Aspects(
            before.iter().map(aspect).collect(),
            after.iter().map(aspect).collect(),
        ),
    }
}

fn to_field(f: &FieldRec) -> FieldChange {
    match f {
        FieldRec::Property(name, b, a) => FieldChange::Property {
            name: name.clone(),
            before: b.as_ref().map(to_value),
            after: a.as_ref().map(to_value),
        },
        FieldRec::Type(b, a) => FieldChange::Type {
            before: b.map(element_id),
            after: a.map(element_id),
        },
        FieldRec::Code(b, a) => FieldChange::Code {
            before: b.as_ref().map(to_code),
            after: a.as_ref().map(to_code),
        },
        FieldRec::Source(b, a) => FieldChange::Source {
            before: b.as_ref().map(to_source),
            after: a.as_ref().map(to_source),
        },
        FieldRec::Aspects(b, a) => FieldChange::Aspects {
            before: b.iter().map(to_aspect).collect(),
            after: a.iter().map(to_aspect).collect(),
        },
    }
}

impl ChangeRecord {
    pub fn from_change_set(changes: &ChangeSet) -> Self {
        Self {
            command: changes.command().map(|c| CommandRec {
                name: c.name().to_owned(),
                parameters: c
                    .parameters()
                    .iter()
                    .map(|(k, v)| (k.clone(), value(v)))
                    .collect(),
            }),
            models: changes
                .models()
                .iter()
                .map(|m| (m.id().as_u128().to_be_bytes(), model_kind(m.kind())))
                .collect(),
            elements: changes
                .elements()
                .iter()
                .map(|c| match c {
                    ElementChange::Created(e) => ElementChangeRec::Created(element(e)),
                    ElementChange::Deleted(e) => ElementChangeRec::Deleted(element(e)),
                    ElementChange::Modified { id: i, fields } => {
                        ElementChangeRec::Modified(id(*i), fields.iter().map(field).collect())
                    }
                })
                .collect(),
            relationships: changes
                .relationships()
                .iter()
                .map(|r| match r {
                    RelationshipChange::Added(r) => (true, relationship(r)),
                    RelationshipChange::Removed(r) => (false, relationship(r)),
                })
                .collect(),
        }
    }

    pub fn to_change_set(&self) -> ChangeSet {
        let command = self.command.as_ref().map(|c| {
            c.parameters
                .iter()
                .fold(Command::new(c.name.clone()), |c, (k, v)| {
                    c.with_parameter(k.clone(), to_value(v))
                })
        });
        let models = self
            .models
            .iter()
            .map(|(m, k)| {
                Model::with_id(
                    ModelId::from_u128(u128::from_be_bytes(*m)),
                    to_model_kind(*k),
                )
            })
            .collect();
        let elements = self
            .elements
            .iter()
            .map(|c| match c {
                ElementChangeRec::Created(e) => ElementChange::Created(to_element(e)),
                ElementChangeRec::Deleted(e) => ElementChange::Deleted(to_element(e)),
                ElementChangeRec::Modified(i, fields) => ElementChange::Modified {
                    id: element_id(*i),
                    fields: fields.iter().map(to_field).collect(),
                },
            })
            .collect();
        let relationships = self
            .relationships
            .iter()
            .map(|(added, r)| {
                let r = Relationship::new(
                    to_class(&r.class),
                    element_id(r.source),
                    element_id(r.target),
                );
                match added {
                    true => RelationshipChange::Added(r),
                    false => RelationshipChange::Removed(r),
                }
            })
            .collect();
        ChangeSet::from_parts(command, models, elements, relationships)
    }
}

/// The encodings compared.
#[derive(Debug, Clone, Copy)]
pub enum Encoding {
    Postcard,
    PostcardZstd,
    PostcardLz4,
    Cbor,
    CborZstd,
    Json,
    JsonZstd,
}

impl Encoding {
    pub const ALL: [Self; 7] = [
        Self::Postcard,
        Self::PostcardZstd,
        Self::PostcardLz4,
        Self::Cbor,
        Self::CborZstd,
        Self::Json,
        Self::JsonZstd,
    ];

    pub fn encode(self, record: &ChangeRecord) -> Vec<u8> {
        let cbor = |r: &ChangeRecord| {
            let mut out = Vec::new();
            ciborium::into_writer(r, &mut out).unwrap();
            out
        };
        match self {
            Self::Postcard => postcard::to_stdvec(record).unwrap(),
            Self::PostcardZstd => {
                zstd::encode_all(&postcard::to_stdvec(record).unwrap()[..], 3).unwrap()
            }
            Self::PostcardLz4 => {
                lz4_flex::compress_prepend_size(&postcard::to_stdvec(record).unwrap())
            }
            Self::Cbor => cbor(record),
            Self::CborZstd => zstd::encode_all(&cbor(record)[..], 3).unwrap(),
            Self::Json => serde_json::to_vec(record).unwrap(),
            Self::JsonZstd => {
                zstd::encode_all(&serde_json::to_vec(record).unwrap()[..], 3).unwrap()
            }
        }
    }

    pub fn decode(self, bytes: &[u8]) -> ChangeRecord {
        match self {
            Self::Postcard => postcard::from_bytes(bytes).unwrap(),
            Self::PostcardZstd => postcard::from_bytes(&zstd::decode_all(bytes).unwrap()).unwrap(),
            Self::PostcardLz4 => {
                postcard::from_bytes(&lz4_flex::decompress_size_prepended(bytes).unwrap()).unwrap()
            }
            Self::Cbor => ciborium::from_reader(bytes).unwrap(),
            Self::CborZstd => ciborium::from_reader(&zstd::decode_all(bytes).unwrap()[..]).unwrap(),
            Self::Json => serde_json::from_slice(bytes).unwrap(),
            Self::JsonZstd => serde_json::from_slice(&zstd::decode_all(bytes).unwrap()).unwrap(),
        }
    }
}
