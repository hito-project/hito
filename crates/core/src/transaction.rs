//! Transactions: the only way to change an [`ElementStore`].
//!
//! A transaction stages its changes beside the store and leaves the store
//! untouched until it commits. Committing runs the domains' validation
//! [rules](crate::Rule) on what changed, then turns the staged changes into a
//! [`ChangeSet`] of deltas and applies it. If a rule fails, or the
//! transaction is dropped, panics or is leaked with [`std::mem::forget`], the
//! staged changes are simply never applied.
//!
//! The commit is the single point where a change becomes part of the model,
//! and the change set it returns is what undo and redo (#19) and change sets
//! (#21) are built on.

use std::mem;

use crate::change::ElementChange;
use crate::view::Staged;
use crate::{
    ChangeSet, Code, Command, Element, ElementId, ElementStore, Model, ModelId, StoreError,
    StoreView, Value, Violation,
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

    /// Checks every element the transaction created or changed against the
    /// rules for its class. If none is broken, applies the changes to the
    /// store and returns them. Otherwise discards them and returns every
    /// [violation](StoreError::Violations).
    pub fn commit(mut self) -> Result<ChangeSet, StoreError> {
        let violations = self.validate();
        if !violations.is_empty() {
            return Err(StoreError::Violations(violations));
        }
        let staged = mem::take(&mut self.staged);
        let elements = staged.order.iter().filter_map(|id| {
            let before = self.store.elements.get(id);
            ElementChange::between(before, staged.elements[id].as_ref())
        });
        let changes = ChangeSet {
            command: self.command.take(),
            elements: elements.collect(),
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

    fn validate(&self) -> Vec<Violation> {
        let view = self.view();
        let changed = self.staged.order.iter();
        changed
            .filter_map(|&id| view.get(id))
            .flat_map(|element| {
                let rules = view.registry().rules(element.class());
                rules.filter_map(move |rule| rule.check(&view, element).err())
            })
            .collect()
    }
}
