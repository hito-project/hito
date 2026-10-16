//! Relationships between elements
//! ([element model §5](https://github.com/hito-project/hito/blob/main/docs/architecture/element-model.md#5-relationships-three-kinds-plus-drives)).
//!
//! A domain declares its relationship types in its [`Schema`](crate::Schema),
//! next to its classes. A relationship links a source element to a target
//! element, and reads from source to target: a column is *attached to* a
//! level, a beam *frames into* a column, an opening is *hosted by* a slab.
//! The source depends on the target, so the type says two things about the
//! target:
//!
//! - whether a change to it recomputes the source ([`RelationshipDef::driving`])
//! - what happens to the source when it's deleted ([`OnDelete`])

use crate::{ClassId, ElementId};

/// What deleting a relationship's target does to its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OnDelete {
    /// The source is deleted too: the target owns it, as a slab owns its
    /// openings.
    Cascade,
    /// The relationship is removed and the source stays.
    Detach,
    /// The target can't be deleted while the source refers to it.
    Block,
}

/// A relationship type as a schema declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationshipDef {
    pub(crate) name: String,
    source: ClassId,
    target: ClassId,
    drives: bool,
    on_delete: OnDelete,
}

impl RelationshipDef {
    /// A relationship type called `name`, from elements of class `source` to
    /// elements of class `target` (or their subclasses). It doesn't drive,
    /// and deleting the target detaches the source.
    pub fn new(name: impl Into<String>, source: ClassId, target: ClassId) -> Self {
        Self {
            name: name.into(),
            source,
            target,
            drives: false,
            on_delete: OnDelete::Detach,
        }
    }

    /// Makes the target drive the source: when a transaction changes the
    /// target, the source is recomputed before the transaction commits.
    pub fn driving(mut self) -> Self {
        self.drives = true;
        self
    }

    pub fn on_delete(mut self, rule: OnDelete) -> Self {
        self.on_delete = rule;
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn source(&self) -> &ClassId {
        &self.source
    }

    pub fn target(&self) -> &ClassId {
        &self.target
    }

    /// Whether the target drives the source.
    pub fn drives(&self) -> bool {
        self.drives
    }

    pub fn delete_rule(&self) -> OnDelete {
        self.on_delete
    }
}

/// A relationship of type `class` from `source` to `target`.
///
/// An element can have any number of relationships, but only one of each
/// type to the same target.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Relationship {
    class: ClassId,
    source: ElementId,
    target: ElementId,
}

impl Relationship {
    pub fn new(class: ClassId, source: ElementId, target: ElementId) -> Self {
        Self {
            class,
            source,
            target,
        }
    }

    pub fn class(&self) -> &ClassId {
        &self.class
    }

    pub fn source(&self) -> ElementId {
        self.source
    }

    pub fn target(&self) -> ElementId {
        self.target
    }
}
