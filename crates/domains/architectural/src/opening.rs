//! Doors and windows, hosted in walls (#194).
//!
//! A door or a window is placed in a wall, its **host**, through
//! [`HOSTED_BY`]. It sits [`DISTANCE_ALONG`] the wall's location line,
//! measured from the wall's start to the door's centre, in the middle of
//! the wall's thickness, so it moves with the wall, and it's deleted with
//! it. Its bottom is [`SILL_HEIGHT`] above the wall's base level: the
//! type's default sill height unless the door or window sets its own, as
//! Revit's "Sill Height".
//!
//! The wall gets the opening cut automatically: the wall's geometry is its
//! layers minus a box the size of each door and window it hosts (see
//! [`cut`]). The wall's geometry reads its doors and windows, so the cut
//! follows them when they move or their type changes.
//!
//! Two flips, as Revit's flip arrows:
//!
//! - **facing**: a door or window faces the wall's exterior side, and a
//!   door's leaf swings to the other side, into the room.
//!   [`FACING_FLIPPED`] swaps the sides.
//! - **hand**: a door's hinges are on the side of the wall's start.
//!   [`HAND_FLIPPED`] puts them on the side of its end.
//!
//! [`opening`] gives the placed door or window, and [`plan_symbol`] the
//! lines and arc a plan view draws for it, as Revit's door swing and
//! window lines.
//!
//! ```
//! use hito_architectural::opening::{self, Placement};
//! use hito_architectural::opening_type::{self, Definition};
//! use hito_architectural::wall::{self, Placement as WallPlacement};
//! use hito_architectural::wall_type::{self, Function, Layer};
//! use hito_core::{Element, ElementStore, Model, ModelKind, SchemaRegistry};
//! use hito_structural::{class, level};
//!
//! # let mut registry = SchemaRegistry::new();
//! # hito_architectural::register(&mut registry)?;
//! # let mut store = ElementStore::new(registry);
//! let mut tx = store.transaction();
//! let model = tx.insert_model(Model::new(ModelKind::Physical))?;
//! let ground = Element::new(class(level::LEVEL), model)
//!     .with_property(level::NAME, "Planta baja")
//!     .with_property(level::ELEVATION, 0.0);
//! let ground = tx.insert(ground)?;
//! let brick = wall_type::insert(&mut tx, model, "Ladrillo 20", &[Layer::new(Function::Structure, 0.2)])?;
//! let wall = wall::insert(&mut tx, model, &WallPlacement::new(brick, ground, [0.0, 0.0], [5.0, 0.0]))?;
//! let door = opening_type::insert(&mut tx, model, &Definition::door("Puerta 80 × 205", 0.8, 2.05))?;
//! let door = opening::insert(&mut tx, model, &Placement::new(door, wall, 1.0))?;
//! tx.commit()?;
//! let cut = opening::cut(&store.view(), door).unwrap();
//! assert_eq!((cut.from, cut.to, cut.bottom, cut.top), (0.6, 1.4, 0.0, 2.05));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::f64::consts::FRAC_PI_2;

use hito_core::{
    ClassDef, ClassId, DataType, Dimension, Element, ElementId, ModelId, OnDelete, Relationship,
    RelationshipDef, Rule, StoreError, StoreView, Transaction,
};
use hito_structural::level::{ATTACHED_TO_LEVEL, ELEVATION};

use crate::footprint::WallGeom;
use crate::opening_type::{self, DOOR_TYPE, Kind, Size, WINDOW_TYPE};
use crate::plan::{Point, add, cross, dot, scale, sub};
use crate::read::Read;
use crate::wall::{self, TOLERANCE};
use crate::{class, wall::WALL};

/// What doors and windows have in common. Not used on its own.
pub const OPENING: &str = "WallOpening";
/// A door, typed by a [door type](opening_type::DOOR_TYPE).
pub const DOOR: &str = "Door";
/// A window, typed by a [window type](opening_type::WINDOW_TYPE).
pub const WINDOW: &str = "Window";
/// From a door or window to the wall it's in. The wall drives it, and
/// deleting the wall deletes it.
pub const HOSTED_BY: &str = "HostedBy";

/// How far the centre is along the wall's location line, from its start.
pub const DISTANCE_ALONG: &str = "DistanceAlong";
/// The bottom's height above the wall's base level. Unset means the
/// type's [default](opening_type::SILL_HEIGHT).
pub use crate::opening_type::SILL_HEIGHT;
/// Whether a door's hinges are on the side of the wall's end instead of its
/// start.
pub const HAND_FLIPPED: &str = "HandFlipped";
/// Whether the door or window faces the wall's interior side instead of
/// its exterior side.
pub const FACING_FLIPPED: &str = "FacingFlipped";

/// A door leaf's thickness, in 3D.
pub const LEAF_THICKNESS: f64 = 0.04;
/// A window pane's thickness, in 3D.
pub const GLASS_THICKNESS: f64 = 0.006;
/// The gap around a door leaf or a window pane, inside the frame.
const CLEARANCE: f64 = 0.002;
/// How far the box cut out of a wall goes past the wall's faces, top and
/// bottom, so the cut never leaves a skin.
const CUT_MARGIN: f64 = 0.01;
/// How far outside a wall's faces a point may be and still pick the wall.
pub const PICK_DISTANCE: f64 = 0.05;

const LENGTH: DataType = DataType::Quantity(Dimension::LENGTH);

/// A door or window to add with [`insert`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// A door type or a window type: it says which is added.
    pub opening_type: ElementId,
    pub wall: ElementId,
    pub distance_along: f64,
    /// `None` keeps the type's default sill height.
    pub sill_height: Option<f64>,
    pub hand_flipped: bool,
    pub facing_flipped: bool,
}

impl Placement {
    /// A door or window of `opening_type` in `wall`, its centre
    /// `distance_along` the wall from its start, unflipped.
    pub fn new(opening_type: ElementId, wall: ElementId, distance_along: f64) -> Self {
        Self {
            opening_type,
            wall,
            distance_along,
            sill_height: None,
            hand_flipped: false,
            facing_flipped: false,
        }
    }

    pub fn with_sill_height(mut self, sill_height: f64) -> Self {
        self.sill_height = Some(sill_height);
        self
    }

    pub fn hand_flipped(mut self, flipped: bool) -> Self {
        self.hand_flipped = flipped;
        self
    }

    pub fn facing_flipped(mut self, flipped: bool) -> Self {
        self.facing_flipped = flipped;
        self
    }
}

/// Adds a door or a window, as its type says, to `model`, hosted by the
/// placement's wall. Returns it.
pub fn insert(
    tx: &mut Transaction<'_>,
    model: ModelId,
    placement: &Placement,
) -> Result<ElementId, StoreError> {
    let p = placement;
    let kind =
        opening_type::read_kind(&tx.view(), p.opening_type).ok_or(StoreError::InvalidType {
            class: class(OPENING),
            type_id: p.opening_type,
        })?;
    let mut element = Element::new(kind.class(), model)
        .with_type(p.opening_type)
        .with_property(DISTANCE_ALONG, p.distance_along)
        .with_property(HAND_FLIPPED, p.hand_flipped)
        .with_property(FACING_FLIPPED, p.facing_flipped);
    if let Some(sill) = p.sill_height {
        element = element.with_property(SILL_HEIGHT, sill);
    }
    let id = tx.insert(element)?;
    tx.relate(Relationship::new(class(HOSTED_BY), id, p.wall))?;
    Ok(id)
}

/// Moves a door or window along its wall, its centre `distance_along`
/// from the wall's start.
pub fn move_to(
    tx: &mut Transaction<'_>,
    opening: ElementId,
    distance_along: f64,
) -> Result<(), StoreError> {
    tx.set_property(opening, DISTANCE_ALONG, distance_along)
}

/// Swaps which side of the wall a door's hinges are on.
pub fn flip_hand(tx: &mut Transaction<'_>, opening: ElementId) -> Result<(), StoreError> {
    let flipped = tx.view().boolean(opening, HAND_FLIPPED);
    tx.set_property(opening, HAND_FLIPPED, !flipped)
}

/// Swaps which side of the wall a door or window faces, and so which side
/// a door swings to.
pub fn flip_facing(tx: &mut Transaction<'_>, opening: ElementId) -> Result<(), StoreError> {
    let flipped = tx.view().boolean(opening, FACING_FLIPPED);
    tx.set_property(opening, FACING_FLIPPED, !flipped)
}

/// The wall a door or window is in.
pub fn host(store: &StoreView<'_>, opening: ElementId) -> Option<ElementId> {
    store.targets(opening, &class(HOSTED_BY)).first().copied()
}

/// The doors and windows in a wall.
pub fn hosted(store: &StoreView<'_>, wall: ElementId) -> Vec<ElementId> {
    let mut hosted = store.sources(wall, &class(HOSTED_BY));
    hosted.sort();
    hosted
}

/// A placed door or window, as its geometry and plan symbol are drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Opening {
    pub id: ElementId,
    pub kind: Kind,
    pub wall: ElementId,
    pub size: Size,
    pub distance_along: f64,
    /// The centre, in plan, in the middle of the wall's thickness.
    pub centre: Point,
    /// Along the wall, from its start to its end, of length 1.
    pub along: Point,
    /// Across the wall, towards the side the door or window faces, of
    /// length 1. A door swings to the other side.
    pub facing: Point,
    /// Along the wall, from the centre towards the hinges, of length 1.
    pub hinges: Point,
    /// The bottom's and top's elevations.
    pub bottom: f64,
    pub top: f64,
    /// The host wall's thickness.
    pub wall_width: f64,
}

impl Opening {
    /// The point `along` the wall and `across` it towards the facing side,
    /// from the centre.
    fn at(&self, along: f64, across: f64) -> Point {
        add(
            self.centre,
            add(scale(self.along, along), scale(self.facing, across)),
        )
    }

    /// A rectangle in plan, from `a0` to `a1` along the wall and `c0` to
    /// `c1` across it.
    fn rectangle(&self, (a0, a1): (f64, f64), (c0, c1): (f64, f64)) -> Vec<Point> {
        vec![
            self.at(a0, c0),
            self.at(a1, c0),
            self.at(a1, c1),
            self.at(a0, c1),
        ]
    }
}

/// The door or window, `None` if it isn't complete: no host, no type with
/// a size, or a host wall that isn't complete.
pub fn opening(store: &StoreView<'_>, id: ElementId) -> Option<Opening> {
    read_opening(store, id)
}

/// The part of its wall a door or window cuts out: `from` and `to` along
/// the wall's location line, `bottom` and `top` as elevations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cut {
    pub wall: ElementId,
    pub from: f64,
    pub to: f64,
    pub bottom: f64,
    pub top: f64,
}

/// What a door or window cuts out of its wall.
pub fn cut(store: &StoreView<'_>, id: ElementId) -> Option<Cut> {
    read_opening(store, id).map(|o| cut_of(&o))
}

fn cut_of(o: &Opening) -> Cut {
    let half = o.size.width / 2.0;
    Cut {
        wall: o.wall,
        from: o.distance_along - half,
        to: o.distance_along + half,
        bottom: o.bottom,
        top: o.top,
    }
}

/// A circular arc in plan, from the angle `start` through `sweep` radians,
/// anticlockwise when positive. Angles are from the x axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arc {
    pub centre: Point,
    pub radius: f64,
    pub start: f64,
    pub sweep: f64,
}

/// What a plan view draws for a door or window, in plan coordinates, as
/// Revit's symbols at the usual scales:
///
/// - a door: its frame's jambs, its leaf open at 90° and the arc its edge
///   swings through
/// - a window: its jambs, the wall's two faces across the opening, and the
///   glass between the jambs
///
/// The wall itself is drawn with a gap the size of [`PlanSymbol::cut`]
/// where the view's cut plane passes through the opening.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanSymbol {
    /// The hole in the wall: the opening's width, the wall's thickness.
    pub cut: Vec<Point>,
    /// Closed outlines: the jambs.
    pub outlines: Vec<Vec<Point>>,
    pub lines: Vec<[Point; 2]>,
    pub arcs: Vec<Arc>,
}

/// The door or window's plan symbol.
pub fn plan_symbol(store: &StoreView<'_>, id: ElementId) -> Option<PlanSymbol> {
    let o = read_opening(store, id)?;
    let Size {
        width,
        frame_width: fw,
        frame_depth: fd,
        ..
    } = o.size;
    let (half, wall_half) = (width / 2.0, o.wall_width / 2.0);
    let cut = o.rectangle((-half, half), (-wall_half, wall_half));
    let frame = (-fd / 2.0, fd / 2.0);
    let outlines = match fw > 0.0 {
        true => vec![
            o.rectangle((-half, -half + fw), frame),
            o.rectangle((half - fw, half), frame),
        ],
        false => Vec::new(),
    };
    let clear = width - 2.0 * fw;
    let (lines, arcs) = match o.kind {
        Kind::Door => {
            let swing = scale(o.facing, -1.0);
            let hinge = add(o.centre, scale(o.hinges, half - fw));
            let hinge = add(hinge, scale(swing, fd / 2.0));
            let open = add(hinge, scale(swing, clear));
            let closed = scale(o.hinges, -1.0);
            let sweep = cross(swing, closed).signum() * FRAC_PI_2;
            let arc = Arc {
                centre: hinge,
                radius: clear,
                start: swing[1].atan2(swing[0]),
                sweep,
            };
            (vec![[hinge, open]], vec![arc])
        }
        Kind::Window => {
            let face = |across| [o.at(-half, across), o.at(half, across)];
            let inner = half - fw;
            let glass = [o.at(-inner, 0.0), o.at(inner, 0.0)];
            (vec![face(wall_half), face(-wall_half), glass], Vec::new())
        }
    };
    Some(PlanSymbol {
        cut,
        outlines,
        lines,
        arcs,
    })
}

/// Where a point in plan is on a wall, for placing a door or window there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnWall {
    /// How far along the wall's location line the point is, from its start.
    pub distance_along: f64,
    /// Whether the point is on the wall's interior side, so a door placed
    /// there swings towards it unflipped, or else on its exterior side.
    pub interior_side: bool,
}

/// Where `point` is on `wall`, `None` if the wall isn't complete.
pub fn on_wall(store: &StoreView<'_>, wall: ElementId, point: Point) -> Option<OnWall> {
    let w = WallGeom::load(store, wall)?;
    let p = sub(point, w.start);
    Some(OnWall {
        distance_along: dot(p, w.direction),
        interior_side: dot(p, w.side) < w.middle(),
    })
}

/// The wall at `point` in plan, on `level` if one is given: the one whose
/// body, or a band [`PICK_DISTANCE`] around it, holds the point, the
/// nearest to its middle if several do. How a door or window finds its
/// host from a click, or a typed point.
pub fn wall_at(store: &StoreView<'_>, level: Option<ElementId>, point: Point) -> Option<ElementId> {
    let on_level = |wall: &ElementId| {
        let structural = hito_structural::class(ATTACHED_TO_LEVEL);
        level.is_none_or(|l| store.targets(*wall, &structural).contains(&l))
    };
    let distance = |wall: ElementId| {
        let w = WallGeom::load(store, wall)?;
        let p = sub(point, w.start);
        let along = dot(p, w.direction);
        let across = (dot(p, w.side) - w.middle()).abs();
        let inside = (0.0..=w.length).contains(&along) && across <= w.width() / 2.0 + PICK_DISTANCE;
        inside.then_some(across)
    };
    wall::walls(store)
        .into_iter()
        .filter(on_level)
        .filter_map(|w| Some((distance(w)?, w)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, w)| w)
}

pub(crate) fn read_opening<'a>(store: &impl Read<'a>, id: ElementId) -> Option<Opening> {
    if !store.is_a(id, &class(OPENING)) {
        return None;
    }
    let kind = opening_type::read_kind(store, store.type_of(id)?)?;
    let size = opening_type::read_size(store, store.type_of(id)?)?;
    let wall = *store.targets(id, &class(HOSTED_BY)).first()?;
    let w = WallGeom::load(store, wall)?;
    let distance_along = store.real(id, DISTANCE_ALONG)?;
    let sill = store.real(id, SILL_HEIGHT).unwrap_or(size.sill_height);
    let level = store.targets(wall, &hito_structural::class(ATTACHED_TO_LEVEL));
    let base = match level.first() {
        Some(&level) => store.real(level, ELEVATION).unwrap_or(0.0),
        None => w.bottom,
    };
    let centre = add(
        w.start,
        add(
            scale(w.direction, distance_along),
            scale(w.side, w.middle()),
        ),
    );
    let facing = match store.boolean(id, FACING_FLIPPED) {
        true => scale(w.side, -1.0),
        false => w.side,
    };
    let hinges = match store.boolean(id, HAND_FLIPPED) {
        true => w.direction,
        false => scale(w.direction, -1.0),
    };
    Some(Opening {
        id,
        kind,
        wall,
        size,
        distance_along,
        centre,
        along: w.direction,
        facing,
        hinges,
        bottom: base + sill,
        top: base + sill + size.height,
        wall_width: w.width(),
    })
}

/// The boxes a wall's doors and windows cut out of it, for the wall's
/// geometry: each the opening's size in elevation, through the whole wall.
pub(crate) fn wall_voids<'a>(store: &impl Read<'a>, wall: ElementId) -> Vec<hito_geometry::Solid> {
    let Some(w) = WallGeom::load(store, wall) else {
        return Vec::new();
    };
    let mut openings = store.sources(wall, &class(HOSTED_BY));
    openings.sort();
    openings
        .into_iter()
        .filter_map(|id| read_opening(store, id))
        .filter_map(|o| {
            let cut = cut_of(&o);
            // Where the cut reaches the wall's bottom or top, it goes past
            // it, so no sliver of wall is left.
            let bottom = match cut.bottom <= w.bottom + TOLERANCE {
                true => w.bottom - CUT_MARGIN,
                false => cut.bottom,
            };
            let top = match cut.top >= w.top - TOLERANCE {
                true => w.top + CUT_MARGIN,
                false => cut.top,
            };
            (top > bottom).then_some(())?;
            let depth = o.wall_width + 2.0 * CUT_MARGIN;
            let start = add(o.centre, scale(o.along, -o.size.width / 2.0));
            let (width, height) = (o.size.width, top - bottom);
            let profile = [[0.0, 0.0], [width, 0.0], [width, height], [0.0, height]];
            let profile = hito_geometry::Profile::polygon(profile.to_vec());
            Some(extrusion(&o, start, bottom, depth, profile))
        })
        .collect()
}

/// A door or window's own geometry: its frame, and its leaf, closed, or
/// its glass.
pub(crate) fn body(o: &Opening) -> Option<hito_geometry::Solid> {
    use hito_geometry::Solid;
    let Size {
        width,
        height,
        frame_width: fw,
        frame_depth: fd,
        ..
    } = o.size;
    let (h, w) = (width / 2.0, height);
    let mut parts = Vec::new();
    if fw > 0.0 {
        let outer = [[-h, 0.0], [h, 0.0], [h, w], [-h, w]];
        let profile = match o.kind {
            Kind::Door => hito_geometry::Profile::polygon(vec![
                [-h, 0.0],
                [-h + fw, 0.0],
                [-h + fw, w - fw],
                [h - fw, w - fw],
                [h - fw, 0.0],
                [h, 0.0],
                [h, w],
                [-h, w],
            ]),
            Kind::Window => hito_geometry::Profile::Polygon {
                outline: outer.to_vec(),
                holes: vec![vec![
                    [-h + fw, fw],
                    [h - fw, fw],
                    [h - fw, w - fw],
                    [-h + fw, w - fw],
                ]],
            },
        };
        parts.push(extrusion(o, o.centre, o.bottom, fd, profile));
    }
    // The leaf or the pane fills the frame, with a small gap round it.
    let (thickness, bottom) = match o.kind {
        Kind::Door => (LEAF_THICKNESS.min(fd), CLEARANCE.min(fw)),
        Kind::Window => (GLASS_THICKNESS.min(fd), fw + CLEARANCE),
    };
    let inner = h - fw - CLEARANCE;
    let top = w - fw - CLEARANCE;
    if inner > 0.0 && top > bottom {
        let panel = [
            [-inner, bottom],
            [inner, bottom],
            [inner, top],
            [-inner, top],
        ];
        let profile = hito_geometry::Profile::polygon(panel.to_vec());
        parts.push(extrusion(o, o.centre, o.bottom, thickness, profile));
    }
    match parts.len() {
        0 => None,
        1 => parts.pop(),
        _ => Some(Solid::Union(parts)),
    }
}

/// A profile in the wall's plane (x along the wall, y up), at `origin` in
/// plan and `bottom` in elevation, extruded `depth` across the wall,
/// centred on the wall's middle.
fn extrusion(
    o: &Opening,
    origin: Point,
    bottom: f64,
    depth: f64,
    profile: hito_geometry::Profile,
) -> hito_geometry::Solid {
    let [dx, dy] = o.along;
    // x along the wall, z to its right, so y is up.
    let z = [dy, -dx];
    let origin = add(origin, scale(z, -depth / 2.0));
    let frame = hito_geometry::Frame::new(
        [origin[0], origin[1], bottom],
        [z[0], z[1], 0.0],
        [dx, dy, 0.0],
    )
    .expect("the wall's direction is a unit vector");
    hito_geometry::Solid::Extrusion {
        profile,
        frame,
        depth,
    }
}

/// The door and window classes.
pub(crate) fn classes() -> [ClassDef; 3] {
    let base = ClassDef::new(OPENING, ClassId::GEOMETRIC_ELEMENT_3D)
        .property(DISTANCE_ALONG, LENGTH)
        .property(SILL_HEIGHT, LENGTH)
        .property(HAND_FLIPPED, DataType::Boolean)
        .property(FACING_FLIPPED, DataType::Boolean);
    let door = ClassDef::new(DOOR, class(OPENING)).typed_by(class(DOOR_TYPE));
    let window = ClassDef::new(WINDOW, class(OPENING)).typed_by(class(WINDOW_TYPE));
    [base, door, window]
}

pub(crate) fn relationships() -> [RelationshipDef; 1] {
    let hosted_by = RelationshipDef::new(HOSTED_BY, class(OPENING), class(WALL))
        .driving()
        .on_delete(OnDelete::Cascade);
    [hosted_by]
}

/// The door and window rules:
///
/// - `OpeningHost`: the door or window is in exactly one wall, has a type
///   and a distance along the wall.
/// - `OpeningInWall`, a warning: it fits in its wall, along it and between
///   its base and top. When a wall is shortened past a door, the door
///   stays, and only what's in the wall is cut.
/// - `OpeningOverlap`, a warning: it doesn't overlap another door or
///   window in the same wall.
pub(crate) fn rules() -> Vec<Rule> {
    let host = Rule::new("OpeningHost", class(OPENING), |store, opening| {
        let hosts = store.targets(opening.id(), &class(HOSTED_BY)).len();
        if hosts != 1 {
            return Err(format!("the door or window is in {hosts} walls"));
        }
        if opening.type_id().is_none() {
            return Err("the door or window has no type".into());
        }
        match store.real(opening.id(), DISTANCE_ALONG) {
            Some(x) if x.is_finite() => Ok(()),
            _ => Err("the door or window has no distance along its wall".into()),
        }
    });

    let in_wall = Rule::new("OpeningInWall", class(OPENING), |store, opening| {
        let Some(o) = read_opening(store, opening.id()) else {
            return Ok(()); // incomplete: other rules say why
        };
        let Some(w) = WallGeom::load(store, o.wall) else {
            return Ok(());
        };
        let cut = cut_of(&o);
        if cut.from < -TOLERANCE || cut.to > w.length + TOLERANCE {
            return Err(format!(
                "it goes from {:.3} to {:.3} m along a {:.3} m wall",
                cut.from, cut.to, w.length
            ));
        }
        if cut.bottom < w.bottom - TOLERANCE || cut.top > w.top + TOLERANCE {
            return Err(format!(
                "it goes from {:.3} to {:.3} m up a wall from {:.3} to {:.3} m",
                cut.bottom, cut.top, w.bottom, w.top
            ));
        }
        Ok(())
    })
    .warning();

    let overlap = Rule::new("OpeningOverlap", class(OPENING), |store, opening| {
        let Some(mine) = read_opening(store, opening.id()).map(|o| cut_of(&o)) else {
            return Ok(());
        };
        let others = store.sources(mine.wall, &class(HOSTED_BY));
        let overlapping = others
            .into_iter()
            .filter(|&o| o != opening.id())
            .find(|&o| {
                read_opening(store, o).is_some_and(|other| {
                    let other = cut_of(&other);
                    mine.from < other.to - TOLERANCE
                        && other.from < mine.to - TOLERANCE
                        && mine.bottom < other.top - TOLERANCE
                        && other.bottom < mine.top - TOLERANCE
                })
            });
        match overlapping {
            Some(other) => Err(format!("it overlaps {other} in the same wall")),
            None => Ok(()),
        }
    })
    .warning();

    vec![host, in_wall, overlap]
}
