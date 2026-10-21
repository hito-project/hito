//! Walls: drawn as straight lines on a level, standing up to a level or to
//! a height (#193).
//!
//! A wall is a level-based element (see
//! [`hito_structural::level`]): it stands on its base level, raised by the
//! level offset (Revit's "Base Offset"). Its top is one of:
//!
//! - **a top level**, through [`TOP_LEVEL`], raised by [`TOP_OFFSET`]
//!   (Revit's "Top Constraint: Up to level")
//! - **unconnected**: [`HEIGHT`] above its base (Revit's "Unconnected
//!   Height")
//!
//! Its top and base can also be **attached** to a horizontal element, such
//! as a slab, through [`TOP_ATTACHED_TO`] and [`BASE_ATTACHED_TO`]: the top
//! then stops under it, or the base stands on it, whatever the levels say,
//! as Revit's "Attach Top/Base". All of these drive the wall, so it follows
//! its levels and slabs in the same transaction that moves them, and
//! [`BOTTOM_ELEVATION`], [`TOP_ELEVATION`] and, when the top is constrained,
//! [`HEIGHT`] are recomputed.
//!
//! In plan, the wall is its **location line**, from [`START_X`],
//! [`START_Y`] to [`END_X`], [`END_Y`]. Its type's layers lie across it as
//! [`LOCATION_LINE`] says (the wall's centreline by default, as in Revit).
//! The exterior face is on the left of the line, looking from its start to
//! its end, so walls drawn clockwise face out, as in Revit; [`FLIPPED`]
//! swaps the faces about the location line.
//!
//! A wall is [`STRUCTURAL`] or not, as Revit's "Structural" parameter: the
//! same wall either way, which IFC export (#51) writes as a load-bearing
//! `IfcWall`. A structural wall is expected to be concrete: its
//! [`structural_material`] should be a structural
//! [`ConcreteMaterial`](hito_structural::material::CONCRETE), or the commit
//! warns.
//!
//! ```
//! use hito_architectural::wall::{self, Placement, Top};
//! use hito_architectural::wall_type::{self, Function, Layer};
//! use hito_core::{Element, ElementStore, Model, ModelKind, SchemaRegistry, Value};
//! use hito_structural::{class, level};
//!
//! # let mut registry = SchemaRegistry::new();
//! # hito_architectural::register(&mut registry)?;
//! # let mut store = ElementStore::new(registry);
//! let mut tx = store.transaction();
//! let model = tx.insert_model(Model::new(ModelKind::Physical))?;
//! let level = |name, elevation| {
//!     Element::new(class(level::LEVEL), model)
//!         .with_property(level::NAME, name)
//!         .with_property(level::ELEVATION, elevation)
//! };
//! let ground = tx.insert(level("Planta baja", 0.0))?;
//! let first = tx.insert(level("Primer piso", 2.8))?;
//! let concrete = wall_type::insert(&mut tx, model, "Hormigón 20", &[Layer::new(Function::Structure, 0.2)])?;
//! let placement = Placement::new(concrete, ground, [0.0, 0.0], [5.0, 0.0])
//!     .with_top(Top::Level { level: first, offset: 0.0 });
//! let wall = wall::insert(&mut tx, model, &placement)?;
//! tx.commit()?;
//! assert_eq!(store.property(wall, wall::HEIGHT), Some(&Value::Real(2.8)));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use hito_core::{
    ClassDef, DataType, Dimension, Element, ElementId, ModelId, Properties, Recompute,
    Relationship, RelationshipDef, Rule, StoreError, StoreView, Transaction,
};
use hito_structural::level::{
    ATTACHED_TO_LEVEL, BOTTOM_ELEVATION as SURFACE_BOTTOM, ELEVATION, HORIZONTAL_ELEMENT, LEVEL,
    LEVEL_BASED_ELEMENT, LEVEL_OFFSET, TOP_ELEVATION as SURFACE_TOP,
};
use hito_structural::material::CONCRETE;

use crate::plan::{Point, distance};
use crate::read::Read;
use crate::wall_type::{self, Function};
use crate::{class, join};

/// A wall.
pub const WALL: &str = "Wall";
/// From a wall to its top level, which drives it. Deleting the level
/// leaves the wall unconnected, at the height it had.
pub const TOP_LEVEL: &str = "TopLevel";
/// From a wall to the horizontal element its top is attached to, such as
/// the slab above. Deleting it detaches the wall.
pub const TOP_ATTACHED_TO: &str = "TopAttachedTo";
/// From a wall to the horizontal element its base stands on.
pub const BASE_ATTACHED_TO: &str = "BaseAttachedTo";

/// The location line's start and end, in plan.
pub const START_X: &str = "StartX";
pub const START_Y: &str = "StartY";
pub const END_X: &str = "EndX";
pub const END_Y: &str = "EndY";
/// Which line across the wall its location line is, as one of
/// [`LocationLine`]'s names. Unset means [`LocationLine::WallCenterline`].
pub const LOCATION_LINE: &str = "LocationLine";
/// How far the top is above the top level. Negative is below. Unused
/// without a top level.
pub const TOP_OFFSET: &str = "TopOffset";
/// The wall's height. Set it for an unconnected wall; with a top level or
/// an attached top it's recomputed.
pub const HEIGHT: &str = "Height";
/// The wall's base above the project's origin: its base level plus the
/// level offset, or the top of what its base is attached to. Recomputed.
pub const BOTTOM_ELEVATION: &str = "BottomElevation";
/// The wall's top above the project's origin. Recomputed.
pub const TOP_ELEVATION: &str = "TopElevation";
/// Whether the exterior face is on the right of the location line instead
/// of the left.
pub const FLIPPED: &str = "Flipped";
/// Whether the wall is structural, as Revit's "Structural" parameter.
pub const STRUCTURAL: &str = "Structural";
/// How far the base is above the base level, as Revit's "Base Offset":
/// the level-based element's level offset.
pub use hito_structural::level::LEVEL_OFFSET as BASE_OFFSET;

/// How close two points must be to count as one: 1 mm. Wall ends this
/// close join, and a wall must be longer.
pub const TOLERANCE: f64 = 0.001;

const LENGTH: DataType = DataType::Quantity(Dimension::LENGTH);

/// Which line across a wall its location line is, with Revit's options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LocationLine {
    /// Halfway between the faces.
    #[default]
    WallCenterline,
    /// Halfway across the core.
    CoreCenterline,
    /// On the exterior face.
    FinishFaceExterior,
    /// On the interior face.
    FinishFaceInterior,
    /// On the core's exterior side.
    CoreFaceExterior,
    /// On the core's interior side.
    CoreFaceInterior,
}

impl LocationLine {
    pub const ALL: [LocationLine; 6] = [
        Self::WallCenterline,
        Self::CoreCenterline,
        Self::FinishFaceExterior,
        Self::FinishFaceInterior,
        Self::CoreFaceExterior,
        Self::CoreFaceInterior,
    ];

    /// The name stored in the [`LOCATION_LINE`] property.
    pub fn name(self) -> &'static str {
        match self {
            Self::WallCenterline => "WallCenterline",
            Self::CoreCenterline => "CoreCenterline",
            Self::FinishFaceExterior => "FinishFaceExterior",
            Self::FinishFaceInterior => "FinishFaceInterior",
            Self::CoreFaceExterior => "CoreFaceExterior",
            Self::CoreFaceInterior => "CoreFaceInterior",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.name() == name)
    }
}

/// Where a wall's top is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Top {
    /// This high above its base.
    Unconnected { height: f64 },
    /// At this level, raised by `offset`.
    Level { level: ElementId, offset: f64 },
}

/// A wall to add with [`insert`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub wall_type: ElementId,
    pub base_level: ElementId,
    pub base_offset: f64,
    pub top: Top,
    pub start: Point,
    pub end: Point,
    pub location_line: LocationLine,
    pub flipped: bool,
    pub structural: bool,
}

impl Placement {
    /// A wall of `wall_type` on `base_level` from `start` to `end`, 3 m
    /// high and unconnected, on its centreline, as Revit draws by default.
    pub fn new(wall_type: ElementId, base_level: ElementId, start: Point, end: Point) -> Self {
        Self {
            wall_type,
            base_level,
            base_offset: 0.0,
            top: Top::Unconnected { height: 3.0 },
            start,
            end,
            location_line: LocationLine::default(),
            flipped: false,
            structural: false,
        }
    }

    pub fn with_top(mut self, top: Top) -> Self {
        self.top = top;
        self
    }

    pub fn with_base_offset(mut self, offset: f64) -> Self {
        self.base_offset = offset;
        self
    }

    pub fn with_location_line(mut self, location_line: LocationLine) -> Self {
        self.location_line = location_line;
        self
    }

    pub fn flipped(mut self, flipped: bool) -> Self {
        self.flipped = flipped;
        self
    }

    pub fn structural(mut self, structural: bool) -> Self {
        self.structural = structural;
        self
    }

    /// The same wall, from `start` to `end`: the next wall of a chain.
    pub fn along(mut self, start: Point, end: Point) -> Self {
        self.start = start;
        self.end = end;
        self
    }
}

/// Adds a wall to `model`, and [joins](join::update) it to the walls it
/// touches. Returns the wall.
pub fn insert(
    tx: &mut Transaction<'_>,
    model: ModelId,
    placement: &Placement,
) -> Result<ElementId, StoreError> {
    let p = placement;
    let mut element = Element::new(class(WALL), model)
        .with_type(p.wall_type)
        .with_property(START_X, p.start[0])
        .with_property(START_Y, p.start[1])
        .with_property(END_X, p.end[0])
        .with_property(END_Y, p.end[1])
        .with_property(LEVEL_OFFSET, p.base_offset)
        .with_property(LOCATION_LINE, p.location_line.name())
        .with_property(FLIPPED, p.flipped)
        .with_property(STRUCTURAL, p.structural);
    element = match p.top {
        Top::Unconnected { height } => element.with_property(HEIGHT, height),
        Top::Level { offset, .. } => element.with_property(TOP_OFFSET, offset),
    };
    let wall = tx.insert(element)?;
    let structural = hito_structural::class;
    tx.relate(Relationship::new(
        structural(ATTACHED_TO_LEVEL),
        wall,
        p.base_level,
    ))?;
    if let Top::Level { level, .. } = p.top {
        tx.relate(Relationship::new(class(TOP_LEVEL), wall, level))?;
    }
    join::update(tx, &[wall])?;
    Ok(wall)
}

/// Swaps the wall's exterior and interior faces about its location line,
/// as Revit's flip arrows.
pub fn flip(tx: &mut Transaction<'_>, wall: ElementId) -> Result<(), StoreError> {
    let flipped = tx.view().boolean(wall, FLIPPED);
    tx.set_property(wall, FLIPPED, !flipped)
}

/// The wall's location line, from its start to its end.
pub fn line(store: &StoreView<'_>, wall: ElementId) -> Option<(Point, Point)> {
    read_line(store, wall)
}

/// The wall's interior face in plan, along the wall from its start to its
/// end: the line a slab boundary picked from the wall follows, as Revit's
/// Pick Walls does by default.
///
/// The exterior is on the wall's left, looking from its start to its end,
/// unless the wall is flipped (`Flipped`), so the interior face is on its
/// right, or on its left when flipped. Its distance from the location line
/// comes from the location line's position across the type's layers.
pub fn interior_face(store: &StoreView<'_>, wall: ElementId) -> Option<(Point, Point)> {
    let w = crate::footprint::WallGeom::load(store, wall)?;
    let offset = crate::plan::scale(w.side, w.interior());
    Some((
        crate::plan::add(w.start, offset),
        crate::plan::add(w.end, offset),
    ))
}

/// The wall's base and top elevations, from its levels, offsets and
/// attachments, as they're recomputed.
pub fn elevations(store: &StoreView<'_>, wall: ElementId) -> Option<(f64, f64)> {
    read_elevations(store, wall)
}

/// The material a structural wall carries: that of its type's first
/// structural layer, or of its first core layer.
pub fn structural_material(store: &StoreView<'_>, wall: ElementId) -> Option<ElementId> {
    let layers = wall_type::layers(store, store.get(wall)?.type_id()?);
    let core = &layers[wall_type::core(&layers)];
    let structure = core.iter().find(|l| l.function == Function::Structure);
    structure.or(core.first())?.material
}

/// The wall class.
pub(crate) fn classes() -> [ClassDef; 1] {
    let wall = ClassDef::new(WALL, hito_structural::class(LEVEL_BASED_ELEMENT))
        .typed_by(class(crate::wall_type::WALL_TYPE))
        .property(START_X, LENGTH)
        .property(START_Y, LENGTH)
        .property(END_X, LENGTH)
        .property(END_Y, LENGTH)
        .property(LOCATION_LINE, DataType::Text)
        .property(TOP_OFFSET, LENGTH)
        .property(HEIGHT, LENGTH)
        .property(BOTTOM_ELEVATION, LENGTH)
        .property(TOP_ELEVATION, LENGTH)
        .property(FLIPPED, DataType::Boolean)
        .property(STRUCTURAL, DataType::Boolean);
    [wall]
}

pub(crate) fn relationships() -> [RelationshipDef; 3] {
    let structural = hito_structural::class;
    let top_level = RelationshipDef::new(TOP_LEVEL, class(WALL), structural(LEVEL)).driving();
    let attach =
        |name| RelationshipDef::new(name, class(WALL), structural(HORIZONTAL_ELEMENT)).driving();
    [top_level, attach(TOP_ATTACHED_TO), attach(BASE_ATTACHED_TO)]
}

/// A wall's elevations follow its levels, offsets and attachments.
pub(crate) fn recomputes() -> Vec<Recompute> {
    let follow = Recompute::new(class(WALL), |store, wall| {
        let mut values = Properties::new();
        if let Some((bottom, top)) = read_elevations(store, wall.id()) {
            values.insert(BOTTOM_ELEVATION.into(), bottom.into());
            values.insert(TOP_ELEVATION.into(), top.into());
            if top_is_constrained(store, wall.id()) {
                values.insert(HEIGHT.into(), (top - bottom).into());
            }
        }
        values
    });
    vec![follow]
}

/// The wall rules:
///
/// - `WallLine`: the wall has both ends, more than 1 mm apart.
/// - `WallType`: the wall has a wall type, with a thickness.
/// - `WallHeight`: the wall's top is above its base.
/// - `WallConstraints`: at most one top level, one attached top and one
///   attached base, and a known location line.
/// - `StructuralWallConcrete` (a warning): a structural wall's structural
///   material, from its type, is a concrete material, as #193 asks.
pub(crate) fn rules() -> Vec<Rule> {
    let line = Rule::new("WallLine", class(WALL), |store, wall| {
        match read_line(store, wall.id()) {
            None => Err("the wall is missing an end point".into()),
            Some((start, end)) if distance(start, end) <= TOLERANCE => {
                Err("the wall is shorter than 1 mm".into())
            }
            Some(_) => Ok(()),
        }
    });

    let wall_type = Rule::new("WallType", class(WALL), |store, wall| {
        let Some(wall_type) = wall.type_id() else {
            return Err("the wall has no type".into());
        };
        match wall_type::width(store, wall_type) {
            width if width > 0.0 => Ok(()),
            _ => Err("the wall's type has no thickness".into()),
        }
    });

    let height = Rule::new(
        "WallHeight",
        class(WALL),
        |store, wall| match read_elevations(store, wall.id()) {
            None => Err("the wall has no base level or no height".into()),
            Some((bottom, top)) if top - bottom <= TOLERANCE => Err(format!(
                "the wall's top ({top} m) isn't above its base ({bottom} m)"
            )),
            Some(_) => Ok(()),
        },
    );

    let constraints = Rule::new("WallConstraints", class(WALL), |store, wall| {
        for name in [TOP_LEVEL, TOP_ATTACHED_TO, BASE_ATTACHED_TO] {
            let n = store.targets(wall.id(), &class(name)).len();
            if n > 1 {
                return Err(format!("the wall has {n} `{name}` relationships"));
            }
        }
        match store.text(wall.id(), LOCATION_LINE) {
            Some(name) if LocationLine::from_name(name).is_none() => {
                Err(format!("`{name}` isn't a location line"))
            }
            _ => Ok(()),
        }
    });

    // A warning, not an error: the wall can still be drawn and marked
    // structural before its type has a material.
    let concrete = Rule::new("StructuralWallConcrete", class(WALL), |store, wall| {
        if !store.boolean(wall.id(), STRUCTURAL) {
            return Ok(());
        }
        let concrete = hito_structural::class(CONCRETE);
        match structural_material(store, wall.id()) {
            Some(m) if store.is_a(m, &concrete) => Ok(()),
            Some(m) => Err(format!("the structural wall's material {m} isn't concrete")),
            None => Err("the structural wall's structural layer has no material".into()),
        }
    })
    .warning();

    vec![line, wall_type, height, constraints, concrete]
}

pub(crate) fn read_line<'a>(store: &impl Read<'a>, wall: ElementId) -> Option<(Point, Point)> {
    let get = |name| store.real(wall, name);
    Some(([get(START_X)?, get(START_Y)?], [get(END_X)?, get(END_Y)?]))
}

pub(crate) fn read_location_line<'a>(store: &impl Read<'a>, wall: ElementId) -> LocationLine {
    let name = store.text(wall, LOCATION_LINE);
    name.and_then(LocationLine::from_name).unwrap_or_default()
}

/// The base and top elevations, read from what drives them rather than
/// from the recomputed properties, so they're right inside a transaction
/// before it commits.
pub(crate) fn read_elevations<'a>(store: &impl Read<'a>, wall: ElementId) -> Option<(f64, f64)> {
    let structural = hito_structural::class;
    let level_elevation = |level| store.real(level, ELEVATION).unwrap_or(0.0);
    let attached = |name| store.targets(wall, &class(name)).first().copied();

    let bottom = match attached(BASE_ATTACHED_TO) {
        Some(surface) => store.real(surface, SURFACE_TOP)?,
        None => {
            let base = store.targets(wall, &structural(ATTACHED_TO_LEVEL));
            let offset = store.real(wall, LEVEL_OFFSET).unwrap_or(0.0);
            level_elevation(*base.first()?) + offset
        }
    };
    let top_level = store.targets(wall, &class(TOP_LEVEL)).first().copied();
    let top = match (attached(TOP_ATTACHED_TO), top_level) {
        (Some(surface), _) => store.real(surface, SURFACE_BOTTOM)?,
        (None, Some(level)) => level_elevation(level) + store.real(wall, TOP_OFFSET).unwrap_or(0.0),
        (None, None) => bottom + store.real(wall, HEIGHT)?,
    };
    Some((bottom, top))
}

fn top_is_constrained<'a>(store: &impl Read<'a>, wall: ElementId) -> bool {
    [TOP_LEVEL, TOP_ATTACHED_TO]
        .into_iter()
        .any(|name| !store.targets(wall, &class(name)).is_empty())
}

/// Every wall in the store.
pub(crate) fn walls(store: &StoreView<'_>) -> Vec<ElementId> {
    let registry = store.registry();
    let mut walls: Vec<_> = store
        .elements()
        .filter(|e| registry.is_a(e.class(), &class(WALL)))
        .map(Element::id)
        .collect();
    walls.sort();
    walls
}
