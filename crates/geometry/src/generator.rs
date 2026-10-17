//! Generators: how a domain's elements get their geometry.

use std::cell::RefCell;
use std::fmt;
use std::sync::Arc;

use hito_core::{ClassId, Element, ElementId, Relationship, SchemaRegistry, StoreView, Value};

use crate::Solid;

type Generate = dyn Fn(&Inputs<'_>, &Element) -> Option<Solid> + Send + Sync;

/// Generates the body geometry of the elements of one class and its
/// subclasses, from their parameters.
///
/// A domain hands its generators to the [`GeometryCache`](crate::GeometryCache)
/// at startup. The function reads the model through [`Inputs`], which
/// records what it read, so the cache knows when to run it again. It returns
/// `None` when the element has no geometry, such as a column whose level is
/// missing. It runs on several threads at once, so it must not depend on
/// anything but its inputs.
///
/// ```
/// use hito_core::ClassId;
/// use hito_geometry::{Frame, Generator, Profile, Solid};
///
/// let column = Generator::new(ClassId::new("Example", "Column"), |inputs, column| {
///     let base = inputs.real(column.id(), "BaseElevation")?;
///     Some(Solid::Extrusion {
///         profile: Profile::Rectangle {
///             width: inputs.real(column.id(), "Width")?,
///             height: inputs.real(column.id(), "Depth")?,
///         },
///         frame: Frame::at([0.0, 0.0, base]),
///         depth: inputs.real(column.id(), "Height")?,
///     })
/// });
/// ```
#[derive(Clone)]
pub struct Generator {
    class: ClassId,
    generate: Arc<Generate>,
}

impl Generator {
    pub fn new(
        class: ClassId,
        generate: impl Fn(&Inputs<'_>, &Element) -> Option<Solid> + Send + Sync + 'static,
    ) -> Self {
        Self {
            class,
            generate: Arc::new(generate),
        }
    }

    pub fn class(&self) -> &ClassId {
        &self.class
    }

    /// Runs the generator on `element`, and returns its solid with the
    /// elements it read: the element itself and its type always, then
    /// whatever else the function asked for.
    pub(crate) fn generate(
        &self,
        view: StoreView<'_>,
        element: &Element,
    ) -> (Option<Solid>, Vec<ElementId>) {
        let inputs = Inputs {
            view,
            read: RefCell::new(Vec::new()),
        };
        inputs.record(element.id());
        let solid = (self.generate)(&inputs, element);
        let mut read = inputs.read.into_inner();
        read.sort();
        read.dedup();
        (solid, read)
    }
}

impl fmt::Debug for Generator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Generator")
            .field("class", &self.class)
            .finish_non_exhaustive()
    }
}

/// What a [`Generator`] reads the model through. Every element it reads
/// becomes an input of the geometry: when one of them changes, the cache
/// generates the geometry again.
pub struct Inputs<'a> {
    view: StoreView<'a>,
    read: RefCell<Vec<ElementId>>,
}

impl<'a> Inputs<'a> {
    pub fn registry(&self) -> &'a SchemaRegistry {
        self.view.registry()
    }

    pub fn get(&self, id: ElementId) -> Option<&'a Element> {
        self.record(id);
        self.view.get(id)
    }

    /// The value of a property that applies to an element: its own, or its
    /// type's, as [`StoreView::property`].
    pub fn property(&self, id: ElementId, name: &str) -> Option<&'a Value> {
        self.record(id);
        self.view.property(id, name)
    }

    /// A [`Real`](Value::Real) property's value, as [`Inputs::property`].
    /// `None` if it's unset or of another type.
    pub fn real(&self, id: ElementId, name: &str) -> Option<f64> {
        match self.property(id, name)? {
            Value::Real(value) => Some(*value),
            _ => None,
        }
    }

    /// The relationships from an element: what it depends on.
    pub fn relationships_from(&self, id: ElementId) -> impl Iterator<Item = &'a Relationship> {
        self.record(id);
        self.view.relationships_from(id)
    }

    /// The relationships to an element: what depends on it.
    pub fn relationships_to(&self, id: ElementId) -> impl Iterator<Item = &'a Relationship> {
        self.record(id);
        self.view.relationships_to(id)
    }

    /// Records an element as read, with its type, whose values it inherits.
    fn record(&self, id: ElementId) {
        let mut read = self.read.borrow_mut();
        read.push(id);
        read.extend(self.view.get(id).and_then(Element::type_id));
    }
}
