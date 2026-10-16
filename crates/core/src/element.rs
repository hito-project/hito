//! Elements, their aspects, and the models that contain them
//! ([element model §2 and §4](https://github.com/hito-project/hito/blob/main/docs/architecture/element-model.md#2-models-one-container-concept-from-the-start)).

use std::collections::BTreeMap;

use crate::{ClassId, Code, ElementId, ExternalSource, Value};

/// The identity of a [`Model`]. Like an [`ElementId`], it is a 128-bit UUID
/// generated locally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModelId(ElementId);

impl ModelId {
    /// Generates a new, globally unique ID.
    pub fn generate() -> Self {
        Self(ElementId::generate())
    }

    /// The model ID with this 128-bit value.
    pub const fn from_u128(value: u128) -> Self {
        Self(ElementId::from_u128(value))
    }

    /// The ID's 128-bit value.
    pub const fn as_u128(self) -> u128 {
        self.0.as_u128()
    }
}

/// The modelling perspective a model holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ModelKind {
    /// The physical building or work, such as the structure.
    Physical,
    /// Types, materials and profiles.
    Definition,
    /// 2D content of a view or sheet.
    Drawing,
    /// The analytical model.
    Analytical,
    /// Read-only content from an external file.
    Linked,
}

/// A container of elements. Every element lives in exactly one model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    id: ModelId,
    kind: ModelKind,
}

impl Model {
    /// A new model with a new ID.
    pub fn new(kind: ModelKind) -> Self {
        Self::with_id(ModelId::generate(), kind)
    }

    /// A model with an existing ID, such as one read from a file.
    pub fn with_id(id: ModelId, kind: ModelKind) -> Self {
        Self { id, kind }
    }

    pub fn id(&self) -> ModelId {
        self.id
    }

    pub fn kind(&self) -> ModelKind {
        self.kind
    }
}

/// Property values by property name.
pub type Properties = BTreeMap<String, Value>;

/// A group of properties of an aspect class, owned by one element.
///
/// Aspects let one domain add data to another domain's elements, such as
/// structural properties on an architectural wall.
#[derive(Debug, Clone, PartialEq)]
pub struct Aspect {
    class: ClassId,
    properties: Properties,
}

impl Aspect {
    /// An aspect of class `class`, with no property values yet.
    pub fn new(class: ClassId) -> Self {
        Self {
            class,
            properties: Properties::new(),
        }
    }

    /// Sets a property value.
    pub fn with_property(mut self, name: impl Into<String>, value: impl Into<Value>) -> Self {
        self.properties.insert(name.into(), value.into());
        self
    }

    pub fn class(&self) -> &ClassId {
        &self.class
    }

    pub fn properties(&self) -> &Properties {
        &self.properties
    }
}

/// An element: an instance of an element class, in a model.
///
/// Build one with [`Element::new`] and the `with_` methods, then add it to an
/// [`ElementStore`](crate::ElementStore) through a
/// [`Transaction`](crate::Transaction), which checks it against the schemas.
/// Once stored, it changes only through transactions.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub(crate) id: ElementId,
    pub(crate) class: ClassId,
    pub(crate) model: ModelId,
    pub(crate) type_id: Option<ElementId>,
    pub(crate) code: Option<Code>,
    pub(crate) source: Option<ExternalSource>,
    pub(crate) properties: Properties,
    pub(crate) aspects: Vec<Aspect>,
}

impl Element {
    /// A new element of class `class` in `model`, with a new ID.
    pub fn new(class: ClassId, model: ModelId) -> Self {
        Self::with_id(ElementId::generate(), class, model)
    }

    /// An element with an existing ID, such as one read from a file.
    pub fn with_id(id: ElementId, class: ClassId, model: ModelId) -> Self {
        Self {
            id,
            class,
            model,
            type_id: None,
            code: None,
            source: None,
            properties: Properties::new(),
            aspects: Vec::new(),
        }
    }

    /// Sets the element's type: a definition element whose property values
    /// this element shares.
    pub fn with_type(mut self, type_id: ElementId) -> Self {
        self.type_id = Some(type_id);
        self
    }

    pub fn with_code(mut self, code: Code) -> Self {
        self.code = Some(code);
        self
    }

    pub fn with_source(mut self, source: ExternalSource) -> Self {
        self.source = Some(source);
        self
    }

    /// Sets a property value. On an instance, this overrides the type's
    /// value of the same name.
    pub fn with_property(mut self, name: impl Into<String>, value: impl Into<Value>) -> Self {
        self.properties.insert(name.into(), value.into());
        self
    }

    pub fn with_aspect(mut self, aspect: Aspect) -> Self {
        self.aspects.push(aspect);
        self
    }

    pub fn id(&self) -> ElementId {
        self.id
    }

    pub fn class(&self) -> &ClassId {
        &self.class
    }

    pub fn model(&self) -> ModelId {
        self.model
    }

    /// The element's type, if it has one.
    pub fn type_id(&self) -> Option<ElementId> {
        self.type_id
    }

    pub fn code(&self) -> Option<&Code> {
        self.code.as_ref()
    }

    /// Where the element was imported or linked from, if anywhere.
    pub fn source(&self) -> Option<&ExternalSource> {
        self.source.as_ref()
    }

    /// The element's own property values, without its type's. See
    /// [`ElementStore::property`](crate::ElementStore::property) for the
    /// value that applies.
    pub fn properties(&self) -> &Properties {
        &self.properties
    }

    /// All the element's aspects.
    pub fn aspects(&self) -> &[Aspect] {
        &self.aspects
    }

    /// The element's aspects of class `class` exactly: at most one for a
    /// unique aspect class.
    pub fn aspects_of<'a>(&'a self, class: &'a ClassId) -> impl Iterator<Item = &'a Aspect> {
        self.aspects.iter().filter(move |a| &a.class == class)
    }
}
