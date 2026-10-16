//! The in-memory element store.
//!
//! It holds a session's models and elements, checks every element against
//! the registered schemas, and resolves type and instance property values.
//! It changes only through a [`Transaction`].
//! Saving and loading go through a storage adapter (E10), which reads and
//! writes what this store holds: IDs, class names, codes and property values.

use std::collections::HashMap;
use std::fmt;

use crate::element::Properties;
use crate::{
    Aspect, ClassId, ClassKind, Code, Command, Element, ElementId, Model, ModelId, SchemaRegistry,
    Transaction, Value, Violation,
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
    registry: SchemaRegistry,
    models: HashMap<ModelId, Model>,
    elements: HashMap<ElementId, Element>,
    codes: HashMap<Code, ElementId>,
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

    pub fn registry(&self) -> &SchemaRegistry {
        &self.registry
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

    /// The value of property `name` that applies to an element: its own
    /// value if it has one, otherwise its type's. `None` if neither has a
    /// value, or the element doesn't exist.
    pub fn property(&self, id: ElementId, name: &str) -> Option<&Value> {
        let element = self.elements.get(&id)?;
        element.properties.get(name).or_else(|| {
            let type_element = self.elements.get(&element.type_id?)?;
            type_element.properties.get(name)
        })
    }

    /// The element with this code, if any.
    pub fn find_by_code(&self, code: &Code) -> Option<ElementId> {
        self.codes.get(code).copied()
    }

    // The changes below are made through a `Transaction`, which documents
    // them. Each checks first and changes nothing if it fails.

    pub(crate) fn insert_model(&mut self, model: Model) -> Result<ModelId, StoreError> {
        let id = model.id();
        if self.models.contains_key(&id) {
            return Err(StoreError::DuplicateModel(id));
        }
        self.models.insert(id, model);
        Ok(id)
    }

    pub(crate) fn insert(&mut self, element: Element) -> Result<ElementId, StoreError> {
        let id = element.id;
        if self.elements.contains_key(&id) {
            return Err(StoreError::DuplicateElement(id));
        }
        if !self.models.contains_key(&element.model) {
            return Err(StoreError::UnknownModel(element.model));
        }
        self.check_class(&element.class, |kind| kind == ClassKind::Element)?;
        self.check_properties(&element.class, &element.properties)?;
        if let Some(type_id) = element.type_id {
            self.check_type(&element.class, type_id)?;
        }
        self.check_aspects(&element.aspects)?;
        if let Some(code) = &element.code {
            self.check_code_free(code, id)?;
            self.codes.insert(code.clone(), id);
        }
        self.elements.insert(id, element);
        Ok(id)
    }

    pub(crate) fn set_property(
        &mut self,
        id: ElementId,
        name: &str,
        value: impl Into<Value>,
    ) -> Result<(), StoreError> {
        let value = value.into();
        let element = self
            .elements
            .get(&id)
            .ok_or(StoreError::UnknownElement(id))?;
        self.check_property(&element.class, name, &value)?;
        let element = self.elements.get_mut(&id).expect("checked above");
        element.properties.insert(name.to_owned(), value);
        Ok(())
    }

    pub(crate) fn clear_property(
        &mut self,
        id: ElementId,
        name: &str,
    ) -> Result<Option<Value>, StoreError> {
        let element = self
            .elements
            .get_mut(&id)
            .ok_or(StoreError::UnknownElement(id))?;
        Ok(element.properties.remove(name))
    }

    pub(crate) fn set_code(&mut self, id: ElementId, code: Option<Code>) -> Result<(), StoreError> {
        if !self.elements.contains_key(&id) {
            return Err(StoreError::UnknownElement(id));
        }
        if let Some(code) = &code {
            self.check_code_free(code, id)?;
        }
        let element = self.elements.get_mut(&id).expect("checked above");
        if let Some(old) = element.code.take() {
            self.codes.remove(&old);
        }
        if let Some(code) = &code {
            self.codes.insert(code.clone(), id);
        }
        element.code = code;
        Ok(())
    }

    /// Puts elements back in the state a transaction recorded (`None` if
    /// they didn't exist), latest first, and removes the models it added.
    pub(crate) fn restore(
        &mut self,
        elements: Vec<(ElementId, Option<Element>)>,
        models: &[ModelId],
    ) {
        // Free every code first: an element may get back a code that another
        // restored element holds now.
        for (id, _) in &elements {
            if let Some(code) = self.elements.get(id).and_then(|e| e.code.as_ref()) {
                self.codes.remove(code);
            }
        }
        for (id, before) in elements.into_iter().rev() {
            match before {
                Some(element) => {
                    if let Some(code) = &element.code {
                        self.codes.insert(code.clone(), id);
                    }
                    self.elements.insert(id, element);
                }
                None => {
                    self.elements.remove(&id);
                }
            }
        }
        for id in models {
            self.models.remove(id);
        }
    }

    fn check_class(
        &self,
        class: &ClassId,
        kind_is_valid: impl Fn(ClassKind) -> bool,
    ) -> Result<ClassKind, StoreError> {
        let kind = self
            .registry
            .class(class)
            .ok_or_else(|| StoreError::UnknownClass(class.clone()))?
            .kind();
        if !kind_is_valid(kind) {
            return Err(StoreError::WrongClassKind(class.clone()));
        }
        Ok(kind)
    }

    fn check_property(&self, class: &ClassId, name: &str, value: &Value) -> Result<(), StoreError> {
        let property =
            self.registry
                .property(class, name)
                .ok_or_else(|| StoreError::UnknownProperty {
                    class: class.clone(),
                    property: name.to_owned(),
                })?;
        if !property.data_type().accepts(value) {
            return Err(StoreError::WrongValueType {
                class: class.clone(),
                property: name.to_owned(),
            });
        }
        Ok(())
    }

    fn check_properties(&self, class: &ClassId, properties: &Properties) -> Result<(), StoreError> {
        properties
            .iter()
            .try_for_each(|(name, value)| self.check_property(class, name, value))
    }

    fn check_type(&self, class: &ClassId, type_id: ElementId) -> Result<(), StoreError> {
        let type_element = self
            .elements
            .get(&type_id)
            .ok_or(StoreError::UnknownElement(type_id))?;
        let type_class = self.registry.class(class).and_then(|c| c.type_class());
        match type_class {
            Some(type_class) if self.registry.is_a(&type_element.class, type_class) => Ok(()),
            _ => Err(StoreError::InvalidType {
                class: class.clone(),
                type_id,
            }),
        }
    }

    fn check_aspects(&self, aspects: &[Aspect]) -> Result<(), StoreError> {
        for (i, aspect) in aspects.iter().enumerate() {
            let kind = self.check_class(aspect.class(), |kind| kind != ClassKind::Element)?;
            let repeated = aspects[..i].iter().any(|a| a.class() == aspect.class());
            if kind == ClassKind::UniqueAspect && repeated {
                return Err(StoreError::DuplicateAspect(aspect.class().clone()));
            }
            self.check_properties(aspect.class(), aspect.properties())?;
        }
        Ok(())
    }

    fn check_code_free(&self, code: &Code, id: ElementId) -> Result<(), StoreError> {
        match self.codes.get(code) {
            Some(&other) if other != id => Err(StoreError::CodeInUse {
                code: code.clone(),
                element: other,
            }),
            _ => Ok(()),
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
