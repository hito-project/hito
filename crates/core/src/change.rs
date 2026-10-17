//! What a committed transaction changed
//! ([element model §9](https://github.com/hito-project/hito/blob/main/docs/architecture/element-model.md#9-change-sets-one-structure-for-undo-saving-and-collaboration)).
//!
//! A [`ChangeSet`] is a record of facts, as deltas: element 42's `Height`
//! went from 3 m to 3.5 m. A creation carries the whole new element and a
//! deletion the whole deleted one, since that is their delta. It's plain
//! data, and applying it needs no domain logic, so it means the same in any
//! version of HITO. A transaction commits by applying its change set to the
//! store. Undo and redo (#19) and change sets (#21) are built on it. Its
//! serialised form is decided by the history spike (#156).

use crate::{
    Aspect, Code, Element, ElementId, ExternalSource, Model, Properties, Relationship, Value,
    Violation,
};

/// The user command a transaction carries out, as plain data: a name and
/// parameters. The UI shows it in the undo list (#19).
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    name: String,
    parameters: Properties,
}

impl Command {
    /// A command called `name`, with no parameters yet.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parameters: Properties::new(),
        }
    }

    pub fn with_parameter(mut self, name: impl Into<String>, value: impl Into<Value>) -> Self {
        self.parameters.insert(name.into(), value.into());
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn parameters(&self) -> &Properties {
        &self.parameters
    }
}

/// Everything one transaction changed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChangeSet {
    pub(crate) command: Option<Command>,
    pub(crate) models: Vec<Model>,
    pub(crate) elements: Vec<ElementChange>,
    pub(crate) relationships: Vec<RelationshipChange>,
    pub(crate) warnings: Vec<Violation>,
}

impl ChangeSet {
    /// A change set read back from storage or received from elsewhere, to
    /// [replay](crate::ElementStore::replay). Warnings aren't part of it:
    /// they're derived, not facts.
    pub fn from_parts(
        command: Option<Command>,
        models: Vec<Model>,
        elements: Vec<ElementChange>,
        relationships: Vec<RelationshipChange>,
    ) -> Self {
        Self {
            command,
            models,
            elements,
            relationships,
            warnings: Vec::new(),
        }
    }

    /// The change set that undoes this one: creations become deletions and
    /// back, each field goes from its after value to its before value, in
    /// reverse order. Spike #156: models can't be removed yet, so added
    /// models stay.
    pub fn inverse(&self) -> Self {
        let elements = self.elements.iter().rev().map(ElementChange::inverse);
        let relationships = self.relationships.iter().rev().map(|r| match r {
            RelationshipChange::Added(r) => RelationshipChange::Removed(r.clone()),
            RelationshipChange::Removed(r) => RelationshipChange::Added(r.clone()),
        });
        Self {
            command: self.command.clone(),
            models: Vec::new(),
            elements: elements.collect(),
            relationships: relationships.collect(),
            warnings: Vec::new(),
        }
    }

    /// The command the transaction carried out, if it was given one.
    pub fn command(&self) -> Option<&Command> {
        self.command.as_ref()
    }

    /// The models the transaction added.
    pub fn models(&self) -> &[Model] {
        &self.models
    }

    /// The elements the transaction created, changed or deleted, in the
    /// order it first changed them. An element whose changes cancel out
    /// isn't included.
    pub fn elements(&self) -> &[ElementChange] {
        &self.elements
    }

    /// The relationships the transaction added or removed, in the order it
    /// first changed them. Deleting an element removes its relationships.
    pub fn relationships(&self) -> &[RelationshipChange] {
        &self.relationships
    }

    /// The [warnings](crate::Severity::Warning) the changed elements raised.
    /// They didn't block the commit; the UI shows them.
    pub fn warnings(&self) -> &[Violation] {
        &self.warnings
    }
}

/// A relationship a transaction added or removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationshipChange {
    Added(Relationship),
    Removed(Relationship),
}

/// What a transaction did to one element.
#[derive(Debug, Clone, PartialEq)]
pub enum ElementChange {
    Created(Element),
    /// Only the fields that differ, each from its value before the
    /// transaction to its value after, however many times it changed.
    Modified {
        id: ElementId,
        fields: Vec<FieldChange>,
    },
    Deleted(Element),
}

impl ElementChange {
    pub fn id(&self) -> ElementId {
        match self {
            Self::Created(element) | Self::Deleted(element) => element.id,
            Self::Modified { id, .. } => *id,
        }
    }

    fn inverse(&self) -> Self {
        match self {
            Self::Created(element) => Self::Deleted(element.clone()),
            Self::Deleted(element) => Self::Created(element.clone()),
            Self::Modified { id, fields } => Self::Modified {
                id: *id,
                fields: fields.iter().rev().map(FieldChange::inverse).collect(),
            },
        }
    }

    /// The change between two states of an element, `None` if it's the
    /// same. `None` before means it was created, and after that it was
    /// deleted.
    pub(crate) fn between(before: Option<&Element>, after: Option<&Element>) -> Option<Self> {
        match (before, after) {
            (None, None) => None,
            (None, Some(after)) => Some(Self::Created(after.clone())),
            (Some(before), None) => Some(Self::Deleted(before.clone())),
            (Some(before), Some(after)) => {
                let fields = FieldChange::between(before, after);
                let id = after.id;
                (!fields.is_empty()).then_some(Self::Modified { id, fields })
            }
        }
    }
}

/// One field of an element, before and after a transaction. The class and
/// model of an element never change.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldChange {
    /// One of the element's own property values; `None` means unset.
    Property {
        name: String,
        before: Option<Value>,
        after: Option<Value>,
    },
    Type {
        before: Option<ElementId>,
        after: Option<ElementId>,
    },
    Code {
        before: Option<Code>,
        after: Option<Code>,
    },
    Source {
        before: Option<ExternalSource>,
        after: Option<ExternalSource>,
    },
    /// Aspects have no identity of their own, so they're recorded as the
    /// whole list.
    Aspects {
        before: Vec<Aspect>,
        after: Vec<Aspect>,
    },
}

impl FieldChange {
    fn between(before: &Element, after: &Element) -> Vec<Self> {
        debug_assert!(before.class == after.class && before.model == after.model);
        let mut names: Vec<_> = before.properties.keys().collect();
        names.extend(after.properties.keys());
        names.sort();
        names.dedup();
        let mut fields: Vec<_> = names
            .into_iter()
            .filter_map(|name| {
                let (b, a) = (before.properties.get(name), after.properties.get(name));
                (b != a).then(|| Self::Property {
                    name: name.clone(),
                    before: b.cloned(),
                    after: a.cloned(),
                })
            })
            .collect();
        if before.type_id != after.type_id {
            let (before, after) = (before.type_id, after.type_id);
            fields.push(Self::Type { before, after });
        }
        if before.code != after.code {
            let (before, after) = (before.code.clone(), after.code.clone());
            fields.push(Self::Code { before, after });
        }
        if before.source != after.source {
            let (before, after) = (before.source.clone(), after.source.clone());
            fields.push(Self::Source { before, after });
        }
        if before.aspects != after.aspects {
            let (before, after) = (before.aspects.clone(), after.aspects.clone());
            fields.push(Self::Aspects { before, after });
        }
        fields
    }

    fn inverse(&self) -> Self {
        match self.clone() {
            Self::Property {
                name,
                before,
                after,
            } => Self::Property {
                name,
                before: after,
                after: before,
            },
            Self::Type { before, after } => Self::Type {
                before: after,
                after: before,
            },
            Self::Code { before, after } => Self::Code {
                before: after,
                after: before,
            },
            Self::Source { before, after } => Self::Source {
                before: after,
                after: before,
            },
            Self::Aspects { before, after } => Self::Aspects {
                before: after,
                after: before,
            },
        }
    }

    /// Sets the field to its value after the change.
    pub(crate) fn apply(&self, element: &mut Element) {
        match self {
            Self::Property {
                name,
                after: Some(value),
                ..
            } => {
                element.properties.insert(name.clone(), value.clone());
            }
            Self::Property {
                name, after: None, ..
            } => {
                element.properties.remove(name);
            }
            Self::Type { after, .. } => element.type_id = *after,
            Self::Code { after, .. } => element.code = after.clone(),
            Self::Source { after, .. } => element.source = after.clone(),
            Self::Aspects { after, .. } => element.aspects = after.clone(),
        }
    }
}
