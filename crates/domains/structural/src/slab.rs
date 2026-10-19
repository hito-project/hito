//! Slabs: floors drawn by their boundary on a level, with openings (#39).
//!
//! A slab is a [horizontal element](crate::level::HORIZONTAL_ELEMENT), as
//! Revit's structural floors: its top face is on its level, raised by the
//! level offset (Revit's "Height Offset From Level"), and its type's
//! [`THICKNESS`] goes down from there. The level drives it, so moving the
//! level moves the slab, and the walls attached to it, in the same
//! transaction. [`TOP_ELEVATION`] and [`BOTTOM_ELEVATION`] are recomputed.
//!
//! In plan, a slab is its **boundary**: a closed [outline](crate::outline),
//! stored as the slab's [`BOUNDARY_POINT`] aspects, one per corner in
//! [`POSITION`] order, as Revit's sketch keeps the slab's boundary lines.
//! **Openings** are elements of their own, each with its own boundary,
//! owned by the slab through [`OPENING_IN`]: they must lie inside the
//! slab's boundary and apart from each other, and deleting the slab
//! deletes them. Through IFC (#51) a slab is an `IfcSlab` and its openings
//! `IfcOpeningElement`s.
//!
//! A [slab type](SLAB_TYPE) has a thickness and a material, as Revit's floor
//! types. Its material is a structural [material](crate::material), such
//! as a concrete.
//!
//! ```
//! use hito_core::{Element, ElementStore, Model, ModelKind, SchemaRegistry, Value};
//! use hito_structural::{class, level, slab};
//!
//! # let mut registry = SchemaRegistry::new();
//! # hito_structural::register(&mut registry)?;
//! # let mut store = ElementStore::new(registry);
//! let mut tx = store.transaction();
//! let model = tx.insert_model(Model::new(ModelKind::Physical))?;
//! let first = Element::new(class(level::LEVEL), model)
//!     .with_property(level::NAME, "Primer piso")
//!     .with_property(level::ELEVATION, 3.0);
//! let first = tx.insert(first)?;
//! let losa = slab::insert_type(&mut tx, model, "Losa 12", 0.12, None)?;
//! let outline = [[0.0, 0.0], [6.0, 0.0], [6.0, 4.0], [0.0, 4.0]];
//! let floor = slab::insert(&mut tx, model, &slab::Placement::new(losa, first, &outline))?;
//! slab::insert_opening(&mut tx, model, floor, &[[1.0, 1.0], [2.0, 1.0], [2.0, 2.0], [1.0, 2.0]])?;
//! tx.commit()?;
//! assert_eq!(store.property(floor, slab::TOP_ELEVATION), Some(&Value::Real(3.0)));
//! assert_eq!(slab::elevations(&store.view(), floor), Some((2.88, 3.0)));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use hito_core::{
    Aspect, ClassDef, ClassId, DataType, Dimension, Element, ElementId, ModelId, OnDelete,
    Properties, Recompute, Relationship, RelationshipDef, Rule, StoreError, StoreView, Transaction,
    Value,
};

use crate::class;
use crate::level::{ATTACHED_TO_LEVEL, ELEVATION, HORIZONTAL_ELEMENT, LEVEL_OFFSET};
use crate::material::MATERIAL;
use crate::outline::{self, Point};
use crate::read::Read;

pub use crate::level::{BOTTOM_ELEVATION, TOP_ELEVATION};

/// A slab.
pub const SLAB: &str = "Slab";
/// A slab type: a thickness and a material.
pub const SLAB_TYPE: &str = "SlabType";
/// An opening in a slab, such as a stairwell or a shaft.
pub const SLAB_OPENING: &str = "SlabOpening";
/// One corner of a slab's or an opening's boundary: a multi-aspect.
pub const BOUNDARY_POINT: &str = "BoundaryPoint";
/// From an opening to the slab it's in, which owns and drives it. Deleting
/// the slab deletes its openings.
pub const OPENING_IN: &str = "OpeningIn";
/// From a slab type to its material.
pub const SLAB_MATERIAL: &str = "SlabMaterial";

/// A slab type's name, unique among slab types. It's data, not interface
/// text, so it isn't translated.
pub const NAME: &str = "Name";
/// A slab type's thickness, down from the slab's top face.
pub const THICKNESS: &str = "Thickness";
/// Where a boundary corner comes along the boundary: corners go in
/// increasing position, and positions are unique within one boundary.
pub const POSITION: &str = "Position";
/// A boundary corner, in plan.
pub const X: &str = "X";
pub const Y: &str = "Y";
/// How far the slab's top is above its level, as Revit's "Height Offset
/// From Level": the level-based element's level offset.
pub use crate::level::LEVEL_OFFSET as HEIGHT_OFFSET;

const LENGTH: DataType = DataType::Quantity(Dimension::LENGTH);

/// A slab to add with [`insert`].
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub slab_type: ElementId,
    pub level: ElementId,
    pub offset: f64,
    pub outline: Vec<Point>,
}

impl Placement {
    /// A slab of `slab_type` on `level`, its top on the level, with this
    /// boundary.
    pub fn new(slab_type: ElementId, level: ElementId, outline: &[Point]) -> Self {
        Self {
            slab_type,
            level,
            offset: 0.0,
            outline: outline.to_vec(),
        }
    }

    pub fn with_offset(mut self, offset: f64) -> Self {
        self.offset = offset;
        self
    }
}

/// Adds a slab type called `name` to `model`. Returns the type.
pub fn insert_type(
    tx: &mut Transaction<'_>,
    model: ModelId,
    name: &str,
    thickness: f64,
    material: Option<ElementId>,
) -> Result<ElementId, StoreError> {
    let element = Element::new(class(SLAB_TYPE), model)
        .with_property(NAME, name)
        .with_property(THICKNESS, thickness);
    let slab_type = tx.insert(element)?;
    if let Some(material) = material {
        tx.relate(Relationship::new(class(SLAB_MATERIAL), slab_type, material))?;
    }
    Ok(slab_type)
}

/// Adds a slab to `model`. Returns the slab.
pub fn insert(
    tx: &mut Transaction<'_>,
    model: ModelId,
    placement: &Placement,
) -> Result<ElementId, StoreError> {
    let element = Element::new(class(SLAB), model)
        .with_type(placement.slab_type)
        .with_property(LEVEL_OFFSET, placement.offset);
    let slab = tx.insert(with_boundary(element, &placement.outline))?;
    tx.relate(Relationship::new(
        class(ATTACHED_TO_LEVEL),
        slab,
        placement.level,
    ))?;
    Ok(slab)
}

/// Adds an opening with this boundary to `slab`. Returns the opening.
pub fn insert_opening(
    tx: &mut Transaction<'_>,
    model: ModelId,
    slab: ElementId,
    outline: &[Point],
) -> Result<ElementId, StoreError> {
    let element = with_boundary(Element::new(class(SLAB_OPENING), model), outline);
    let opening = tx.insert(element)?;
    tx.relate(Relationship::new(class(OPENING_IN), opening, slab))?;
    Ok(opening)
}

fn with_boundary(element: Element, outline: &[Point]) -> Element {
    (1i64..)
        .zip(outline)
        .fold(element, |element, (position, p)| {
            let corner = Aspect::new(class(BOUNDARY_POINT))
                .with_property(POSITION, position)
                .with_property(X, p[0])
                .with_property(Y, p[1]);
            element.with_aspect(corner)
        })
}

/// A slab's or an opening's boundary, corner by corner.
pub fn boundary(store: &StoreView<'_>, element: ElementId) -> Vec<Point> {
    read_boundary(store, element)
}

/// A slab's openings.
pub fn openings(store: &StoreView<'_>, slab: ElementId) -> Vec<ElementId> {
    let mut openings = store.sources(slab, &class(OPENING_IN));
    openings.sort();
    openings
}

/// The slab an opening is in.
pub fn host(store: &StoreView<'_>, opening: ElementId) -> Option<ElementId> {
    store.targets(opening, &class(OPENING_IN)).first().copied()
}

/// A slab's bottom and top elevations, from its level, offset and type, as
/// they're recomputed.
pub fn elevations(store: &StoreView<'_>, slab: ElementId) -> Option<(f64, f64)> {
    read_elevations(store, slab)
}

/// A slab type's thickness, if it has one.
pub fn thickness(store: &StoreView<'_>, slab_type: ElementId) -> Option<f64> {
    store.real(slab_type, THICKNESS)
}

/// A slab type's material, if it has one.
pub fn material(store: &StoreView<'_>, slab_type: ElementId) -> Option<ElementId> {
    store
        .targets(slab_type, &class(SLAB_MATERIAL))
        .first()
        .copied()
}

/// The slabs on `level` whose boundary has `outline` inside it: where an
/// opening drawn on that level goes. Lowest ID first.
pub fn slabs_around(store: &StoreView<'_>, level: ElementId, outline: &[Point]) -> Vec<ElementId> {
    let mut slabs: Vec<_> = store
        .sources(level, &class(ATTACHED_TO_LEVEL))
        .into_iter()
        .filter(|&id| {
            store
                .get(id)
                .is_some_and(|e| store.registry().is_a(e.class(), &class(SLAB)))
        })
        .filter(|&slab| outline::inside(outline, &read_boundary(store, slab)))
        .collect();
    slabs.sort();
    slabs
}

/// The slab classes: the slab, its type, its openings and their boundary
/// corners.
pub(crate) fn classes() -> [ClassDef; 4] {
    let slab_type = ClassDef::new(SLAB_TYPE, ClassId::DEFINITION_ELEMENT)
        .property(NAME, DataType::Text)
        .property(THICKNESS, LENGTH);
    let slab = ClassDef::new(SLAB, class(HORIZONTAL_ELEMENT)).typed_by(class(SLAB_TYPE));
    let opening = ClassDef::new(SLAB_OPENING, ClassId::GEOMETRIC_ELEMENT_3D);
    let corner = ClassDef::new(BOUNDARY_POINT, ClassId::MULTI_ASPECT)
        .property(POSITION, DataType::Integer)
        .property(X, LENGTH)
        .property(Y, LENGTH);
    [slab_type, slab, opening, corner]
}

pub(crate) fn relationships() -> [RelationshipDef; 2] {
    let opening_in = RelationshipDef::new(OPENING_IN, class(SLAB_OPENING), class(SLAB))
        .driving()
        .on_delete(OnDelete::Cascade);
    let material = RelationshipDef::new(SLAB_MATERIAL, class(SLAB_TYPE), class(MATERIAL));
    [opening_in, material]
}

/// A slab's top and bottom follow its level, offset and type.
pub(crate) fn recomputes() -> Vec<Recompute> {
    let follow = Recompute::new(class(SLAB), |store, slab| {
        let mut values = Properties::new();
        if let Some((bottom, top)) = read_elevations(store, slab.id()) {
            values.insert(BOTTOM_ELEVATION.into(), bottom.into());
            values.insert(TOP_ELEVATION.into(), top.into());
        }
        values
    });
    vec![follow]
}

/// The slab rules:
///
/// - `SlabTypeName`: every slab type has a name, and no two share one, as
///   in Revit.
/// - `SlabTypeThickness`: every slab type is thicker than 1 mm.
/// - `SlabType`: every slab has a slab type.
/// - `SlabBoundary`: every slab's and opening's boundary is a valid
///   [outline](crate::outline), with unique corner positions.
/// - `SlabOpening`: every opening is in exactly one slab, inside its
///   boundary, and apart from its other openings.
pub(crate) fn rules() -> Vec<Rule> {
    let name = Rule::new("SlabTypeName", class(SLAB_TYPE), |store, slab_type| {
        let name = match slab_type.properties().get(NAME) {
            Some(Value::Text(name)) if !name.trim().is_empty() => name,
            _ => return Err("the slab type has no name".into()),
        };
        let registry = store.registry();
        let same_name = store.elements().find(|other| {
            other.id() != slab_type.id()
                && registry.is_a(other.class(), &class(SLAB_TYPE))
                && other.properties().get(NAME) == Some(&Value::Text(name.clone()))
        });
        match same_name {
            Some(other) => Err(format!("slab type {} is also called `{name}`", other.id())),
            None => Ok(()),
        }
    });

    let thickness = Rule::new(
        "SlabTypeThickness",
        class(SLAB_TYPE),
        |store, slab_type| match store.real(slab_type.id(), THICKNESS) {
            Some(t) if t > outline::TOLERANCE => Ok(()),
            Some(t) => Err(format!("the slab type is {t} m thick")),
            None => Err("the slab type has no thickness".into()),
        },
    );

    let slab_type = Rule::new("SlabType", class(SLAB), |_, slab| match slab.type_id() {
        Some(_) => Ok(()),
        None => Err("the slab has no type".into()),
    });

    let boundary = |of: &str| {
        Rule::new("SlabBoundary", class(of), |_, element| {
            let corners = corners(element);
            let mut positions: Vec<_> = corners.iter().map(|(p, _)| *p).collect();
            positions.sort();
            if positions.windows(2).any(|w| w[0] == w[1]) {
                return Err("two boundary corners have the same position".into());
            }
            let points: Vec<_> = corners.into_iter().filter_map(|(_, p)| p).collect();
            outline::check(&points).map_err(|p| p.message().into())
        })
    };

    let opening = Rule::new("SlabOpening", class(SLAB_OPENING), |store, opening| {
        let hosts = store.targets(opening.id(), &class(OPENING_IN));
        let [slab] = hosts[..] else {
            return Err(format!("the opening is in {} slabs", hosts.len()));
        };
        let points = read_boundary(store, opening.id());
        if !outline::inside(&points, &read_boundary(store, slab)) {
            return Err("the opening isn't inside its slab's boundary".into());
        }
        let others = store.sources(slab, &class(OPENING_IN));
        let overlapping = others.into_iter().find(|&other| {
            other != opening.id() && !outline::disjoint(&points, &read_boundary(store, other))
        });
        match overlapping {
            Some(other) => Err(format!("the opening overlaps opening {other}")),
            None => Ok(()),
        }
    });

    vec![
        name,
        thickness,
        slab_type,
        boundary(SLAB),
        boundary(SLAB_OPENING),
        opening,
    ]
}

/// An element's boundary corners with their positions, in position order.
/// A corner missing a coordinate is `None`.
fn corners(element: &Element) -> Vec<(i64, Option<Point>)> {
    let class = class(BOUNDARY_POINT);
    let mut corners: Vec<_> = element
        .aspects_of(&class)
        .map(|corner| {
            let properties = corner.properties();
            let real = |name| match properties.get(name) {
                Some(Value::Real(x)) => Some(*x),
                _ => None,
            };
            let position = match properties.get(POSITION) {
                Some(Value::Integer(p)) => *p,
                _ => i64::MAX,
            };
            (position, real(X).zip(real(Y)).map(|(x, y)| [x, y]))
        })
        .collect();
    // Aspects keep their creation order, so equal positions keep it too.
    corners.sort_by_key(|(position, _)| *position);
    corners
}

pub(crate) fn read_boundary<'a>(store: &impl Read<'a>, element: ElementId) -> Vec<Point> {
    let Some(element) = store.element(element) else {
        return Vec::new();
    };
    corners(element)
        .into_iter()
        .filter_map(|(_, p)| p)
        .collect()
}

/// The bottom and top elevations, read from what drives them rather than
/// from the recomputed properties, so they're right inside a transaction
/// before it commits.
pub(crate) fn read_elevations<'a>(store: &impl Read<'a>, slab: ElementId) -> Option<(f64, f64)> {
    let level = *store.targets(slab, &class(ATTACHED_TO_LEVEL)).first()?;
    let top = store.real(level, ELEVATION)? + store.real(slab, LEVEL_OFFSET).unwrap_or(0.0);
    let thickness = store.real(slab, THICKNESS)?;
    Some((top - thickness, top))
}

/// Every slab in the store, lowest ID first.
pub fn slabs(store: &StoreView<'_>) -> Vec<ElementId> {
    let registry = store.registry();
    let mut slabs: Vec<_> = store
        .elements()
        .filter(|e| registry.is_a(e.class(), &class(SLAB)))
        .map(Element::id)
        .collect();
    slabs.sort();
    slabs
}
