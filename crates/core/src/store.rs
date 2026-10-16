//! The in-memory element store.
//!
//! It holds a session's models and elements, checks every element against
//! the registered schemas, and resolves type and instance property values.
//! It changes only through a [`Transaction`].
//! Saving and loading go through a storage adapter (E10), which reads and
//! writes what this store holds: IDs, class names, codes and property values.

use std::collections::HashMap;
use std::fmt;

use crate::change::ElementChange;
use crate::{
    ChangeSet, ClassId, Code, Command, Element, ElementId, Model, ModelId, SchemaRegistry,
    StoreView, Transaction, Value, Violation,
};

/// A session's elements and models, checked against a schema registry.
///
/// Every change goes through a [`Transaction`]. The store itself only reads,
/// so this doesn't compile:
///
/// ```compile_fail
/// # use hito_core::{ElementStore, Model, ModelKind, SchemaRegistry};
/// let mut store = ElementStore::new(SchemaRegistry::new());
/// store.insert_model(Model::new(ModelKind::Physical));
/// ```
#[derive(Debug, Clone)]
pub struct ElementStore {
    pub(crate) registry: SchemaRegistry,
    pub(crate) models: HashMap<ModelId, Model>,
    pub(crate) elements: HashMap<ElementId, Element>,
    pub(crate) codes: HashMap<Code, ElementId>,
}

impl ElementStore {
    /// An empty store. The registry must already hold every schema the
    /// session uses.
    pub fn new(registry: SchemaRegistry) -> Self {
        Self {
            registry,
            models: HashMap::new(),
            elements: HashMap::new(),
            codes: HashMap::new(),
        }
    }

    /// Starts a transaction, the only way to change the store.
    pub fn transaction(&mut self) -> Transaction<'_> {
        Transaction::new(self, None)
    }

    /// Starts a transaction that carries out a user command. The command is
    /// returned with what changed.
    pub fn transaction_for(&mut self, command: Command) -> Transaction<'_> {
        Transaction::new(self, Some(command))
    }

    /// The store as a [`StoreView`], which has every read, documented.
    pub fn view(&self) -> StoreView<'_> {
        StoreView::new(self, None)
    }

    pub fn registry(&self) -> &SchemaRegistry {
        &self.registry
    }

    pub fn model(&self, id: ModelId) -> Option<&Model> {
        self.models.get(&id)
    }

    /// Every model, in no particular order.
    pub fn models(&self) -> impl Iterator<Item = &Model> {
        self.models.values()
    }

    pub fn get(&self, id: ElementId) -> Option<&Element> {
        self.elements.get(&id)
    }

    /// Every stored element, in no particular order.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.elements.values()
    }

    /// See [`StoreView::property`].
    pub fn property(&self, id: ElementId, name: &str) -> Option<&Value> {
        self.view().property(id, name)
    }

    /// The element with this code, if any.
    pub fn find_by_code(&self, code: &Code) -> Option<ElementId> {
        self.codes.get(code).copied()
    }

    /// Applies a committed change set. It runs no checks and no domain
    /// logic: the transaction checked it.
    pub(crate) fn apply(&mut self, changes: &ChangeSet) {
        for model in &changes.models {
            self.models.insert(model.id(), model.clone());
        }
        // Free the codes of every changed element first: one may take a code
        // another gives up.
        for change in &changes.elements {
            let element = self.elements.get(&change.id());
            if let Some(code) = element.and_then(|e| e.code.as_ref()) {
                self.codes.remove(code);
            }
        }
        for change in &changes.elements {
            let id = change.id();
            match change {
                ElementChange::Created(element) => {
                    self.elements.insert(id, element.clone());
                }
                ElementChange::Modified { fields, .. } => {
                    let element = self.elements.get_mut(&id).expect("modified elements exist");
                    fields.iter().for_each(|field| field.apply(element));
                }
                ElementChange::Deleted(_) => {
                    self.elements.remove(&id);
                }
            }
            if let Some(code) = self.elements.get(&id).and_then(|e| e.code.clone()) {
                self.codes.insert(code, id);
            }
        }
    }
}

/// Why the store refused a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    DuplicateModel(ModelId),
    DuplicateElement(ElementId),
    UnknownModel(ModelId),
    UnknownElement(ElementId),
    UnknownClass(ClassId),
    /// An aspect class was used for an element, or an element class for an
    /// aspect.
    WrongClassKind(ClassId),
    /// The class has no property with this name.
    UnknownProperty {
        class: ClassId,
        property: String,
    },
    /// The value doesn't fit the property's data type.
    WrongValueType {
        class: ClassId,
        property: String,
    },
    /// The class has no type class, or the type element isn't of it.
    InvalidType {
        class: ClassId,
        type_id: ElementId,
    },
    /// The element has two aspects of the same unique aspect class.
    DuplicateAspect(ClassId),
    /// Another element already has this code.
    CodeInUse {
        code: Code,
        element: ElementId,
    },
    /// A transaction broke validation rules, so it was rolled back.
    Violations(Vec<Violation>),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateModel(id) => write!(f, "model {id:?} already exists"),
            Self::DuplicateElement(id) => write!(f, "element {id} already exists"),
            Self::UnknownModel(id) => write!(f, "model {id:?} doesn't exist"),
            Self::UnknownElement(id) => write!(f, "element {id} doesn't exist"),
            Self::UnknownClass(class) => write!(f, "class `{class}` isn't registered"),
            Self::WrongClassKind(class) => write!(f, "class `{class}` is the wrong kind here"),
            Self::UnknownProperty { class, property } => {
                write!(f, "class `{class}` has no property `{property}`")
            }
            Self::WrongValueType { class, property } => {
                write!(f, "wrong value type for property `{property}` of `{class}`")
            }
            Self::InvalidType { class, type_id } => {
                write!(f, "element {type_id} can't be the type of a `{class}`")
            }
            Self::DuplicateAspect(class) => write!(f, "unique aspect `{class}` appears twice"),
            Self::CodeInUse { code, element } => {
                write!(f, "code `{}` is used by element {element}", code.value())
            }
            Self::Violations(violations) => {
                write!(f, "{} validation rules broken", violations.len())?;
                violations.iter().try_for_each(|v| write!(f, "\n- {v}"))
            }
        }
    }
}

impl std::error::Error for StoreError {}
