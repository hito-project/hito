//! What a committed transaction changed
//! ([element model §9](https://github.com/hito-project/hito/blob/main/docs/architecture/element-model.md#9-change-sets-one-structure-for-undo-saving-and-collaboration)).
//!
//! A [`ChangeSet`] is a record of facts, as deltas: element 42's `Height`
//! went from 3 m to 3.5 m. A creation carries the whole new element and a
//! deletion the whole deleted one, since that is their delta. It's plain
//! data, and applying it needs no domain logic, so it means the same in any
//! version of HITO. A transaction commits by applying its change set to the
//! store, and [`ElementStore::replay`](crate::ElementStore::replay) applies
//! one again later.
//!
//! Change sets are the model's history
//! ([ADR 0024](https://github.com/hito-project/hito/blob/main/docs/decisions/0024-model-history.md)).
//! The core gives them no serialised form: the storage adapter keeps its
//! own versioned record type and converts to and from it.

use crate::{
    Aspect, AspectId, Code, Element, ElementId, ExternalSource, Model, Properties, Relationship,
    StoreError, Value, Violation,
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
    /// A change set built from its parts, such as one read back from a file
    /// to [replay](crate::ElementStore::replay). It has no warnings: they
    /// are derived from the model, not facts about the change.
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

/// Who made a change: the user, or the model following the user's change.
///
/// Propagated changes are what recompute set, the elements a deletion took
/// with it, and the relationships it removed. A merge report shows what
/// someone actually did (ADR 0024).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Origin {
    User,
    Propagated,
}

/// A relationship a transaction added or removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationshipChange {
    Added {
        relationship: Relationship,
        origin: Origin,
    },
    Removed {
        relationship: Relationship,
        origin: Origin,
    },
}

impl RelationshipChange {
    pub fn relationship(&self) -> &Relationship {
        match self {
            Self::Added { relationship, .. } | Self::Removed { relationship, .. } => relationship,
        }
    }

    pub fn origin(&self) -> Origin {
        match self {
            Self::Added { origin, .. } | Self::Removed { origin, .. } => *origin,
        }
    }
}

/// What a transaction did to one element.
#[derive(Debug, Clone, PartialEq)]
pub enum ElementChange {
    Created {
        element: Element,
        origin: Origin,
    },
    /// Only the fields that differ, each from its value before the
    /// transaction to its value after, however many times it changed.
    Modified {
        id: ElementId,
        fields: Vec<ChangedField>,
    },
    Deleted {
        element: Element,
        origin: Origin,
    },
}

impl ElementChange {
    pub fn id(&self) -> ElementId {
        match self {
            Self::Created { element, .. } | Self::Deleted { element, .. } => element.id,
            Self::Modified { id, .. } => *id,
        }
    }

    /// The change between two states of an element, `None` if it's the
    /// same. `None` before means it was created, and after that it was
    /// deleted. `origin` says who changed the element as a whole, and who
    /// changed each of its fields.
    pub(crate) fn between(
        before: Option<&Element>,
        after: Option<&Element>,
        origin: Origin,
        field_origin: impl Fn(&FieldKey) -> Origin,
    ) -> Option<Self> {
        match (before, after) {
            (None, None) => None,
            (None, Some(after)) => Some(Self::Created {
                element: after.clone(),
                origin,
            }),
            (Some(before), None) => Some(Self::Deleted {
                element: before.clone(),
                origin,
            }),
            (Some(before), Some(after)) => {
                let fields: Vec<_> = FieldChange::between(before, after)
                    .into_iter()
                    .map(|change| ChangedField {
                        origin: field_origin(&change.key()),
                        change,
                    })
                    .collect();
                let id = after.id;
                (!fields.is_empty()).then_some(Self::Modified { id, fields })
            }
        }
    }
}

/// One field of a modified element, and who changed it.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangedField {
    pub change: FieldChange,
    /// [`Origin::Propagated`] only if the user didn't change the field
    /// in that transaction.
    pub origin: Origin,
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
    /// An aspect added (`None` before) or removed (`None` after), whole. An
    /// aspect's class doesn't change, so its other changes are
    /// [`AspectProperty`](Self::AspectProperty).
    Aspect {
        id: AspectId,
        before: Option<Aspect>,
        after: Option<Aspect>,
    },
    /// One property value of an aspect the element keeps; `None` means
    /// unset.
    AspectProperty {
        aspect: AspectId,
        name: String,
        before: Option<Value>,
        after: Option<Value>,
    },
}

/// Which field a [`FieldChange`] is about.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum FieldKey {
    Property(String),
    Type,
    Code,
    Source,
    Aspect(AspectId),
    AspectProperty(AspectId, String),
}

/// The property values that differ between two sets, by name.
fn property_changes<'a>(
    before: &'a Properties,
    after: &'a Properties,
) -> impl Iterator<Item = (&'a String, Option<&'a Value>, Option<&'a Value>)> {
    let mut names: Vec<_> = before.keys().chain(after.keys()).collect();
    names.sort();
    names.dedup();
    names.into_iter().filter_map(|name| {
        let (b, a) = (before.get(name), after.get(name));
        (b != a).then_some((name, b, a))
    })
}

impl FieldChange {
    fn between(before: &Element, after: &Element) -> Vec<Self> {
        debug_assert!(before.class == after.class && before.model == after.model);
        let properties = property_changes(&before.properties, &after.properties);
        let mut fields: Vec<_> = properties
            .map(|(name, b, a)| Self::Property {
                name: name.clone(),
                before: b.cloned(),
                after: a.cloned(),
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
        // Both lists are in ID order.
        let mut ids: Vec<_> = before
            .aspects
            .iter()
            .chain(&after.aspects)
            .map(Aspect::id)
            .collect();
        ids.sort();
        ids.dedup();
        for id in ids {
            let find = |e: &'_ Element| e.aspects.iter().find(|a| a.id == id).cloned();
            match (find(before), find(after)) {
                (Some(b), Some(a)) if b.class == a.class => {
                    let properties = property_changes(&b.properties, &a.properties);
                    fields.extend(properties.map(|(name, b, a)| Self::AspectProperty {
                        aspect: id,
                        name: name.clone(),
                        before: b.cloned(),
                        after: a.cloned(),
                    }));
                }
                (before, after) => fields.push(Self::Aspect { id, before, after }),
            }
        }
        fields
    }

    /// Which field this change is about.
    pub(crate) fn key(&self) -> FieldKey {
        match self {
            Self::Property { name, .. } => FieldKey::Property(name.clone()),
            Self::Type { .. } => FieldKey::Type,
            Self::Code { .. } => FieldKey::Code,
            Self::Source { .. } => FieldKey::Source,
            Self::Aspect { id, .. } => FieldKey::Aspect(*id),
            Self::AspectProperty { aspect, name, .. } => {
                FieldKey::AspectProperty(*aspect, name.clone())
            }
        }
    }

    /// Sets the field to its value after the change.
    pub(crate) fn apply(&self, element: &mut Element) {
        let set = |properties: &mut Properties, name: &String, value: &Option<Value>| {
            match value {
                Some(value) => properties.insert(name.clone(), value.clone()),
                None => properties.remove(name),
            };
        };
        match self {
            Self::Property { name, after, .. } => set(&mut element.properties, name, after),
            Self::Type { after, .. } => element.type_id = *after,
            Self::Code { after, .. } => element.code = after.clone(),
            Self::Source { after, .. } => element.source = after.clone(),
            Self::Aspect { id, after, .. } => {
                element.aspects.retain(|a| a.id != *id);
                if let Some(aspect) = after {
                    element.add_aspect(aspect.clone());
                }
            }
            Self::AspectProperty {
                aspect,
                name,
                after,
                ..
            } => {
                // Replay checks the aspect exists, unless the change set
                // changes the element twice.
                let aspect = element.aspects.iter_mut().find(|a| a.id == *aspect);
                if let Some(aspect) = aspect {
                    set(&mut aspect.properties, name, after);
                }
            }
        }
    }

    /// Whether the change fits the element as it was before: an aspect
    /// added must be new, and one removed or changed must exist.
    pub(crate) fn check(&self, element: &Element) -> Result<(), StoreError> {
        let has = |id: &AspectId| element.aspects.iter().any(|a| a.id == *id);
        match self {
            Self::Aspect {
                before: None,
                after: Some(aspect),
                ..
            } if has(&aspect.id) => Err(StoreError::DuplicateAspect(aspect.class.clone())),
            Self::Aspect {
                id,
                before: Some(_),
                ..
            }
            | Self::AspectProperty { aspect: id, .. }
                if !has(id) =>
            {
                Err(StoreError::UnknownAspect(*id))
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClassId, ModelId};

    #[test]
    fn aspects_change_one_by_one_and_property_by_property() {
        let note =
            |text: &str| Aspect::new(ClassId::new("Test", "Note")).with_property("Text", text);
        let (kept, removed, added) = (note("a"), note("b"), note("c"));
        let class = ClassId::new("Test", "Column");
        let before = Element::new(class, ModelId::generate())
            .with_aspect(kept.clone())
            .with_aspect(removed.clone());
        let mut after = before.clone();
        after.aspects.retain(|a| a.id != removed.id);
        after.aspects[0]
            .properties
            .insert("Text".into(), "a2".into());
        after.add_aspect(added.clone());

        let fields = FieldChange::between(&before, &after);
        assert_eq!(
            fields,
            [
                FieldChange::AspectProperty {
                    aspect: kept.id,
                    name: "Text".into(),
                    before: Some("a".into()),
                    after: Some("a2".into()),
                },
                FieldChange::Aspect {
                    id: removed.id,
                    before: Some(removed),
                    after: None,
                },
                FieldChange::Aspect {
                    id: added.id,
                    before: None,
                    after: Some(added),
                },
            ]
        );
        let mut replayed = before.clone();
        fields.iter().for_each(|f| f.apply(&mut replayed));
        assert_eq!(replayed, after);
    }
}
