//! Levels: named elevations that organise the building by floors (#33).
//!
//! A level is a spatial location element with a name and an elevation, as
//! in Revit and IFC's `IfcBuildingStorey`. The elevation is measured from
//! the project's origin, in metres.
//!
//! Physical elements are **level-based**: they derive from
//! [`LEVEL_BASED_ELEMENT`] and are attached to exactly one level through an
//! [`ATTACHED_TO_LEVEL`] relationship, with an offset from it, as Revit's
//! "base level" and "base offset". The level drives them, so changing its
//! elevation recomputes their [`BASE_ELEVATION`] in the same transaction:
//! one undo step moves the level and everything on it.
//!
//! ```
//! use hito_core::{Element, ElementStore, Model, ModelKind, Relationship, SchemaRegistry, Value};
//! use hito_structural::{class, level};
//!
//! # let mut registry = SchemaRegistry::new();
//! # hito_structural::register(&mut registry)?;
//! # let mut store = ElementStore::new(registry);
//! # let mut tx = store.transaction();
//! # let model = tx.insert_model(Model::new(ModelKind::Physical))?;
//! let first_floor = Element::new(class(level::LEVEL), model)
//!     .with_property(level::NAME, "Primer piso")
//!     .with_property(level::ELEVATION, 3.0);
//! let first_floor = tx.insert(first_floor)?;
//! # tx.commit()?;
//! # let mut tx = store.transaction();
//! tx.set_property(first_floor, level::ELEVATION, 3.2)?;
//! tx.commit()?;
//! assert_eq!(store.property(first_floor, level::ELEVATION), Some(&Value::Real(3.2)));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Deleting a level deletes the elements attached to it, as in Revit. The UI
//! lists them first with
//! [`preview_delete`](hito_core::StoreView::preview_delete) and asks.

use hito_core::{
    ClassDef, ClassId, DataType, Dimension, ElementId, OnDelete, Properties, Recompute,
    RelationshipDef, Rule, StoreView, Value,
};

use crate::class;

/// A level, such as "Planta baja" at 0.00 m.
pub const LEVEL: &str = "Level";
/// The base class of elements that stand on a level: columns, beams, slabs,
/// walls and footings derive from it.
pub const LEVEL_BASED_ELEMENT: &str = "LevelBasedElement";
/// From a level-based element to its level, which drives it. Deleting the
/// level deletes the element.
pub const ATTACHED_TO_LEVEL: &str = "AttachedToLevel";

/// A level's name, unique among levels, such as `Planta baja`. It's data,
/// not interface text, so it isn't translated.
pub const NAME: &str = "Name";
/// A level's elevation above the project's origin.
pub const ELEVATION: &str = "Elevation";
/// How far a level-based element's base is above its level. Negative is
/// below.
pub const LEVEL_OFFSET: &str = "LevelOffset";
/// A level-based element's base elevation above the project's origin: its
/// level's elevation plus its offset. It's recomputed, so setting it has no
/// lasting effect.
pub const BASE_ELEVATION: &str = "BaseElevation";

const LENGTH: DataType = DataType::Quantity(Dimension::LENGTH);

/// The level classes.
pub(crate) fn classes() -> [ClassDef; 2] {
    let level = ClassDef::new(LEVEL, ClassId::SPATIAL_LOCATION_ELEMENT)
        .property(NAME, DataType::Text)
        .property(ELEVATION, LENGTH);
    let level_based = ClassDef::new(LEVEL_BASED_ELEMENT, ClassId::GEOMETRIC_ELEMENT_3D)
        .property(LEVEL_OFFSET, LENGTH)
        .property(BASE_ELEVATION, LENGTH);
    [level, level_based]
}

pub(crate) fn relationships() -> [RelationshipDef; 1] {
    let attached =
        RelationshipDef::new(ATTACHED_TO_LEVEL, class(LEVEL_BASED_ELEMENT), class(LEVEL))
            .driving()
            .on_delete(OnDelete::Cascade);
    [attached]
}

/// A level-based element's base elevation follows its level and offset.
pub(crate) fn recomputes() -> Vec<Recompute> {
    let follow_level = Recompute::new(class(LEVEL_BASED_ELEMENT), |store, element| {
        let mut values = Properties::new();
        if let Some(level) = level_of(store, element.id()) {
            let elevation = length(store, level, ELEVATION).unwrap_or(0.0);
            let offset = length(store, element.id(), LEVEL_OFFSET).unwrap_or(0.0);
            values.insert(BASE_ELEVATION.into(), (elevation + offset).into());
        }
        values
    });
    vec![follow_level]
}

/// The level rules:
///
/// - `LevelName`: every level has a name, and no two levels share one, as
///   in Revit.
/// - `LevelElevation`: every level has an elevation.
/// - `OneLevel`: every level-based element is attached to exactly one level.
pub(crate) fn rules() -> Vec<Rule> {
    let name = Rule::new("LevelName", class(LEVEL), |store, level| {
        let name = match level.properties().get(NAME) {
            Some(Value::Text(name)) if !name.trim().is_empty() => name,
            _ => return Err("the level has no name".into()),
        };
        let registry = store.registry();
        let same_name = store.elements().find(|other| {
            other.id() != level.id()
                && registry.is_a(other.class(), &class(LEVEL))
                && other.properties().get(NAME) == Some(&Value::Text(name.clone()))
        });
        match same_name {
            Some(other) => Err(format!("level {} is also called `{name}`", other.id())),
            None => Ok(()),
        }
    });

    let elevation = Rule::new("LevelElevation", class(LEVEL), |_, level| {
        match level.properties().get(ELEVATION) {
            Some(_) => Ok(()),
            None => Err("the level has no elevation".into()),
        }
    });

    let one_level = Rule::new("OneLevel", class(LEVEL_BASED_ELEMENT), |store, element| {
        let attached = attached_to_level(store, element.id()).count();
        match attached {
            1 => Ok(()),
            0 => Err("the element isn't attached to a level".into()),
            n => Err(format!("the element is attached to {n} levels")),
        }
    });

    vec![name, elevation, one_level]
}

/// The level an element is attached to, if any.
pub fn level_of(store: &StoreView<'_>, element: ElementId) -> Option<ElementId> {
    attached_to_level(store, element).next()
}

/// Every level, lowest first, with its elevation. Levels at the same
/// elevation keep no particular order.
pub fn levels<'a>(store: &StoreView<'a>) -> Vec<(ElementId, f64)> {
    let registry = store.registry();
    let mut levels: Vec<_> = store
        .elements()
        .filter(|e| registry.is_a(e.class(), &class(LEVEL)))
        .map(|e| (e.id(), length(store, e.id(), ELEVATION).unwrap_or(0.0)))
        .collect();
    levels.sort_by(|a, b| a.1.total_cmp(&b.1));
    levels
}

fn attached_to_level<'a>(
    store: &StoreView<'a>,
    element: ElementId,
) -> impl Iterator<Item = ElementId> + use<'a> {
    let attached = class(ATTACHED_TO_LEVEL);
    store
        .relationships_from(element)
        .filter(move |r| r.class() == &attached)
        .map(|r| r.target())
}

fn length(store: &StoreView<'_>, id: ElementId, name: &str) -> Option<f64> {
    match store.property(id, name) {
        Some(Value::Real(x)) => Some(*x),
        _ => None,
    }
}
