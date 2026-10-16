//! Reading the store, as it is or as an open transaction would leave it.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::element::Properties;
use crate::{
    Aspect, ClassId, ClassKind, Code, Element, ElementId, ElementStore, Model, ModelId, OnDelete,
    Relationship, SchemaRegistry, StoreError, Value,
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
    /// Each relationship the transaction added or removed, and whether it
    /// exists now. In the order they were first changed.
    pub(crate) relationships: HashMap<Relationship, bool>,
    pub(crate) relationship_order: Vec<Relationship>,
    /// The added relationships the store doesn't have, by source and by
    /// target.
    outgoing: HashMap<ElementId, Vec<Relationship>>,
    incoming: HashMap<ElementId, Vec<Relationship>>,
    /// The elements the transaction created with a type, by type.
    pub(crate) instances: HashMap<ElementId, Vec<ElementId>>,
}

impl Staged {
    /// Records that a relationship now exists, or no longer does.
    /// `in_store` says whether the store has it.
    pub(crate) fn set_relationship(&mut self, r: Relationship, exists: bool, in_store: bool) {
        if !self.relationships.contains_key(&r) {
            self.relationship_order.push(r.clone());
        }
        self.relationships.insert(r.clone(), exists);
        if in_store {
            return;
        }
        let (source, target) = (r.source(), r.target());
        if exists {
            self.outgoing.entry(source).or_default().push(r.clone());
            self.incoming.entry(target).or_default().push(r);
        } else {
            for (index, id) in [(&mut self.outgoing, source), (&mut self.incoming, target)] {
                if let Some(list) = index.get_mut(&id) {
                    list.retain(|x| x != &r);
                }
            }
        }
    }
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

    /// The relationships from an element: what it depends on.
    pub fn relationships_from(
        &self,
        source: ElementId,
    ) -> impl Iterator<Item = &'a Relationship> + use<'a> {
        let staged = self.staged;
        let stored = self.store.outgoing.get(&source).into_iter().flatten();
        let added = staged.and_then(|s| s.outgoing.get(&source));
        let kept =
            stored.filter(move |r| staged.is_none_or(|s| s.relationships.get(*r) != Some(&false)));
        kept.chain(added.into_iter().flatten())
    }

    /// The relationships to an element: what depends on it.
    pub fn relationships_to(
        &self,
        target: ElementId,
    ) -> impl Iterator<Item = &'a Relationship> + use<'a> {
        let staged = self.staged;
        let stored = self.store.incoming.get(&target).into_iter().flatten();
        let added = staged.and_then(|s| s.incoming.get(&target));
        let kept =
            stored.filter(move |r| staged.is_none_or(|s| s.relationships.get(*r) != Some(&false)));
        kept.chain(added.into_iter().flatten())
    }

    /// Everything deleting `id` would delete, without deleting anything: the
    /// element first, then, directly or not, the instances of a deleted
    /// type and the sources of cascading relationships to a deleted element.
    /// Fails, as the deletion would, if a blocking relationship would be
    /// left pointing at a deleted element.
    ///
    /// The UI shows this to ask for confirmation before deleting.
    pub fn preview_delete(&self, id: ElementId) -> Result<Vec<ElementId>, StoreError> {
        if self.get(id).is_none() {
            return Err(StoreError::UnknownElement(id));
        }
        let mut deleted = vec![id];
        let mut seen = HashSet::from([id]);
        let mut next = 0;
        while let Some(&element) = deleted.get(next) {
            next += 1;
            let owned = self
                .relationships_to(element)
                .filter_map(|r| (self.delete_rule(r) == OnDelete::Cascade).then_some(r.source()));
            for owned in owned.chain(self.instances_of(element)) {
                if seen.insert(owned) {
                    deleted.push(owned);
                }
            }
        }
        for &element in &deleted {
            let blocking = self
                .relationships_to(element)
                .find(|r| self.delete_rule(r) == OnDelete::Block && !seen.contains(&r.source()));
            if let Some(relationship) = blocking {
                return Err(StoreError::DeleteBlocked {
                    element,
                    relationship: relationship.clone(),
                });
            }
        }
        Ok(deleted)
    }

    pub(crate) fn has_relationship(&self, relationship: &Relationship) -> bool {
        match self.staged.and_then(|s| s.relationships.get(relationship)) {
            Some(&exists) => exists,
            None => self.store.has_relationship(relationship),
        }
    }

    /// The elements whose type is `type_id`.
    pub(crate) fn instances_of(
        &self,
        type_id: ElementId,
    ) -> impl Iterator<Item = ElementId> + use<'a> {
        let view = *self;
        let stored = self.store.instances.get(&type_id).into_iter().flatten();
        let added = self.staged.and_then(|s| s.instances.get(&type_id));
        let all = stored.chain(added.into_iter().flatten()).copied();
        all.filter(move |&i| view.get(i).is_some_and(|e| e.type_id == Some(type_id)))
    }

    /// Checks that a new relationship's type is registered, its elements
    /// exist and are of the type's classes, it's new, and, if it drives,
    /// that it doesn't close a cycle of dependencies.
    pub(crate) fn check_relationship(&self, relationship: &Relationship) -> Result<(), StoreError> {
        let registry = self.registry();
        let class = relationship.class();
        let def = registry
            .relationship(class)
            .ok_or_else(|| StoreError::UnknownRelationship(class.clone()))?;
        let ends = [
            (relationship.source(), def.source()),
            (relationship.target(), def.target()),
        ];
        for (id, end_class) in ends {
            let element = self.get(id).ok_or(StoreError::UnknownElement(id))?;
            if !registry.is_a(&element.class, end_class) {
                return Err(StoreError::InvalidRelationship(relationship.clone()));
            }
        }
        if self.has_relationship(relationship) {
            return Err(StoreError::DuplicateRelationship(relationship.clone()));
        }
        if def.drives() {
            let (source, target) = (relationship.source(), relationship.target());
            if let Some(path) = self.dependency_path(target, source) {
                let cycle = [source].into_iter().chain(path).collect();
                return Err(StoreError::CircularDependency(cycle));
            }
        }
        Ok(())
    }

    /// `changed` and every element they drive, directly or not, in an order
    /// where each element comes after the elements that drive it. An element
    /// is driven by the targets of its driving relationships and by its
    /// type.
    pub(crate) fn recompute_order(
        &self,
        changed: impl IntoIterator<Item = ElementId>,
    ) -> Result<Vec<ElementId>, StoreError> {
        let mut affected = Vec::new();
        let mut seen = HashSet::new();
        for id in changed {
            if seen.insert(id) {
                affected.push(id);
            }
        }
        let mut next = 0;
        while let Some(&id) = affected.get(next) {
            next += 1;
            for dependant in self.driven_by(id) {
                if seen.insert(dependant) {
                    affected.push(dependant);
                }
            }
        }

        // Kahn's algorithm, on the affected part of the graph only.
        let mut drivers: HashMap<ElementId, usize> = affected
            .iter()
            .map(|&id| (id, self.drivers(id).filter(|d| seen.contains(d)).count()))
            .collect();
        let mut ready: VecDeque<_> = affected
            .iter()
            .copied()
            .filter(|id| drivers[id] == 0)
            .collect();
        let mut order = Vec::with_capacity(affected.len());
        while let Some(id) = ready.pop_front() {
            order.push(id);
            for dependant in self.driven_by(id) {
                let count = drivers.get_mut(&dependant).expect("affected");
                *count -= 1;
                if *count == 0 {
                    ready.push_back(dependant);
                }
            }
        }
        if order.len() < affected.len() {
            // `relate` refuses cycles, so this is a bug, but it's reported
            // rather than looped on.
            let mut cycle: Vec<_> = drivers.into_iter().filter(|(_, n)| *n > 0).collect();
            cycle.sort();
            return Err(StoreError::CircularDependency(
                cycle.into_iter().map(|(id, _)| id).collect(),
            ));
        }
        Ok(order)
    }

    fn delete_rule(&self, relationship: &Relationship) -> OnDelete {
        let def = self.registry().relationship(relationship.class());
        def.map_or(OnDelete::Detach, |d| d.delete_rule())
    }

    fn drives(&self, relationship: &Relationship) -> bool {
        let def = self.registry().relationship(relationship.class());
        def.is_some_and(|d| d.drives())
    }

    /// The elements that drive `id`: the targets of its driving
    /// relationships, and its type.
    fn drivers(&self, id: ElementId) -> impl Iterator<Item = ElementId> + use<'a> {
        let view = *self;
        let driving = self.relationships_from(id).filter(move |r| view.drives(r));
        let type_id = self.get(id).and_then(|e| e.type_id);
        driving.map(|r| r.target()).chain(type_id)
    }

    /// The elements `id` drives: the sources of driving relationships to
    /// it, and its instances if it's a type.
    fn driven_by(&self, id: ElementId) -> impl Iterator<Item = ElementId> + use<'a> {
        let view = *self;
        let driving = self.relationships_to(id).filter(move |r| view.drives(r));
        driving.map(|r| r.source()).chain(self.instances_of(id))
    }

    /// A chain of driving dependencies from `from` to `to`, as the elements
    /// along it, both ends included.
    fn dependency_path(&self, from: ElementId, to: ElementId) -> Option<Vec<ElementId>> {
        let mut came_from = HashMap::from([(from, from)]);
        let mut stack = vec![from];
        while let Some(id) = stack.pop() {
            if id == to {
                let mut path = vec![id];
                while let Some(&previous) = path.last().filter(|&&p| p != from) {
                    path.push(came_from[&previous]);
                }
                path.reverse();
                return Some(path);
            }
            for driver in self.drivers(id) {
                if let Entry::Vacant(entry) = came_from.entry(driver) {
                    entry.insert(id);
                    stack.push(driver);
                }
            }
        }
        None
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
