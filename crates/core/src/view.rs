//! Reading the store, as it is or as an open transaction would leave it.

use std::collections::HashMap;

use crate::element::Properties;
use crate::{
    Aspect, ClassId, ClassKind, Code, Element, ElementId, ElementStore, Model, ModelId,
    SchemaRegistry, StoreError, Value,
};

/// The changes a transaction has made so far. They stay here, out of the
/// store, until it commits.
#[derive(Debug, Default)]
pub(crate) struct Staged {
    pub(crate) models: Vec<Model>,
    /// The current state of each element the transaction changed, `None` if
    /// it no longer exists.
    pub(crate) elements: HashMap<ElementId, Option<Element>>,
    /// The order the elements were first changed in.
    pub(crate) order: Vec<ElementId>,
    /// Changes to the code index: the element with each code, `None` if no
    /// element has it any more.
    pub(crate) codes: HashMap<Code, Option<ElementId>>,
}

/// A read-only view of an [`ElementStore`]: the store itself, or the store
/// with an open transaction's changes on top. Get one with
/// [`ElementStore::view`] or [`Transaction::view`](crate::Transaction::view).
///
/// Validation rules read the model through it, so they see what the
/// transaction is about to commit.
#[derive(Debug, Clone, Copy)]
pub struct StoreView<'a> {
    store: &'a ElementStore,
    staged: Option<&'a Staged>,
}

impl<'a> StoreView<'a> {
    pub(crate) fn new(store: &'a ElementStore, staged: Option<&'a Staged>) -> Self {
        Self { store, staged }
    }

    pub fn registry(&self) -> &'a SchemaRegistry {
        &self.store.registry
    }

    pub fn model(&self, id: ModelId) -> Option<&'a Model> {
        let mut staged = self.staged.into_iter().flat_map(|s| &s.models);
        staged
            .find(|m| m.id() == id)
            .or_else(|| self.store.models.get(&id))
    }

    /// Every model, in no particular order.
    pub fn models(&self) -> impl Iterator<Item = &'a Model> + use<'a> {
        let staged = self.staged.into_iter().flat_map(|s| &s.models);
        self.store.models.values().chain(staged)
    }

    pub fn get(&self, id: ElementId) -> Option<&'a Element> {
        match self.staged.and_then(|s| s.elements.get(&id)) {
            Some(staged) => staged.as_ref(),
            None => self.store.elements.get(&id),
        }
    }

    /// Every element, in no particular order.
    pub fn elements(&self) -> impl Iterator<Item = &'a Element> + use<'a> {
        let staged = self.staged;
        let unchanged = self.store.elements.values();
        let unchanged =
            unchanged.filter(move |e| staged.is_none_or(|s| !s.elements.contains_key(&e.id)));
        let changed = staged
            .into_iter()
            .flat_map(|s| s.elements.values().flatten());
        unchanged.chain(changed)
    }

    /// The value of property `name` that applies to an element: its own
    /// value if it has one, otherwise its type's. `None` if neither has a
    /// value, or the element doesn't exist.
    pub fn property(&self, id: ElementId, name: &str) -> Option<&'a Value> {
        let element = self.get(id)?;
        element.properties.get(name).or_else(|| {
            let type_element = self.get(element.type_id?)?;
            type_element.properties.get(name)
        })
    }

    /// The element with this code, if any.
    pub fn find_by_code(&self, code: &Code) -> Option<ElementId> {
        match self.staged.and_then(|s| s.codes.get(code)) {
            Some(staged) => *staged,
            None => self.store.codes.get(code).copied(),
        }
    }

    /// Checks a new element against its class:
    ///
    /// - its ID is new, its model exists and its class is a registered
    ///   element class
    /// - each property value is declared by the class and has the right data
    ///   type
    /// - its type, if any, is an element of the class's type class
    /// - its aspects are of registered aspect classes, with at most one of
    ///   each unique aspect class
    /// - its code, if any, isn't used by another element
    pub(crate) fn check_new(&self, element: &Element) -> Result<(), StoreError> {
        let id = element.id;
        if self.get(id).is_some() {
            return Err(StoreError::DuplicateElement(id));
        }
        if self.model(element.model).is_none() {
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
        }
        Ok(())
    }

    pub(crate) fn check_property(
        &self,
        class: &ClassId,
        name: &str,
        value: &Value,
    ) -> Result<(), StoreError> {
        let property =
            self.registry()
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

    pub(crate) fn check_code_free(&self, code: &Code, id: ElementId) -> Result<(), StoreError> {
        match self.find_by_code(code) {
            Some(other) if other != id => Err(StoreError::CodeInUse {
                code: code.clone(),
                element: other,
            }),
            _ => Ok(()),
        }
    }

    fn check_class(
        &self,
        class: &ClassId,
        kind_is_valid: impl Fn(ClassKind) -> bool,
    ) -> Result<ClassKind, StoreError> {
        let kind = self
            .registry()
            .class(class)
            .ok_or_else(|| StoreError::UnknownClass(class.clone()))?
            .kind();
        if !kind_is_valid(kind) {
            return Err(StoreError::WrongClassKind(class.clone()));
        }
        Ok(kind)
    }

    fn check_properties(&self, class: &ClassId, properties: &Properties) -> Result<(), StoreError> {
        properties
            .iter()
            .try_for_each(|(name, value)| self.check_property(class, name, value))
    }

    fn check_type(&self, class: &ClassId, type_id: ElementId) -> Result<(), StoreError> {
        let type_element = self
            .get(type_id)
            .ok_or(StoreError::UnknownElement(type_id))?;
        let registry = self.registry();
        let type_class = registry.class(class).and_then(|c| c.type_class());
        match type_class {
            Some(type_class) if registry.is_a(&type_element.class, type_class) => Ok(()),
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
}
