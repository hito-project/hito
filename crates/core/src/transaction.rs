//! Transactions: the only way to change an [`ElementStore`].
//!
//! A transaction changes the store in place and keeps the state of every
//! element and model from before its first change. Committing runs the
//! domains' validation [rules](crate::Rule) on what changed. If one fails, or
//! the transaction is dropped without committing, even by a panic, the store
//! goes back to that state.
//!
//! The commit is the single point where a change becomes part of the model,
//! so the next E04 and E03 stories hook in there: recomputing dependent
//! elements (#16) before the rules run, and turning the recorded states into
//! a change set (#21) that undo and redo (#19) replay.

use std::collections::HashSet;
use std::mem;
use std::ops::Deref;

use crate::{Code, Element, ElementId, ElementStore, Model, ModelId, StoreError, Value, Violation};

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
/// tx.commit()?;
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
    pub(crate) fn new(store: &'s mut ElementStore) -> Self {
        Self {
            store,
            journal: Journal::default(),
        }
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
    /// rules for its class, and keeps the changes if none is broken.
    /// Otherwise rolls them all back and returns every
    /// [violation](StoreError::Violations).
    pub fn commit(mut self) -> Result<(), StoreError> {
        let violations = self.validate();
        if !violations.is_empty() {
            return Err(StoreError::Violations(violations));
        }
        self.journal = Journal::default();
        Ok(())
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
