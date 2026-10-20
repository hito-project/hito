//! Reading the model the same way from a [`StoreView`] (rules, tools,
//! recompute) and from a generator's [`Inputs`], which records what it
//! reads so the geometry follows it. The architectural domain has the same
//! helper for walls.

use hito_core::{ClassId, ElementId, Relationship, SchemaRegistry, StoreView, Value};
use hito_geometry::Inputs;

pub(crate) trait Read<'a> {
    fn registry(&self) -> &'a SchemaRegistry;
    fn class(&self, id: ElementId) -> Option<&'a ClassId>;
    fn type_of(&self, id: ElementId) -> Option<ElementId>;
    fn property(&self, id: ElementId, name: &str) -> Option<&'a Value>;
    fn relationships_from(&self, id: ElementId) -> Vec<&'a Relationship>;

    /// A [`Real`](Value::Real) property's value.
    fn real(&self, id: ElementId, name: &str) -> Option<f64> {
        match self.property(id, name)? {
            Value::Real(x) => Some(*x),
            _ => None,
        }
    }

    fn text(&self, id: ElementId, name: &str) -> Option<&'a str> {
        match self.property(id, name)? {
            Value::Text(text) => Some(text),
            _ => None,
        }
    }

    /// The targets of the relationships of type `class` from `id`.
    fn targets(&self, id: ElementId, class: &ClassId) -> Vec<ElementId> {
        let from = self.relationships_from(id).into_iter();
        from.filter(|r| r.class() == class)
            .map(Relationship::target)
            .collect()
    }

    fn is_a(&self, id: ElementId, class: &ClassId) -> bool {
        self.class(id)
            .is_some_and(|c| self.registry().is_a(c, class))
    }
}

impl<'a> Read<'a> for StoreView<'a> {
    fn registry(&self) -> &'a SchemaRegistry {
        StoreView::registry(self)
    }

    fn class(&self, id: ElementId) -> Option<&'a ClassId> {
        self.get(id).map(|e| e.class())
    }

    fn type_of(&self, id: ElementId) -> Option<ElementId> {
        self.get(id)?.type_id()
    }

    fn property(&self, id: ElementId, name: &str) -> Option<&'a Value> {
        StoreView::property(self, id, name)
    }

    fn relationships_from(&self, id: ElementId) -> Vec<&'a Relationship> {
        StoreView::relationships_from(self, id).collect()
    }
}

impl<'a> Read<'a> for Inputs<'a> {
    fn registry(&self) -> &'a SchemaRegistry {
        Inputs::registry(self)
    }

    fn class(&self, id: ElementId) -> Option<&'a ClassId> {
        self.get(id).map(|e| e.class())
    }

    fn type_of(&self, id: ElementId) -> Option<ElementId> {
        self.get(id)?.type_id()
    }

    fn property(&self, id: ElementId, name: &str) -> Option<&'a Value> {
        Inputs::property(self, id, name)
    }

    fn relationships_from(&self, id: ElementId) -> Vec<&'a Relationship> {
        Inputs::relationships_from(self, id).collect()
    }
}
