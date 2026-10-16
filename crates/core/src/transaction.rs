//! Transactions: the only way to change an [`ElementStore`].
//!
//! A transaction changes the store in place and keeps the state of every
//! element and model from before its first change. Committing runs the
//! domains' validation [rules](crate::Rule) on what changed. If one fails, or
//! the transaction is dropped without committing, even by a panic, the store
//! goes back to that state.
//!
//! The commit is the single point where a change becomes part of the model.
//! It returns a [`ChangeSet`]: each changed element before and after. The
//! next stories hook in there: recomputing dependent elements (#16) before
//! the rules run, and undo and redo (#19) and change sets (#21) built on the
//! returned record.

use std::collections::HashSet;
use std::mem;
use std::ops::Deref;

use crate::change::ElementChange;
use crate::{
    ChangeSet, Code, Command, Element, ElementId, ElementStore, Model, ModelId, StoreError, Value,
    Violation,
};

/// A group of changes to an [`ElementStore`] that apply together or not at
/// all. Start one with [`ElementStore::transaction`].
///
/// While it's open, the transaction is the only access to the store. It reads
/// like the store, through [`Deref`], and sees its own changes:
///
/// ```
/// # use hito_core::{ClassId, Element, ElementStore, Model, ModelKind, SchemaRegistry, StoreError};
/// # fn main() -> Result<(), StoreError> {
/// let mut store = ElementStore::new(SchemaRegistry::new());
/// let mut tx = store.transaction();
/// let model = tx.insert_model(Model::new(ModelKind::Physical))?;
/// let element = tx.insert(Element::new(ClassId::GEOMETRIC_ELEMENT_3D, model))?;
/// assert!(tx.get(element).is_some());
/// let changes = tx.commit()?;
/// assert_eq!(changes.elements()[0].id(), element);
/// # Ok(())
/// # }
/// ```
///
/// A step that fails changes nothing, and leaves the transaction open: the
/// caller decides whether to go on or give up. Returning the error with `?`
/// drops the transaction, which rolls all of it back.
#[must_use = "a transaction is rolled back unless it's committed"]
#[derive(Debug)]
pub struct Transaction<'s> {
    store: &'s mut ElementStore,
    command: Option<Command>,
    journal: Journal,
}

/// What a transaction changed, with the state from before.
#[derive(Debug, Default)]
struct Journal {
    models: Vec<ModelId>,
    /// Each changed element and its state before the transaction, `None` if
    /// the transaction created it. In the order they were first changed.
    elements: Vec<(ElementId, Option<Element>)>,
    changed: HashSet<ElementId>,
}

impl<'s> Transaction<'s> {
    pub(crate) fn new(store: &'s mut ElementStore, command: Option<Command>) -> Self {
        Self {
            store,
            command,
            journal: Journal::default(),
        }
    }

    /// The command the transaction carries out, if it was given one.
    pub fn command(&self) -> Option<&Command> {
        self.command.as_ref()
    }

    /// Adds a model.
    pub fn insert_model(&mut self, model: Model) -> Result<ModelId, StoreError> {
        let id = self.store.insert_model(model)?;
        self.journal.models.push(id);
        Ok(id)
    }

    /// Adds an element, after checking it against its class:
    ///
    /// - its model exists and its class is a registered element class
    /// - each property value is declared by the class and has the right data
    ///   type
    /// - its type, if any, is a stored element of the class's type class
    /// - its aspects are of registered aspect classes, with at most one of
    ///   each unique aspect class
    /// - its code, if any, isn't used by another element
    pub fn insert(&mut self, element: Element) -> Result<ElementId, StoreError> {
        self.change(element.id(), |store| store.insert(element))
    }

    /// Sets one of an element's own property values. On a type, the new
    /// value applies to every instance that doesn't override it.
    pub fn set_property(
        &mut self,
        id: ElementId,
        name: &str,
        value: impl Into<Value>,
    ) -> Result<(), StoreError> {
        self.change(id, |store| store.set_property(id, name, value))
    }

    /// Removes one of an element's own property values, so its type's value
    /// applies again. Returns the removed value.
    pub fn clear_property(
        &mut self,
        id: ElementId,
        name: &str,
    ) -> Result<Option<Value>, StoreError> {
        self.change(id, |store| store.clear_property(id, name))
    }

    /// Gives an element a new code, or removes it with `None`. The element
    /// keeps its ID.
    pub fn set_code(&mut self, id: ElementId, code: Option<Code>) -> Result<(), StoreError> {
        self.change(id, |store| store.set_code(id, code))
    }

    /// Checks every element the transaction created or changed against the
    /// rules for its class, and keeps the changes if none is broken. Returns
    /// what changed. Otherwise rolls them all back and returns every
    /// [violation](StoreError::Violations).
    pub fn commit(mut self) -> Result<ChangeSet, StoreError> {
        let violations = self.validate();
        if !violations.is_empty() {
            return Err(StoreError::Violations(violations));
        }
        let journal = mem::take(&mut self.journal);
        let store = &*self.store;
        let models = journal.models.iter();
        let elements = journal.elements.into_iter().map(|(id, before)| {
            let after = store.get(id).cloned();
            ElementChange { id, before, after }
        });
        Ok(ChangeSet {
            command: self.command.take(),
            models: models.filter_map(|&id| store.model(id)).cloned().collect(),
            elements: elements.filter(|c| c.before != c.after).collect(),
        })
    }

    /// Undoes every change. Dropping the transaction does the same.
    pub fn roll_back(self) {
        drop(self);
    }

    /// Applies one change to an element, first recording its state if this
    /// is the transaction's first change to it.
    fn change<T>(
        &mut self,
        id: ElementId,
        apply: impl FnOnce(&mut ElementStore) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let first = self.journal.changed.insert(id);
        if first {
            let before = self.store.get(id).cloned();
            self.journal.elements.push((id, before));
        }
        let result = apply(self.store);
        // A failed change leaves the element as it was.
        if result.is_err() && first {
            self.journal.elements.pop();
            self.journal.changed.remove(&id);
        }
        result
    }

    fn validate(&self) -> Vec<Violation> {
        let store = &*self.store;
        let changed = self.journal.elements.iter();
        changed
            .filter_map(|(id, _)| store.get(*id))
            .flat_map(|element| {
                let rules = store.registry().rules(element.class());
                rules.filter_map(move |rule| rule.check(store, element).err())
            })
            .collect()
    }
}

impl Deref for Transaction<'_> {
    type Target = ElementStore;

    fn deref(&self) -> &ElementStore {
        self.store
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        let journal = mem::take(&mut self.journal);
        self.store.restore(journal.elements, &journal.models);
    }
}
