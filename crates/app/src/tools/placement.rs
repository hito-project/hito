//! What the structural placement tools share: the level elements go on,
//! the level above it, and the project's first type of a class. The Wall
//! tool has its own copy of the level helpers for now.

use hito_core::{ClassId, Element, ElementId, StoreView};
use hito_structural::level;

/// How far apart two levels must be to count as different: 1 mm.
const TOLERANCE: f64 = 0.001;

/// The level elements go on: the active one if it still exists, or else
/// the lowest.
pub fn base_level(view: &StoreView<'_>, active: Option<ElementId>) -> Option<ElementId> {
    let levels = level::levels(view);
    let exists = |id: &ElementId| levels.iter().any(|(l, _)| l == id);
    active
        .filter(exists)
        .or_else(|| levels.first().map(|(id, _)| *id))
}

/// The lowest level above `base`, if any.
pub fn level_above(view: &StoreView<'_>, base: ElementId) -> Option<ElementId> {
    let levels = level::levels(view);
    let (_, elevation) = levels.iter().find(|(id, _)| *id == base)?;
    let above = levels.iter().find(|(_, e)| *e > elevation + TOLERANCE);
    above.map(|(id, _)| *id)
}

/// The type new elements get until the type selector (#26) exists: the
/// project's first of class `class`.
pub fn first_of(view: &StoreView<'_>, class: &ClassId) -> Option<ElementId> {
    let registry = view.registry();
    view.elements()
        .filter(|e| registry.is_a(e.class(), class))
        .map(Element::id)
        .min()
}
