//! Version 1 of the change record format, documented with the file format
//! (#49).
//!
//! These types are the format: postcard writes struct fields in the order
//! they're declared, and enum variants by their position. Never reorder,
//! remove or change them. A field can't be added either, since old records
//! wouldn't have it; a change like that is a new version. A variant can be
//! added at the end of an enum, because old records never use it.
//!
//! IDs are their 16 bytes, most significant first. Property values are
//! sorted by name, as the core keeps them, so the same record always
//! encodes to the same bytes.

use std::collections::BTreeMap;

use hito_core::{CodeSpec, ElementId, ModelId};
use serde::{Deserialize, Serialize};

use super::DecodeError;

type Id = [u8; 16];

/// Every version of the record. A record starts with its version's
/// position here.
#[derive(Debug, Serialize, Deserialize)]
pub(super) enum Versioned {
    V1(Record),
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Record {
    parents: Vec<[u8; 32]>,
    author: String,
    /// Milliseconds since 1970-01-01 00:00 UTC.
    time: i64,
    /// Name, then read, write and minor version.
    schemas: Vec<(String, u16, u16, u16)>,
    target: Target,
    command: Option<Command>,
    models: Vec<Model>,
    elements: Vec<ElementChange>,
    relationships: Vec<RelationshipChange>,
}

#[derive(Debug, Serialize, Deserialize)]
enum Target {
    Main,
}

#[derive(Debug, Serialize, Deserialize)]
struct Command {
    name: String,
    parameters: Properties,
}

type Properties = Vec<(String, Value)>;

#[derive(Debug, Serialize, Deserialize)]
enum Value {
    Boolean(bool),
    Integer(i64),
    /// Its IEEE 754 bits, so it reads back exactly.
    Real(f64),
    Text(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct Model {
    id: Id,
    kind: ModelKind,
}

#[derive(Debug, Serialize, Deserialize)]
enum ModelKind {
    Physical,
    Definition,
    Drawing,
    Analytical,
    Linked,
}

/// Schema name, then class name.
#[derive(Debug, Serialize, Deserialize)]
struct Class(String, String);

#[derive(Debug, Serialize, Deserialize)]
struct Code {
    spec: String,
    scope: Id,
    value: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Source {
    source: Id,
    identifier: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Aspect {
    class: Class,
    properties: Properties,
}

#[derive(Debug, Serialize, Deserialize)]
struct Element {
    id: Id,
    class: Class,
    model: Id,
    type_id: Option<Id>,
    code: Option<Code>,
    source: Option<Source>,
    properties: Properties,
    aspects: Vec<Aspect>,
}

#[derive(Debug, Serialize, Deserialize)]
enum ElementChange {
    Created(Element),
    Modified { id: Id, fields: Vec<FieldChange> },
    Deleted(Element),
}

/// Each field's value before, then after.
#[derive(Debug, Serialize, Deserialize)]
enum FieldChange {
    Property(String, Option<Value>, Option<Value>),
    Type(Option<Id>, Option<Id>),
    Code(Option<Code>, Option<Code>),
    Source(Option<Source>, Option<Source>),
    Aspects(Vec<Aspect>, Vec<Aspect>),
}

#[derive(Debug, Serialize, Deserialize)]
enum RelationshipChange {
    Added(Relationship),
    Removed(Relationship),
}

#[derive(Debug, Serialize, Deserialize)]
struct Relationship {
    class: Class,
    source: Id,
    target: Id,
}

// From the core's types to the record's.

impl From<&super::ChangeRecord> for Record {
    fn from(record: &super::ChangeRecord) -> Self {
        let changes = &record.changes;
        Self {
            parents: record.parents.iter().map(|p| p.0).collect(),
            author: record.author.clone(),
            time: record.time.as_millis(),
            schemas: record
                .schemas
                .iter()
                .map(|(name, v)| (name.clone(), v.read, v.write, v.minor))
                .collect(),
            target: match record.target {
                super::Target::Main => Target::Main,
            },
            command: changes.command().map(|c| Command {
                name: c.name().to_owned(),
                parameters: properties(c.parameters()),
            }),
            models: changes.models().iter().map(Model::from).collect(),
            elements: changes.elements().iter().map(ElementChange::from).collect(),
            relationships: changes
                .relationships()
                .iter()
                .map(RelationshipChange::from)
                .collect(),
        }
    }
}

fn properties(properties: &hito_core::Properties) -> Properties {
    let values = properties.iter();
    values.map(|(name, v)| (name.clone(), v.into())).collect()
}

impl From<&hito_core::Value> for Value {
    fn from(value: &hito_core::Value) -> Self {
        match value {
            hito_core::Value::Boolean(b) => Self::Boolean(*b),
            hito_core::Value::Integer(i) => Self::Integer(*i),
            hito_core::Value::Real(x) => Self::Real(*x),
            hito_core::Value::Text(s) => Self::Text(s.clone()),
        }
    }
}

impl From<&hito_core::Model> for Model {
    fn from(model: &hito_core::Model) -> Self {
        let kind = match model.kind() {
            hito_core::ModelKind::Physical => ModelKind::Physical,
            hito_core::ModelKind::Definition => ModelKind::Definition,
            hito_core::ModelKind::Drawing => ModelKind::Drawing,
            hito_core::ModelKind::Analytical => ModelKind::Analytical,
            hito_core::ModelKind::Linked => ModelKind::Linked,
            kind => unimplemented!("the record format has no model kind {kind:?} yet"),
        };
        Self {
            id: model.id().as_u128().to_be_bytes(),
            kind,
        }
    }
}

impl From<&hito_core::ClassId> for Class {
    fn from(class: &hito_core::ClassId) -> Self {
        Self(class.schema().to_owned(), class.name().to_owned())
    }
}

impl From<&hito_core::Code> for Code {
    fn from(code: &hito_core::Code) -> Self {
        Self {
            spec: code.spec().name().to_owned(),
            scope: code.scope().to_bytes(),
            value: code.value().to_owned(),
        }
    }
}

impl From<&hito_core::ExternalSource> for Source {
    fn from(source: &hito_core::ExternalSource) -> Self {
        Self {
            source: source.source().to_bytes(),
            identifier: source.identifier().to_owned(),
        }
    }
}

impl From<&hito_core::Aspect> for Aspect {
    fn from(aspect: &hito_core::Aspect) -> Self {
        Self {
            class: aspect.class().into(),
            properties: properties(aspect.properties()),
        }
    }
}

impl From<&hito_core::Element> for Element {
    fn from(element: &hito_core::Element) -> Self {
        Self {
            id: element.id().to_bytes(),
            class: element.class().into(),
            model: element.model().as_u128().to_be_bytes(),
            type_id: element.type_id().map(ElementId::to_bytes),
            code: element.code().map(Code::from),
            source: element.source().map(Source::from),
            properties: properties(element.properties()),
            aspects: element.aspects().iter().map(Aspect::from).collect(),
        }
    }
}

impl From<&hito_core::ElementChange> for ElementChange {
    fn from(change: &hito_core::ElementChange) -> Self {
        match change {
            hito_core::ElementChange::Created(e) => Self::Created(e.into()),
            hito_core::ElementChange::Modified { id, fields } => Self::Modified {
                id: id.to_bytes(),
                fields: fields.iter().map(FieldChange::from).collect(),
            },
            hito_core::ElementChange::Deleted(e) => Self::Deleted(e.into()),
        }
    }
}

impl From<&hito_core::FieldChange> for FieldChange {
    fn from(field: &hito_core::FieldChange) -> Self {
        use hito_core::FieldChange as F;
        let id = |id: &Option<ElementId>| id.map(ElementId::to_bytes);
        let aspects = |a: &[hito_core::Aspect]| a.iter().map(Aspect::from).collect();
        match field {
            F::Property {
                name,
                before,
                after,
            } => Self::Property(
                name.clone(),
                before.as_ref().map(Value::from),
                after.as_ref().map(Value::from),
            ),
            F::Type { before, after } => Self::Type(id(before), id(after)),
            F::Code { before, after } => Self::Code(
                before.as_ref().map(Code::from),
                after.as_ref().map(Code::from),
            ),
            F::Source { before, after } => Self::Source(
                before.as_ref().map(Source::from),
                after.as_ref().map(Source::from),
            ),
            F::Aspects { before, after } => Self::Aspects(aspects(before), aspects(after)),
        }
    }
}

impl From<&hito_core::RelationshipChange> for RelationshipChange {
    fn from(change: &hito_core::RelationshipChange) -> Self {
        let relationship = |r: &hito_core::Relationship| Relationship {
            class: r.class().into(),
            source: r.source().to_bytes(),
            target: r.target().to_bytes(),
        };
        match change {
            hito_core::RelationshipChange::Added(r) => Self::Added(relationship(r)),
            hito_core::RelationshipChange::Removed(r) => Self::Removed(relationship(r)),
        }
    }
}

// From the record's types back to the core's.

impl TryFrom<Record> for super::ChangeRecord {
    type Error = DecodeError;

    fn try_from(record: Record) -> Result<Self, DecodeError> {
        let command = record.command.map(|c| {
            let parameters = c.parameters.into_iter();
            parameters.fold(hito_core::Command::new(c.name), |command, (name, v)| {
                command.with_parameter(name, v)
            })
        });
        let elements = record.elements.into_iter().map(TryInto::try_into);
        let changes = hito_core::ChangeSet::from_parts(
            command,
            record.models.into_iter().map(Into::into).collect(),
            elements.collect::<Result<_, _>>()?,
            record.relationships.into_iter().map(Into::into).collect(),
        );
        let schemas = record.schemas.into_iter();
        Ok(Self {
            parents: record.parents.into_iter().map(super::RecordId).collect(),
            author: record.author,
            time: super::Timestamp::from_millis(record.time),
            schemas: schemas
                .map(|(name, read, write, minor)| {
                    (name, hito_core::SchemaVersion::new(read, write, minor))
                })
                .collect::<BTreeMap<_, _>>(),
            target: match record.target {
                Target::Main => super::Target::Main,
            },
            changes,
        })
    }
}

fn element_id(id: Id) -> ElementId {
    ElementId::from_bytes(id)
}

fn model_id(id: Id) -> ModelId {
    ModelId::from_u128(u128::from_be_bytes(id))
}

impl From<Value> for hito_core::Value {
    fn from(value: Value) -> Self {
        match value {
            Value::Boolean(b) => Self::Boolean(b),
            Value::Integer(i) => Self::Integer(i),
            Value::Real(x) => Self::Real(x),
            Value::Text(s) => Self::Text(s),
        }
    }
}

impl From<Model> for hito_core::Model {
    fn from(model: Model) -> Self {
        let kind = match model.kind {
            ModelKind::Physical => hito_core::ModelKind::Physical,
            ModelKind::Definition => hito_core::ModelKind::Definition,
            ModelKind::Drawing => hito_core::ModelKind::Drawing,
            ModelKind::Analytical => hito_core::ModelKind::Analytical,
            ModelKind::Linked => hito_core::ModelKind::Linked,
        };
        Self::with_id(model_id(model.id), kind)
    }
}

impl From<Class> for hito_core::ClassId {
    fn from(Class(schema, name): Class) -> Self {
        Self::new(schema, name)
    }
}

impl TryFrom<Code> for hito_core::Code {
    type Error = DecodeError;

    fn try_from(code: Code) -> Result<Self, DecodeError> {
        let spec = CodeSpec::new(code.spec);
        Self::new(spec, element_id(code.scope), code.value).map_err(|_| DecodeError::EmptyCode)
    }
}

impl From<Source> for hito_core::ExternalSource {
    fn from(source: Source) -> Self {
        Self::new(element_id(source.source), source.identifier)
    }
}

impl From<Aspect> for hito_core::Aspect {
    fn from(aspect: Aspect) -> Self {
        let properties = aspect.properties.into_iter();
        properties.fold(Self::new(aspect.class.into()), |aspect, (name, v)| {
            aspect.with_property(name, v)
        })
    }
}

impl TryFrom<Element> for hito_core::Element {
    type Error = DecodeError;

    fn try_from(e: Element) -> Result<Self, DecodeError> {
        let mut element = Self::with_id(element_id(e.id), e.class.into(), model_id(e.model));
        if let Some(type_id) = e.type_id {
            element = element.with_type(element_id(type_id));
        }
        if let Some(code) = e.code {
            element = element.with_code(code.try_into()?);
        }
        if let Some(source) = e.source {
            element = element.with_source(source.into());
        }
        for (name, value) in e.properties {
            element = element.with_property(name, value);
        }
        Ok(e.aspects.into_iter().fold(element, |element, aspect| {
            element.with_aspect(aspect.into())
        }))
    }
}

impl TryFrom<ElementChange> for hito_core::ElementChange {
    type Error = DecodeError;

    fn try_from(change: ElementChange) -> Result<Self, DecodeError> {
        Ok(match change {
            ElementChange::Created(e) => Self::Created(e.try_into()?),
            ElementChange::Modified { id, fields } => Self::Modified {
                id: element_id(id),
                fields: fields
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            },
            ElementChange::Deleted(e) => Self::Deleted(e.try_into()?),
        })
    }
}

impl TryFrom<FieldChange> for hito_core::FieldChange {
    type Error = DecodeError;

    fn try_from(field: FieldChange) -> Result<Self, DecodeError> {
        let id = |id: Option<Id>| id.map(element_id);
        let code = |code: Option<Code>| code.map(TryInto::try_into).transpose();
        let aspects = |a: Vec<Aspect>| a.into_iter().map(Into::into).collect();
        Ok(match field {
            FieldChange::Property(name, before, after) => Self::Property {
                name,
                before: before.map(Into::into),
                after: after.map(Into::into),
            },
            FieldChange::Type(before, after) => Self::Type {
                before: id(before),
                after: id(after),
            },
            FieldChange::Code(before, after) => Self::Code {
                before: code(before)?,
                after: code(after)?,
            },
            FieldChange::Source(before, after) => Self::Source {
                before: before.map(Into::into),
                after: after.map(Into::into),
            },
            FieldChange::Aspects(before, after) => Self::Aspects {
                before: aspects(before),
                after: aspects(after),
            },
        })
    }
}

impl From<RelationshipChange> for hito_core::RelationshipChange {
    fn from(change: RelationshipChange) -> Self {
        let relationship = |r: Relationship| {
            hito_core::Relationship::new(r.class.into(), element_id(r.source), element_id(r.target))
        };
        match change {
            RelationshipChange::Added(r) => Self::Added(relationship(r)),
            RelationshipChange::Removed(r) => Self::Removed(relationship(r)),
        }
    }
}
