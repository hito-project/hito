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

use hito_core::{AspectId, CodeSpec, ElementId, ModelId};
use serde::{Deserialize, Serialize};

use super::{DecodeError, EncodeError};

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
    id: Id,
    class: Class,
    properties: Properties,
}

/// Who made a change.
#[derive(Debug, Serialize, Deserialize)]
enum Origin {
    User,
    Propagated,
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
    Created(Origin, Element),
    Modified { id: Id, fields: Vec<ChangedField> },
    Deleted(Origin, Element),
}

#[derive(Debug, Serialize, Deserialize)]
struct ChangedField {
    origin: Origin,
    change: FieldChange,
}

/// Each field's value before, then after.
#[derive(Debug, Serialize, Deserialize)]
enum FieldChange {
    Property(String, Option<Value>, Option<Value>),
    Type(Option<Id>, Option<Id>),
    Code(Option<Code>, Option<Code>),
    Source(Option<Source>, Option<Source>),
    /// The aspect's ID, then the whole aspect before and after.
    Aspect(Id, Option<Aspect>, Option<Aspect>),
    /// The aspect's ID and the property's name, then its value before and
    /// after.
    AspectProperty(Id, String, Option<Value>, Option<Value>),
}

#[derive(Debug, Serialize, Deserialize)]
enum RelationshipChange {
    Added(Origin, Relationship),
    Removed(Origin, Relationship),
}

#[derive(Debug, Serialize, Deserialize)]
struct Relationship {
    class: Class,
    source: Id,
    target: Id,
}

// From the core's types to the record's.

impl TryFrom<&super::ChangeRecord> for Record {
    type Error = EncodeError;

    fn try_from(record: &super::ChangeRecord) -> Result<Self, EncodeError> {
        let changes = &record.changes;
        let models = changes.models().iter().map(Model::try_from);
        Ok(Self {
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
            models: models.collect::<Result<_, _>>()?,
            elements: changes.elements().iter().map(ElementChange::from).collect(),
            relationships: changes
                .relationships()
                .iter()
                .map(RelationshipChange::from)
                .collect(),
        })
    }
}

impl From<hito_core::Origin> for Origin {
    fn from(origin: hito_core::Origin) -> Self {
        match origin {
            hito_core::Origin::User => Self::User,
            hito_core::Origin::Propagated => Self::Propagated,
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

impl TryFrom<&hito_core::Model> for Model {
    type Error = EncodeError;

    fn try_from(model: &hito_core::Model) -> Result<Self, EncodeError> {
        let kind = match model.kind() {
            hito_core::ModelKind::Physical => ModelKind::Physical,
            hito_core::ModelKind::Definition => ModelKind::Definition,
            hito_core::ModelKind::Drawing => ModelKind::Drawing,
            hito_core::ModelKind::Analytical => ModelKind::Analytical,
            hito_core::ModelKind::Linked => ModelKind::Linked,
            kind => return Err(EncodeError::UnknownModelKind(kind)),
        };
        Ok(Self {
            id: model.id().as_u128().to_be_bytes(),
            kind,
        })
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
            id: aspect.id().as_u128().to_be_bytes(),
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
        use hito_core::ElementChange as C;
        match change {
            C::Created { element, origin } => Self::Created((*origin).into(), element.into()),
            C::Modified { id, fields } => Self::Modified {
                id: id.to_bytes(),
                fields: fields
                    .iter()
                    .map(|field| ChangedField {
                        origin: field.origin.into(),
                        change: (&field.change).into(),
                    })
                    .collect(),
            },
            C::Deleted { element, origin } => Self::Deleted((*origin).into(), element.into()),
        }
    }
}

impl From<&hito_core::FieldChange> for FieldChange {
    fn from(field: &hito_core::FieldChange) -> Self {
        use hito_core::FieldChange as F;
        let id = |id: &Option<ElementId>| id.map(ElementId::to_bytes);
        let aspect = |a: &Option<hito_core::Aspect>| a.as_ref().map(Aspect::from);
        let value = |v: &Option<hito_core::Value>| v.as_ref().map(Value::from);
        match field {
            F::Property {
                name,
                before,
                after,
            } => Self::Property(name.clone(), value(before), value(after)),
            F::Type { before, after } => Self::Type(id(before), id(after)),
            F::Code { before, after } => Self::Code(
                before.as_ref().map(Code::from),
                after.as_ref().map(Code::from),
            ),
            F::Source { before, after } => Self::Source(
                before.as_ref().map(Source::from),
                after.as_ref().map(Source::from),
            ),
            F::Aspect { id, before, after } => {
                Self::Aspect(id.as_u128().to_be_bytes(), aspect(before), aspect(after))
            }
            F::AspectProperty {
                aspect,
                name,
                before,
                after,
            } => Self::AspectProperty(
                aspect.as_u128().to_be_bytes(),
                name.clone(),
                value(before),
                value(after),
            ),
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
        let (r, origin) = (change.relationship(), change.origin().into());
        match change {
            hito_core::RelationshipChange::Added { .. } => Self::Added(origin, relationship(r)),
            hito_core::RelationshipChange::Removed { .. } => Self::Removed(origin, relationship(r)),
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

fn aspect_id(id: Id) -> AspectId {
    AspectId::from_u128(u128::from_be_bytes(id))
}

impl From<Origin> for hito_core::Origin {
    fn from(origin: Origin) -> Self {
        match origin {
            Origin::User => Self::User,
            Origin::Propagated => Self::Propagated,
        }
    }
}

impl From<Aspect> for hito_core::Aspect {
    fn from(aspect: Aspect) -> Self {
        let new = Self::with_id(aspect_id(aspect.id), aspect.class.into());
        let properties = aspect.properties.into_iter();
        properties.fold(new, |aspect, (name, v)| aspect.with_property(name, v))
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
            ElementChange::Created(origin, e) => Self::Created {
                element: e.try_into()?,
                origin: origin.into(),
            },
            ElementChange::Modified { id, fields } => Self::Modified {
                id: element_id(id),
                fields: fields
                    .into_iter()
                    .map(|field| {
                        Ok(hito_core::ChangedField {
                            change: field.change.try_into()?,
                            origin: field.origin.into(),
                        })
                    })
                    .collect::<Result<_, DecodeError>>()?,
            },
            ElementChange::Deleted(origin, e) => Self::Deleted {
                element: e.try_into()?,
                origin: origin.into(),
            },
        })
    }
}

impl TryFrom<FieldChange> for hito_core::FieldChange {
    type Error = DecodeError;

    fn try_from(field: FieldChange) -> Result<Self, DecodeError> {
        let id = |id: Option<Id>| id.map(element_id);
        let code = |code: Option<Code>| code.map(TryInto::try_into).transpose();
        let value = |v: Option<Value>| v.map(Into::into);
        Ok(match field {
            FieldChange::Property(name, before, after) => Self::Property {
                name,
                before: value(before),
                after: value(after),
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
            FieldChange::Aspect(id, before, after) => Self::Aspect {
                id: aspect_id(id),
                before: before.map(Into::into),
                after: after.map(Into::into),
            },
            FieldChange::AspectProperty(aspect, name, before, after) => Self::AspectProperty {
                aspect: aspect_id(aspect),
                name,
                before: value(before),
                after: value(after),
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
            RelationshipChange::Added(origin, r) => Self::Added {
                relationship: relationship(r),
                origin: origin.into(),
            },
            RelationshipChange::Removed(origin, r) => Self::Removed {
                relationship: relationship(r),
                origin: origin.into(),
            },
        }
    }
}
