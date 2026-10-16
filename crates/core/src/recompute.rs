//! Recompute functions: how a domain's elements follow the elements they
//! depend on.

use std::fmt;
use std::sync::Arc;

use crate::{ClassId, Element, Properties, StoreView};

type Compute = dyn Fn(&StoreView<'_>, &Element) -> Properties + Send + Sync;

/// Recomputes the elements of one class and its subclasses.
///
/// A domain adds its recompute functions to the
/// [`SchemaRegistry`](crate::SchemaRegistry) at startup. When a transaction
/// commits, each element it changed, and each element that depends on one
/// through a [driving](crate::RelationshipDef::driving) relationship or as
/// an instance of a changed type, is recomputed: drivers first, then what
/// they drive. The function reads the model as the transaction has left it
/// so far, and returns the property values to set on the element. Values
/// that don't change aren't set.
///
/// ```
/// use hito_core::{ClassId, Properties, Recompute, Value};
///
/// // A column's base follows its level.
/// let follow_level = Recompute::new(ClassId::new("Example", "Column"), |store, column| {
///     let mut values = Properties::new();
///     let level = store.relationships_from(column.id()).next();
///     if let Some(elevation) = level.and_then(|l| store.property(l.target(), "Elevation")) {
///         values.insert("BaseElevation".into(), elevation.clone());
///     }
///     values
/// });
/// ```
#[derive(Clone)]
pub struct Recompute {
    class: ClassId,
    compute: Arc<Compute>,
}

impl Recompute {
    pub fn new(
        class: ClassId,
        compute: impl Fn(&StoreView<'_>, &Element) -> Properties + Send + Sync + 'static,
    ) -> Self {
        Self {
            class,
            compute: Arc::new(compute),
        }
    }

    pub fn class(&self) -> &ClassId {
        &self.class
    }

    pub(crate) fn compute(&self, view: &StoreView<'_>, element: &Element) -> Properties {
        (self.compute)(view, element)
    }
}

impl fmt::Debug for Recompute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Recompute")
            .field("class", &self.class)
            .finish_non_exhaustive()
    }
}
