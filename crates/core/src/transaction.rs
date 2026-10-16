//! Transactions: the only way to change an [`ElementStore`].
//!
//! A transaction stages its changes beside the store and leaves the store
//! untouched until it commits. Committing first [recomputes](crate::Recompute)
//! what changed and every element that depends on it, then runs the domains'
//! validation [rules](crate::Rule) on all of those, then turns the staged
//! changes into a [`ChangeSet`] of deltas and applies it. If a rule that is an
//! error fails, or the transaction is dropped, panics or is leaked with
//! [`std::mem::forget`], the staged changes are simply never applied.
//!
//! The commit is the single point where a change becomes part of the model,
//! and the change set it returns is what undo and redo (#19) and change sets
//! (#21) are built on.

use std::collections::BTreeSet;
use std::mem;

use crate::change::{ElementChange, RelationshipChange};
use crate::view::Staged;
use crate::{
    ChangeSet, Code, Command, Element, ElementId, ElementStore, Model, ModelId, Relationship,
    Severity, StoreError, StoreView, Value, Violation,
};

/// A group of changes to an [`ElementStore`] that apply together or not at
/// all. Start one with [`ElementStore::transaction`].
///
/// While it's open, the transaction is the only access to the store. Its
/// [view](Transaction::view) shows the store with its changes:
///
/// ```
/// # use hito_core::{ClassId, Element, ElementStore, Model, ModelKind, SchemaRegistry, StoreError};
/// # fn main() -> Result<(), StoreError> {
/// let mut store = ElementStore::new(SchemaRegistry::new());
/// let mut tx = store.transaction();
/// let model = tx.insert_model(Model::new(ModelKind::Physical))?;
/// let element = tx.insert(Element::new(ClassId::GEOMETRIC_ELEMENT_3D, model))?;
/// assert!(tx.view().get(element).is_some());
/// let changes = tx.commit()?;
/// assert_eq!(changes.elements()[0].id(), element);
/// assert!(store.get(element).is_some());
/// # Ok(())
/// # }
/// ```
///
/// A step that fails changes nothing, and leaves the transaction open: the
/// caller decides whether to go on or give up. Returning the error with `?`
/// drops the transaction, which discards all of it.
#[must_use = "a transaction is discarded unless it's committed"]
#[derive(Debug)]
pub struct Transaction<'s> {
    store: &'s mut ElementStore,
    command: Option<Command>,
    staged: Staged,
}

impl<'s> Transaction<'s> {
    pub(crate) fn new(store: &'s mut ElementStore, command: Option<Command>) -> Self {
        Self {
            store,
            command,
            staged: Staged::default(),
        }
    }

    /// The store as this transaction would leave it.
    pub fn view(&self) -> StoreView<'_> {
        StoreView::new(self.store, Some(&self.staged))
    }

    /// The command the transaction carries out, if it was given one.
    pub fn command(&self) -> Option<&Command> {
        self.command.as_ref()
    }

    /// Adds a model.
    pub fn insert_model(&mut self, model: Model) -> Result<ModelId, StoreError> {
        let id = model.id();
        if self.view().model(id).is_some() {
            return Err(StoreError::DuplicateModel(id));
        }
        self.staged.models.push(model);
        Ok(id)
    }

    /// Adds an element, after checking it against its class:
    ///
    /// - its ID is new, its model exists and its class is a registered
    ///   element class
    /// - each property value is declared by the class and has the right data
    ///   type
    /// - its type, if any, is an element of the class's type class
    /// - its aspects are of registered aspect classes, with at most one of
    ///   each unique aspect class
    /// - its code, if any, isn't used by another element
    pub fn insert(&mut self, element: Element) -> Result<ElementId, StoreError> {
        self.view().check_new(&element)?;
        let id = element.id;
        if let Some(code) = &element.code {
            self.staged.codes.insert(code.clone(), Some(id));
        }
        if let Some(type_id) = element.type_id {
            self.staged.instances.entry(type_id).or_default().push(id);
        }
        if self.staged.elements.insert(id, Some(element)).is_none() {
            self.staged.order.push(id);
        }
        Ok(id)
    }

    /// Sets one of an element's own property values. On a type, the new
    /// value applies to every instance that doesn't override it.
    pub fn set_property(
        &mut self,
        id: ElementId,
        name: &str,
        value: impl Into<Value>,
    ) -> Result<(), StoreError> {
        let value = value.into();
        let element = self.view().get(id).ok_or(StoreError::UnknownElement(id))?;
        self.view().check_property(&element.class, name, &value)?;
        self.stage(id).properties.insert(name.to_owned(), value);
        Ok(())
    }

    /// Removes one of an element's own property values, so its type's value
    /// applies again. Returns the removed value.
    pub fn clear_property(
        &mut self,
        id: ElementId,
        name: &str,
    ) -> Result<Option<Value>, StoreError> {
        let element = self.view().get(id).ok_or(StoreError::UnknownElement(id))?;
        if !element.properties.contains_key(name) {
            return Ok(None);
        }
        Ok(self.stage(id).properties.remove(name))
    }

    /// Gives an element a new code, or removes it with `None`. The element
    /// keeps its ID.
    pub fn set_code(&mut self, id: ElementId, code: Option<Code>) -> Result<(), StoreError> {
        if self.view().get(id).is_none() {
            return Err(StoreError::UnknownElement(id));
        }
        if let Some(code) = &code {
            self.view().check_code_free(code, id)?;
        }
        let element = self.stage(id);
        let old = mem::replace(&mut element.code, code.clone());
        if let Some(old) = old {
            self.staged.codes.insert(old, None);
        }
        if let Some(code) = code {
            self.staged.codes.insert(code, Some(id));
        }
        Ok(())
    }

    /// Adds a relationship. See [`OnDelete`](crate::OnDelete) for what
    /// happens when its target is deleted.
    ///
    /// A driving relationship can't close a cycle: if the target already
    /// depends on the source, directly or not, this fails with the
    /// [cycle](StoreError::CircularDependency).
    pub fn relate(&mut self, relationship: Relationship) -> Result<(), StoreError> {
        self.view().check_relationship(&relationship)?;
        let in_store = self.store.has_relationship(&relationship);
        self.staged.set_relationship(relationship, true, in_store);
        Ok(())
    }

    /// Removes a relationship.
    pub fn unrelate(&mut self, relationship: &Relationship) -> Result<(), StoreError> {
        if !self.view().has_relationship(relationship) {
            return Err(StoreError::MissingRelationship(relationship.clone()));
        }
        let in_store = self.store.has_relationship(relationship);
        self.staged
            .set_relationship(relationship.clone(), false, in_store);
        Ok(())
    }

    /// Deletes an element and everything
    /// [`preview_delete`](StoreView::preview_delete) lists with it: the
    /// instances of a deleted type, and the elements a deleted element owns
    /// through cascading relationships. Elements that refer to a deleted
    /// element through detaching relationships stay and lose the
    /// relationship. A blocking relationship refuses the whole deletion.
    ///
    /// Returns every deleted element, the given one first. On failure,
    /// nothing is deleted.
    pub fn delete(&mut self, id: ElementId) -> Result<Vec<ElementId>, StoreError> {
        let deleted = self.view().preview_delete(id)?;
        let view = self.view();
        let relationships: BTreeSet<_> = deleted
            .iter()
            .flat_map(|&e| view.relationships_from(e).chain(view.relationships_to(e)))
            .cloned()
            .collect();
        for relationship in &relationships {
            self.unrelate(relationship)?;
        }
        for &element in &deleted {
            let Staged {
                elements,
                order,
                codes,
                ..
            } = &mut self.staged;
            let staged = elements.entry(element).or_insert_with(|| {
                order.push(element);
                self.store.elements.get(&element).cloned()
            });
            if let Some(code) = staged.take().and_then(|e| e.code) {
                codes.insert(code, None);
            }
        }
        Ok(deleted)
    }

    /// Recomputes what changed and its dependants, then checks them against
    /// the rules for their classes. If no error rule is broken, applies the
    /// changes to the store and returns them, with any warnings. Otherwise
    /// discards them and returns the error, or every error
    /// [violation](StoreError::Violations).
    pub fn commit(mut self) -> Result<ChangeSet, StoreError> {
        let affected = self.recompute()?;
        let (errors, warnings) = self
            .validate(&affected)
            .into_iter()
            .partition::<Vec<_>, _>(|v| v.severity == Severity::Error);
        if !errors.is_empty() {
            return Err(StoreError::Violations(errors));
        }
        let staged = mem::take(&mut self.staged);
        let store = &*self.store;
        let elements = staged.order.iter().filter_map(|id| {
            let before = store.elements.get(id);
            ElementChange::between(before, staged.elements[id].as_ref())
        });
        let relationships = staged.relationship_order.iter().filter_map(|r| {
            let exists = staged.relationships[r];
            (exists != store.has_relationship(r)).then(|| match exists {
                true => RelationshipChange::Added(r.clone()),
                false => RelationshipChange::Removed(r.clone()),
            })
        });
        let changes = ChangeSet {
            command: self.command.take(),
            elements: elements.collect(),
            relationships: relationships.collect(),
            warnings,
            models: staged.models,
        };
        self.store.apply(&changes);
        Ok(changes)
    }

    /// Discards every change. Dropping the transaction does the same.
    pub fn roll_back(self) {}

    /// The element's staged state, copied from the store on its first
    /// change. The element must exist.
    fn stage(&mut self, id: ElementId) -> &mut Element {
        let Staged {
            elements, order, ..
        } = &mut self.staged;
        let staged = elements.entry(id).or_insert_with(|| {
            order.push(id);
            self.store.elements.get(&id).cloned()
        });
        staged.as_mut().expect("checked by the caller")
    }

    /// Recomputes every element the transaction changed or whose
    /// relationships it changed, and every element they drive, drivers
    /// first. Returns all of them, in that order.
    fn recompute(&mut self) -> Result<Vec<ElementId>, StoreError> {
        let staged = &self.staged;
        let elements = staged.order.iter().copied();
        let sources = staged.relationship_order.iter().map(|r| r.source());
        let changed: Vec<_> = elements.chain(sources).collect();
        let order = self.view().recompute_order(changed)?;
        for &id in &order {
            let view = self.view();
            let Some(element) = view.get(id) else {
                continue; // deleted
            };
            let recomputes = view.registry().recomputes(element.class());
            let values: Vec<_> = recomputes
                .flat_map(|r| r.compute(&view, element))
                .filter(|(name, value)| element.properties().get(name) != Some(value))
                .collect();
            for (name, value) in values {
                self.set_property(id, &name, value)?;
            }
        }
        Ok(order)
    }

    fn validate(&self, elements: &[ElementId]) -> Vec<Violation> {
        let view = self.view();
        elements
            .iter()
            .filter_map(|&id| view.get(id))
            .flat_map(|element| {
                let rules = view.registry().rules(element.class());
                rules.filter_map(move |rule| rule.check(&view, element).err())
            })
            .collect()
    }
}
