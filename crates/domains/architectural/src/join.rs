//! Wall joins: walls that touch join on their own, and clean up layer by
//! layer (#193).
//!
//! When a wall is drawn or moved, [`update`] finds the walls it touches and
//! records each join as a relationship, as IFC's
//! `IfcRelConnectsPathElements` does with its `ATSTART`, `ATEND` and
//! `ATPATH` connections:
//!
//! - **Ends that meet** (within 1 mm) join both ways: each wall's
//!   [`JOINED_AT_START`] or [`JOINED_AT_END`] points to the other. Two
//!   walls make a corner, or a straight run if they're in line; three or
//!   more make a junction.
//! - **An end on another wall's side**, inside its thickness, joins one
//!   way: the end points to the wall it abuts (a T).
//! - **Walls that cross**, away from their ends, are linked by [`CROSSES`]
//!   (an X).
//!
//! Only walls that stand at some common height join. The relationships
//! don't drive anything: each wall's geometry reads the walls it's joined
//! to, so moving one wall regenerates its neighbours' joins too (#28).
//!
//! How each join is cut, so that layers meet layers and nothing overlaps:
//!
//! - **Corner (L):** both walls are cut along the line through the corners
//!   of their faces. Walls of the same type meet on the bisector, every
//!   layer meeting the same layer of the other wall, as Revit's cleaned-up
//!   joins look. A straight run is cut square.
//! - **T:** the abutting wall's core runs on to the other wall's core, and
//!   its finishes stop at the other wall's face. The other wall's finishes
//!   on that side are cut where the core passes, so plaster meets plaster
//!   and brick meets brick.
//! - **X:** the wall with the thicker core goes through, and the other is
//!   cut as two walls abutting it.
//! - **Junctions of three or more ends:** two walls in line go through and
//!   the others abut them, as a T.
//!
//! A join only applies where both walls stand: a wall joined to a lower one
//! is cut into slices at the lower wall's top.

use std::collections::{BTreeSet, HashMap};

use hito_core::{ElementId, Relationship, RelationshipDef, StoreError, Transaction};

use crate::class;
use crate::footprint::{End, WallGeom};
use crate::wall::{self, WALL};

/// From a wall to a wall its start is joined to.
pub const JOINED_AT_START: &str = "JoinedAtStart";
/// From a wall to a wall its end is joined to.
pub const JOINED_AT_END: &str = "JoinedAtEnd";
/// Between two walls that cross, from the one with the smaller ID.
pub const CROSSES: &str = "Crosses";

pub(crate) fn relationships() -> [RelationshipDef; 3] {
    [JOINED_AT_START, JOINED_AT_END, CROSSES]
        .map(|name| RelationshipDef::new(name, class(WALL), class(WALL)))
}

/// Finds the joins of `walls` again, from where they are now: adds the
/// joins they now make and removes the ones they no longer do. Wall
/// placement calls it; anything that moves a wall's ends, changes its
/// type, levels or location line, should call it too.
pub fn update(tx: &mut Transaction<'_>, walls: &[ElementId]) -> Result<(), StoreError> {
    let (wanted, existing) = {
        let view = tx.view();
        let geoms: HashMap<ElementId, WallGeom> = wall::walls(&view)
            .into_iter()
            .filter_map(|id| Some((id, WallGeom::load(&view, id)?)))
            .collect();
        let mut wanted = BTreeSet::new();
        let mut existing = BTreeSet::new();
        for &id in walls {
            let relationships = view.relationships_from(id).chain(view.relationships_to(id));
            existing.extend(relationships.filter(|r| is_join(r)).cloned());
            let Some(me) = geoms.get(&id) else {
                continue;
            };
            for other in geoms.values() {
                if other.id != id && me.overlaps_vertically(other) {
                    wanted.extend(between(me, other));
                }
            }
        }
        (wanted, existing)
    };
    for r in existing.difference(&wanted) {
        tx.unrelate(r)?;
    }
    for r in wanted.difference(&existing) {
        tx.relate(r.clone())?;
    }
    Ok(())
}

fn is_join(r: &Relationship) -> bool {
    [JOINED_AT_START, JOINED_AT_END, CROSSES]
        .iter()
        .any(|name| r.class() == &class(name))
}

/// The joins between two walls, both ways.
fn between(a: &WallGeom, b: &WallGeom) -> Vec<Relationship> {
    let joined = |w: &WallGeom, e: End, to: &WallGeom| {
        Relationship::new(class(e.relationship()), w.id, to.id)
    };
    let parallel = {
        let c = crate::plan::cross(a.direction, b.direction);
        c.abs() < crate::plan::PARALLEL
    };
    let mut found = Vec::new();
    for (me, other) in [(a, b), (b, a)] {
        for e in End::BOTH {
            let p = me.point(e);
            match other.end_at(p) {
                // Ends that meet, unless the walls overlap.
                Some(other_end) => {
                    let along = crate::plan::dot(me.inward(e), other.inward(other_end));
                    if !(parallel && along > 0.0) {
                        found.push(joined(me, e, other));
                    }
                }
                None if !parallel && other.holds(p) => found.push(joined(me, e, other)),
                None => {}
            }
        }
    }
    if !parallel && a.crossing(b).is_some() {
        let (from, to) = if a.id < b.id { (a, b) } else { (b, a) };
        found.push(Relationship::new(class(CROSSES), from.id, to.id));
    }
    found
}

/// The walls `wall`'s end `end` is joined to.
pub fn joined_at(store: &hito_core::StoreView<'_>, wall: ElementId, end: End) -> Vec<ElementId> {
    let class = class(end.relationship());
    store
        .relationships_from(wall)
        .filter(|r| r.class() == &class)
        .map(Relationship::target)
        .collect()
}
