//! Isolated footings: rectangular pads under columns, or placed freely on a
//! level (#41).
//!
//! A footing is a level-based element, as Revit's isolated structural
//! foundations: its top face is on its level, raised by the level offset,
//! and its type's [`THICKNESS`] goes down from there. In plan it's centred
//! on [`X`], [`Y`], its [`LENGTH`] along the direction [`ROTATION`] turns
//! to (anticlockwise from x) and its [`WIDTH`] across it.
//!
//! A footing placed **under a column** is linked to it by [`UNDER`], which
//! drives it: its centre follows the column's position and its top the
//! column's base, whatever its own coordinates and offset say, as Revit's
//! footings placed "At Columns" move with them. Deleting the column leaves
//! the footing where it was. A column on grids moves with them, and its
//! footing with it.
//!
//! A [footing type](FOOTING_TYPE) has a length, a width, a thickness and a
//! structural [material](crate::material), such as a concrete. Through IFC
//! (#51) a footing is an `IfcFooting` (`PAD_FOOTING`).
//!
//! ```
//! use hito_core::{Element, ElementStore, Model, ModelKind, SchemaRegistry, Value};
//! use hito_structural::{class, footing, level};
//!
//! # let mut registry = SchemaRegistry::new();
//! # hito_structural::register(&mut registry)?;
//! # let mut store = ElementStore::new(registry);
//! let mut tx = store.transaction();
//! let model = tx.insert_model(Model::new(ModelKind::Physical))?;
//! let foundation = Element::new(class(level::LEVEL), model)
//!     .with_property(level::NAME, "Fundación")
//!     .with_property(level::ELEVATION, -1.5);
//! let foundation = tx.insert(foundation)?;
//! let pad = footing::insert_type(&mut tx, model, "Base 1.2 x 1.2", [1.2, 1.2, 0.4], None)?;
//! let base = footing::insert(&mut tx, model, &footing::Placement::at(pad, foundation, [4.0, 0.0]))?;
//! tx.commit()?;
//! assert_eq!(footing::elevations(&store.view(), base), Some((-1.9, -1.5)));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use hito_core::{
    ClassDef, ClassId, DataType, Dimension, Element, ElementId, ModelId, Properties, Recompute,
    Relationship, RelationshipDef, Rule, StoreError, StoreView, Transaction, Value,
};

use crate::column::{self, COLUMN};
use crate::level::{
    ATTACHED_TO_LEVEL, BASE_ELEVATION, ELEVATION, LEVEL_BASED_ELEMENT, LEVEL_OFFSET,
};
use crate::material::MATERIAL;
use crate::outline::{Point, TOLERANCE};
use crate::read::Read;
use crate::{class, level};

pub use crate::level::{BOTTOM_ELEVATION, TOP_ELEVATION};

/// An isolated footing.
pub const FOOTING: &str = "IsolatedFooting";
/// An isolated footing type: its size and material.
pub const FOOTING_TYPE: &str = "IsolatedFootingType";
/// From a footing to the column it's under, which drives it. Deleting the
/// column detaches the footing.
pub const UNDER: &str = "FootingUnder";
/// From a footing type to its material.
pub const FOOTING_MATERIAL: &str = "FootingMaterial";

/// A footing type's name, unique among footing types. It's data, not
/// interface text, so it isn't translated.
pub const NAME: &str = "Name";
/// A footing type's size: along its direction, across it, and down from
/// its top.
pub const LENGTH: &str = "Length";
pub const WIDTH: &str = "Width";
pub const THICKNESS: &str = "Thickness";
/// A footing's centre, in plan. Under a column it's recomputed.
pub const X: &str = "X";
pub const Y: &str = "Y";
/// Which way a footing's length runs, anticlockwise from x.
pub const ROTATION: &str = "Rotation";
/// How far a footing's top is above its level, as Revit's "Height Offset
/// From Level": the level-based element's level offset. Unused under a
/// column.
pub use crate::level::LEVEL_OFFSET as HEIGHT_OFFSET;

const LENGTH_TYPE: DataType = DataType::Quantity(Dimension::LENGTH);

/// Where a footing goes, for [`insert`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Location {
    /// Centred here, its top `offset` above `level`.
    Free {
        level: ElementId,
        centre: Point,
        offset: f64,
    },
    /// Under this column, at its base, on its level.
    Under { column: ElementId },
}

/// A footing to add with [`insert`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub footing_type: ElementId,
    pub location: Location,
    pub rotation: f64,
}

impl Placement {
    /// A footing of `footing_type` centred on `centre`, its top on `level`.
    pub fn at(footing_type: ElementId, level: ElementId, centre: Point) -> Self {
        Self {
            footing_type,
            location: Location::Free {
                level,
                centre,
                offset: 0.0,
            },
            rotation: 0.0,
        }
    }

    /// A footing of `footing_type` under `column`.
    pub fn under(footing_type: ElementId, column: ElementId) -> Self {
        Self {
            footing_type,
            location: Location::Under { column },
            rotation: 0.0,
        }
    }

    /// The same footing, its top `offset` above its level. Unused under a
    /// column.
    pub fn with_offset(mut self, new: f64) -> Self {
        if let Location::Free { offset, .. } = &mut self.location {
            *offset = new;
        }
        self
    }

    pub fn with_rotation(mut self, rotation: f64) -> Self {
        self.rotation = rotation;
        self
    }
}

/// Adds a footing type called `name` to `model`, `[length, width,
/// thickness]` in size. Returns the type.
pub fn insert_type(
    tx: &mut Transaction<'_>,
    model: ModelId,
    name: &str,
    [length, width, thickness]: [f64; 3],
    material: Option<ElementId>,
) -> Result<ElementId, StoreError> {
    let element = Element::new(class(FOOTING_TYPE), model)
        .with_property(NAME, name)
        .with_property(LENGTH, length)
        .with_property(WIDTH, width)
        .with_property(THICKNESS, thickness);
    let footing_type = tx.insert(element)?;
    if let Some(material) = material {
        let material = Relationship::new(class(FOOTING_MATERIAL), footing_type, material);
        tx.relate(material)?;
    }
    Ok(footing_type)
}

/// Adds a footing to `model`. Under a column, it goes on the column's
/// level, and fails if the column has none. Returns the footing.
pub fn insert(
    tx: &mut Transaction<'_>,
    model: ModelId,
    placement: &Placement,
) -> Result<ElementId, StoreError> {
    let mut element = Element::new(class(FOOTING), model)
        .with_type(placement.footing_type)
        .with_property(ROTATION, placement.rotation);
    let level = match placement.location {
        Location::Free {
            level,
            centre,
            offset,
        } => {
            element = element
                .with_property(X, centre[0])
                .with_property(Y, centre[1])
                .with_property(LEVEL_OFFSET, offset);
            level
        }
        Location::Under { column } => level::level_of(&tx.view(), column)
            .ok_or_else(|| StoreError::UnknownClass(class(level::LEVEL)))?,
    };
    let footing = tx.insert(element)?;
    tx.relate(Relationship::new(class(ATTACHED_TO_LEVEL), footing, level))?;
    if let Location::Under { column } = placement.location {
        tx.relate(Relationship::new(class(UNDER), footing, column))?;
    }
    Ok(footing)
}

/// The column a footing is under, if any.
pub fn column(store: &StoreView<'_>, footing: ElementId) -> Option<ElementId> {
    store.targets(footing, &class(UNDER)).first().copied()
}

/// The footings under a column.
pub fn footings_under(store: &StoreView<'_>, column: ElementId) -> Vec<ElementId> {
    let mut footings = store.sources(column, &class(UNDER));
    footings.sort();
    footings
}

/// A footing's centre in plan, from its column or its own coordinates, as
/// it's recomputed.
pub fn centre(store: &StoreView<'_>, footing: ElementId) -> Option<Point> {
    read_centre(store, footing)
}

/// A footing's bottom and top elevations, as they're recomputed.
pub fn elevations(store: &StoreView<'_>, footing: ElementId) -> Option<(f64, f64)> {
    read_elevations(store, footing)
}

/// A footing type's material, if it has one.
pub fn material(store: &StoreView<'_>, footing_type: ElementId) -> Option<ElementId> {
    let materials = store.targets(footing_type, &class(FOOTING_MATERIAL));
    materials.first().copied()
}

/// Where a column stands: its centre in plan and its base elevation, where
/// the footing under it goes.
pub fn column_point(store: &StoreView<'_>, column: ElementId) -> Option<[f64; 3]> {
    read_column_point(store, column)
}

/// The column's location (recomputed first when it's on grids, as it drives
/// the footing) and its base from its level and offset, so both are right
/// before the transaction commits.
pub(crate) fn read_column_point<'a>(store: &impl Read<'a>, column: ElementId) -> Option<[f64; 3]> {
    let [x, y] = column::read_location(store, column)?;
    let (base, _) = column::read_elevations(store, column)?;
    Some([x, y, base])
}

/// The footing classes.
pub(crate) fn classes() -> [ClassDef; 2] {
    let footing_type = ClassDef::new(FOOTING_TYPE, ClassId::DEFINITION_ELEMENT)
        .property(NAME, DataType::Text)
        .property(LENGTH, LENGTH_TYPE)
        .property(WIDTH, LENGTH_TYPE)
        .property(THICKNESS, LENGTH_TYPE);
    let footing = ClassDef::new(FOOTING, class(LEVEL_BASED_ELEMENT))
        .typed_by(class(FOOTING_TYPE))
        .property(X, LENGTH_TYPE)
        .property(Y, LENGTH_TYPE)
        .property(ROTATION, DataType::Quantity(Dimension::ANGLE))
        .property(TOP_ELEVATION, LENGTH_TYPE)
        .property(BOTTOM_ELEVATION, LENGTH_TYPE);
    [footing_type, footing]
}

pub(crate) fn relationships() -> [RelationshipDef; 2] {
    let under = RelationshipDef::new(UNDER, class(FOOTING), class(COLUMN)).driving();
    let material = RelationshipDef::new(FOOTING_MATERIAL, class(FOOTING_TYPE), class(MATERIAL));
    [under, material]
}

/// A footing follows its column, or its level and offset, and its type.
pub(crate) fn recomputes() -> Vec<Recompute> {
    let follow = Recompute::new(class(FOOTING), |store, footing| {
        let mut values = Properties::new();
        if let Some((bottom, top)) = read_elevations(store, footing.id()) {
            values.insert(BOTTOM_ELEVATION.into(), bottom.into());
            values.insert(TOP_ELEVATION.into(), top.into());
        }
        if let Some(column) = store.targets(footing.id(), &class(UNDER)).first()
            && let Some([x, y, base]) = read_column_point(store, *column)
        {
            values.insert(X.into(), x.into());
            values.insert(Y.into(), y.into());
            // Its base is its top: the level-based element's base follows.
            values.insert(BASE_ELEVATION.into(), base.into());
        }
        values
    });
    vec![follow]
}

/// The footing rules:
///
/// - `FootingTypeName`: every footing type has a name, and no two share
///   one, as in Revit.
/// - `FootingTypeSize`: every footing type is longer, wider and thicker
///   than 1 mm.
/// - `Footing`: every footing has a footing type, is under at most one
///   column, and has a centre: its own or its column's.
pub(crate) fn rules() -> Vec<Rule> {
    let name = Rule::new("FootingTypeName", class(FOOTING_TYPE), |store, ft| {
        let name = match ft.properties().get(NAME) {
            Some(Value::Text(name)) if !name.trim().is_empty() => name,
            _ => return Err("the footing type has no name".into()),
        };
        let registry = store.registry();
        let same_name = store.elements().find(|other| {
            other.id() != ft.id()
                && registry.is_a(other.class(), &class(FOOTING_TYPE))
                && other.properties().get(NAME) == Some(&Value::Text(name.clone()))
        });
        match same_name {
            Some(other) => Err(format!(
                "footing type {} is also called `{name}`",
                other.id()
            )),
            None => Ok(()),
        }
    });

    let size = Rule::new("FootingTypeSize", class(FOOTING_TYPE), |store, ft| {
        for dimension in [LENGTH, WIDTH, THICKNESS] {
            match store.real(ft.id(), dimension) {
                Some(d) if d > TOLERANCE => {}
                Some(d) => return Err(format!("the footing type's {dimension} is {d} m")),
                None => return Err(format!("the footing type has no {dimension}")),
            }
        }
        Ok(())
    });

    let footing = Rule::new("Footing", class(FOOTING), |store, footing| {
        if footing.type_id().is_none() {
            return Err("the footing has no type".into());
        }
        let columns = store.targets(footing.id(), &class(UNDER)).len();
        if columns > 1 {
            return Err(format!("the footing is under {columns} columns"));
        }
        match read_centre(store, footing.id()) {
            Some(_) => Ok(()),
            None => Err("the footing has no position".into()),
        }
    });

    vec![name, size, footing]
}

pub(crate) fn read_centre<'a>(store: &impl Read<'a>, footing: ElementId) -> Option<Point> {
    match store.targets(footing, &class(UNDER)).first() {
        Some(&column) => read_column_point(store, column).map(|[x, y, _]| [x, y]),
        None => Some([store.real(footing, X)?, store.real(footing, Y)?]),
    }
}

/// The bottom and top elevations, read from what drives them rather than
/// from the recomputed properties, so they're right inside a transaction
/// before it commits.
pub(crate) fn read_elevations<'a>(store: &impl Read<'a>, footing: ElementId) -> Option<(f64, f64)> {
    let top = match store.targets(footing, &class(UNDER)).first() {
        Some(&column) => read_column_point(store, column)?[2],
        None => {
            let level = *store
                .targets(footing, &class(level::ATTACHED_TO_LEVEL))
                .first()?;
            store.real(level, ELEVATION)? + store.real(footing, LEVEL_OFFSET).unwrap_or(0.0)
        }
    };
    Some((top - store.real(footing, THICKNESS)?, top))
}

/// Every footing in the store, lowest ID first.
pub fn footings(store: &StoreView<'_>) -> Vec<ElementId> {
    let registry = store.registry();
    let mut footings: Vec<_> = store
        .elements()
        .filter(|e| registry.is_a(e.class(), &class(FOOTING)))
        .map(Element::id)
        .collect();
    footings.sort();
    footings
}
