//! A wall's footprint: each layer's outline in plan, cut where the wall
//! joins others.
//!
//! The geometry generator extrudes these outlines, and plan views and
//! quantities can read them too. How each kind of junction is cut is
//! described in [`join`](crate::join).

use std::collections::BTreeSet;
use std::ops::Range;

use hito_core::{ElementId, StoreView};

use crate::class;
use crate::join::{CROSSES, JOINED_AT_END, JOINED_AT_START};
use crate::plan::{
    HalfPlane, Line, PARALLEL, Point, add, clip, cross, distance, dot, left, scale, sub,
};
use crate::read::Read;
use crate::wall::{self, LocationLine, TOLERANCE};
use crate::wall_type::{self, Function, LayerInfo};

/// A wall's layers in plan, in horizontal slices.
///
/// Walls joined to walls of the same height have one slice. Where a joined
/// wall is lower or higher, the wall is cut into slices at its top and
/// bottom, since the join only applies where both walls are.
#[derive(Debug, Clone, PartialEq)]
pub struct Footprint {
    /// From the bottom up, without gaps.
    pub slices: Vec<Slice>,
}

/// A horizontal slice of a wall, from `bottom` to `top`, with the same
/// outlines all the way up.
#[derive(Debug, Clone, PartialEq)]
pub struct Slice {
    pub bottom: f64,
    pub top: f64,
    /// The type's layers, from the exterior face to the interior one.
    pub layers: Vec<LayerOutline>,
}

/// One layer's outline in plan: one or more convex polygons. A layer is
/// split in two where another wall's core passes through it.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerOutline {
    pub layer: ElementId,
    pub function: Function,
    pub material: Option<ElementId>,
    pub polygons: Vec<Vec<Point>>,
}

/// The wall's footprint, `None` if the wall isn't complete: no line, no
/// type with a thickness, or no base and top.
pub fn footprint(store: &StoreView<'_>, wall: ElementId) -> Option<Footprint> {
    compute(store, wall)
}

/// One end of a wall's location line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum End {
    Start,
    End,
}

impl End {
    pub const BOTH: [End; 2] = [End::Start, End::End];

    /// The relationship type that joins this end to another wall.
    pub fn relationship(self) -> &'static str {
        match self {
            End::Start => JOINED_AT_START,
            End::End => JOINED_AT_END,
        }
    }
}

/// What the join rules need to know about a wall.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WallGeom {
    pub id: ElementId,
    pub start: Point,
    pub end: Point,
    /// From the start to the end, of length 1.
    pub direction: Point,
    pub length: f64,
    /// Across the wall, towards its exterior face, of length 1.
    pub side: Point,
    pub layers: Vec<LayerInfo>,
    pub core: Range<usize>,
    /// How far each layer boundary is from the exterior face: 0 first,
    /// then the running sum of the thicknesses.
    depths: Vec<f64>,
    /// How far the exterior face is from the location line, along `side`.
    exterior: f64,
    pub bottom: f64,
    pub top: f64,
}

impl WallGeom {
    pub fn load<'a>(store: &impl Read<'a>, id: ElementId) -> Option<Self> {
        if !store.is_a(id, &class(wall::WALL)) {
            return None;
        }
        let (start, end) = wall::read_line(store, id)?;
        let length = distance(start, end);
        if length <= TOLERANCE {
            return None;
        }
        let direction = scale(sub(end, start), 1.0 / length);
        let layers = wall_type::read_layers(store, store.type_of(id)?);
        let depths: Vec<f64> = std::iter::once(0.0)
            .chain(layers.iter().scan(0.0, |sum, l| {
                *sum += l.thickness;
                Some(*sum)
            }))
            .collect();
        let width = *depths.last()?;
        if width <= 0.0 {
            return None;
        }
        let core = wall_type::core(&layers);
        let (core_out, core_in) = (depths[core.start], depths[core.end]);
        let exterior = match wall::read_location_line(store, id) {
            LocationLine::WallCenterline => width / 2.0,
            LocationLine::CoreCenterline => (core_out + core_in) / 2.0,
            LocationLine::FinishFaceExterior => 0.0,
            LocationLine::FinishFaceInterior => width,
            LocationLine::CoreFaceExterior => core_out,
            LocationLine::CoreFaceInterior => core_in,
        };
        let side = match store.boolean(id, wall::FLIPPED) {
            true => scale(left(direction), -1.0),
            false => left(direction),
        };
        let (bottom, top) = wall::read_elevations(store, id)?;
        Some(Self {
            id,
            start,
            end,
            direction,
            length,
            side,
            layers,
            core,
            depths,
            exterior,
            bottom,
            top,
        })
    }

    pub fn width(&self) -> f64 {
        self.depths[self.layers.len()]
    }

    /// How far the middle of the wall's thickness is from the location
    /// line, along `side`. Doors and windows sit there.
    pub fn middle(&self) -> f64 {
        self.exterior - self.width() / 2.0
    }

    fn core_width(&self) -> f64 {
        self.depths[self.core.end] - self.depths[self.core.start]
    }

    /// Where layer boundary `k` is across the wall (0 is the exterior face,
    /// the layer count the interior face), from the location line along
    /// `side`.
    fn boundary(&self, k: usize) -> f64 {
        self.exterior - self.depths[k]
    }

    /// The line along the wall at `offset` from the location line.
    fn line_at(&self, offset: f64) -> Line {
        Line {
            point: add(self.start, scale(self.side, offset)),
            direction: self.direction,
        }
    }

    pub fn point(&self, end: End) -> Point {
        match end {
            End::Start => self.start,
            End::End => self.end,
        }
    }

    /// The direction from `end` into the wall.
    pub fn inward(&self, end: End) -> Point {
        match end {
            End::Start => self.direction,
            End::End => scale(self.direction, -1.0),
        }
    }

    fn along(&self, p: Point) -> f64 {
        dot(sub(p, self.start), self.direction)
    }

    fn across(&self, p: Point) -> f64 {
        dot(sub(p, self.start), self.side)
    }

    /// Whether `p` is inside the wall's body, more than [`TOLERANCE`] from
    /// its ends.
    pub fn holds(&self, p: Point) -> bool {
        let across = self.across(p);
        let along = self.along(p);
        let (inner, outer) = (self.boundary(self.layers.len()), self.boundary(0));
        (inner - TOLERANCE..=outer + TOLERANCE).contains(&across)
            && (TOLERANCE..self.length - TOLERANCE).contains(&along)
    }

    /// The end of this wall at `p`, if one is.
    pub fn end_at(&self, p: Point) -> Option<End> {
        End::BOTH
            .into_iter()
            .find(|&e| distance(self.point(e), p) <= TOLERANCE)
    }

    /// Where this wall's location line crosses `other`'s, if both cross
    /// away from their ends.
    pub fn crossing(&self, other: &WallGeom) -> Option<Point> {
        let line = |w: &WallGeom| Line {
            point: w.start,
            direction: w.direction,
        };
        let x = line(self).intersection(&line(other))?;
        let inside = |w: &WallGeom| (TOLERANCE..w.length - TOLERANCE).contains(&w.along(x));
        (inside(self) && inside(other)).then_some(x)
    }

    /// Whether both walls stand at some common height.
    pub fn overlaps_vertically(&self, other: &WallGeom) -> bool {
        self.bottom < other.top - TOLERANCE && other.bottom < self.top - TOLERANCE
    }

    fn covers(&self, bottom: f64, top: f64) -> bool {
        self.bottom <= bottom + TOLERANCE && self.top >= top - TOLERANCE
    }

    /// The wall's core, or its whole thickness, as a band across the plan.
    fn band(&self, whole: bool) -> (Line, Line) {
        let (out, inn) = match whole {
            true => (0, self.layers.len()),
            false => (self.core.start, self.core.end),
        };
        (
            self.line_at(self.boundary(out)),
            self.line_at(self.boundary(inn)),
        )
    }

    /// Which of the wall's layers are on the side of its core that `towards`
    /// points to: the exterior finishes or the interior ones.
    fn finishes_towards(&self, towards: Point) -> Range<usize> {
        match dot(towards, self.side) > 0.0 {
            true => 0..self.core.start,
            false => self.core.end..self.layers.len(),
        }
    }

    /// Whether going from `a` and from `b` makes a straight line.
    fn opposite(a: Point, b: Point) -> bool {
        cross(a, b).abs() < PARALLEL && dot(a, b) < 0.0
    }
}

/// The walls joined to a wall, and how.
struct Neighbours {
    /// Per end of the wall: the walls joined there.
    at: [Vec<WallGeom>; 2],
    /// Walls whose end is joined to the wall, with that end.
    to: Vec<(WallGeom, End)>,
    crossing: Vec<WallGeom>,
}

impl Neighbours {
    fn read<'a>(store: &impl Read<'a>, wall: ElementId) -> Self {
        let load = |ids: Vec<ElementId>| -> Vec<WallGeom> {
            let ids: BTreeSet<_> = ids.into_iter().collect();
            ids.into_iter()
                .filter(|&id| id != wall)
                .filter_map(|id| WallGeom::load(store, id))
                .collect()
        };
        let at = End::BOTH.map(|e| load(store.targets(wall, &class(e.relationship()))));
        let mut to = Vec::new();
        for e in End::BOTH {
            let sources = load(store.sources(wall, &class(e.relationship())));
            to.extend(sources.into_iter().map(|w| (w, e)));
        }
        let crosses = class(CROSSES);
        let mut crossing = store.targets(wall, &crosses);
        crossing.extend(store.sources(wall, &crosses));
        Self {
            at,
            to,
            crossing: load(crossing),
        }
    }

    fn all(&self) -> impl Iterator<Item = &WallGeom> {
        let at = self.at.iter().flatten();
        at.chain(self.to.iter().map(|(w, _)| w))
            .chain(&self.crossing)
    }
}

pub(crate) fn compute<'a>(store: &impl Read<'a>, wall: ElementId) -> Option<Footprint> {
    let me = WallGeom::load(store, wall)?;
    let neighbours = Neighbours::read(store, wall);

    // Slice where a joined wall starts or stops.
    let mut heights = vec![me.bottom, me.top];
    for other in neighbours.all() {
        for z in [other.bottom, other.top] {
            if me.bottom + TOLERANCE < z && z < me.top - TOLERANCE {
                heights.push(z);
            }
        }
    }
    heights.sort_by(f64::total_cmp);
    heights.dedup_by(|a, b| (*a - *b).abs() <= TOLERANCE);

    let mut slices: Vec<Slice> = Vec::new();
    for pair in heights.windows(2) {
        let (bottom, top) = (pair[0], pair[1]);
        let layers = slice(&me, &neighbours, bottom, top);
        match slices.last_mut() {
            Some(below) if below.layers == layers => below.top = top,
            _ => slices.push(Slice {
                bottom,
                top,
                layers,
            }),
        }
    }
    Some(Footprint { slices })
}

/// What happens to one end of a wall, for its layers.
enum EndCut {
    /// Every layer stops at the same line.
    All(HalfPlane),
    /// The core stops at one line and the finishes at another: the wall
    /// abuts another one.
    Abut { core: HalfPlane, finish: HalfPlane },
}

/// A band another wall cuts out of some of this wall's layers.
struct Notch {
    band: (Line, Line),
    layers: Range<usize>,
}

/// The layer outlines of `me` between `bottom` and `top`, joined to the
/// neighbours that stand there.
fn slice(me: &WallGeom, neighbours: &Neighbours, bottom: f64, top: f64) -> Vec<LayerOutline> {
    let active = |w: &&WallGeom| w.covers(bottom, top);
    let mut notches = Vec::new();
    let cuts = End::BOTH.map(|end| {
        let at: Vec<&WallGeom> = neighbours.at[end as usize].iter().filter(active).collect();
        end_cut(me, end, &at, &mut notches)
    });

    // Walls whose end abuts this one's side, away from its ends.
    for (other, end) in &neighbours.to {
        if active(&other) && me.holds(other.point(*end)) {
            notches.push(Notch {
                band: other.band(false),
                layers: me.finishes_towards(other.inward(*end)),
            });
        }
    }

    // Crossing walls: the one with the thicker core goes through, as if the
    // other were two walls abutting it.
    for other in neighbours.crossing.iter().filter(active) {
        if me.crossing(other).is_none() {
            continue;
        }
        let through = (me.core_width(), std::cmp::Reverse(me.id))
            >= (other.core_width(), std::cmp::Reverse(other.id));
        if through {
            for towards in [me.side, scale(me.side, -1.0)] {
                notches.push(Notch {
                    band: other.band(false),
                    layers: me.finishes_towards(towards),
                });
            }
        } else {
            notches.push(Notch {
                band: other.band(false),
                layers: me.core.clone(),
            });
            for towards in [me.side, scale(me.side, -1.0)] {
                notches.push(Notch {
                    band: other.band(true),
                    layers: me.finishes_towards(towards),
                });
            }
        }
    }

    // Far enough past the ends that every cut line crosses the band.
    let widest = neighbours
        .all()
        .map(WallGeom::width)
        .fold(me.width(), f64::max);
    let reach = me.length + 20.0 * (me.width() + widest) + 1.0;

    let mut outlines = Vec::with_capacity(me.layers.len());
    for (i, layer) in me.layers.iter().enumerate() {
        let (outer, inner) = (me.boundary(i), me.boundary(i + 1));
        let corner = |offset: f64, along: f64| {
            add(
                add(me.start, scale(me.side, offset)),
                scale(me.direction, along),
            )
        };
        let band = vec![
            corner(outer, -reach),
            corner(outer, me.length + reach),
            corner(inner, me.length + reach),
            corner(inner, -reach),
        ];
        let in_core = me.core.contains(&i);
        let mut pieces = vec![band];
        for cut in &cuts {
            let plane = match cut {
                EndCut::All(plane) => plane,
                EndCut::Abut { core, .. } if in_core => core,
                EndCut::Abut { finish, .. } => finish,
            };
            pieces = pieces.iter().map(|p| clip(p, plane)).collect();
        }
        for notch in notches.iter().filter(|n| n.layers.contains(&i)) {
            pieces = pieces
                .iter()
                .flat_map(|p| cut_band(p, notch.band))
                .collect();
        }
        pieces.retain(|p| !p.is_empty());
        outlines.push(LayerOutline {
            layer: layer.id,
            function: layer.function,
            material: layer.material,
            polygons: pieces,
        });
    }
    outlines
}

/// How the end `end` of `me` is cut, given the walls joined there. Adds
/// the notches this wall gets when it's the one going through.
fn end_cut(me: &WallGeom, end: End, joined: &[&WallGeom], notches: &mut Vec<Notch>) -> EndCut {
    let p = me.point(end);
    let square = EndCut::All(HalfPlane {
        point: p,
        normal: me.inward(end),
    });

    // Walls whose ends meet here, and walls this end abuts.
    let mut meeting: Vec<(&WallGeom, End)> = vec![(me, end)];
    let mut abutted = None;
    for &other in joined {
        match other.end_at(p) {
            Some(e) => meeting.push((other, e)),
            None if other.holds(p) => abutted = abutted.or(Some(other)),
            None => {}
        }
    }

    match meeting.len() {
        1 => match abutted {
            Some(through) => abut(me, end, through).unwrap_or(square),
            None => square,
        },
        2 => {
            let (other, other_end) = meeting[1];
            if WallGeom::opposite(me.inward(end), other.inward(other_end)) {
                square
            } else {
                miter(me, end, other, other_end).unwrap_or(square)
            }
        }
        _ => {
            // Three or more walls: two in line go through, the rest abut
            // them. Of several such pairs, the one with the thicker cores
            // goes through.
            let mut pairs = Vec::new();
            for (i, (a, ea)) in meeting.iter().enumerate() {
                for (b, eb) in &meeting[i + 1..] {
                    if WallGeom::opposite(a.inward(*ea), b.inward(*eb)) {
                        let key = (
                            a.core_width() + b.core_width(),
                            std::cmp::Reverse(a.id.min(b.id)),
                        );
                        pairs.push((key, *a, *b));
                    }
                }
            }
            let Some((_, a, b)) = pairs
                .into_iter()
                .max_by(|x, y| x.0.0.total_cmp(&y.0.0).then(x.0.1.cmp(&y.0.1)))
            else {
                return square;
            };
            if a.id == me.id || b.id == me.id {
                for (other, e) in &meeting {
                    if other.id != a.id && other.id != b.id {
                        notches.push(Notch {
                            band: other.band(false),
                            layers: me.finishes_towards(other.inward(*e)),
                        });
                    }
                }
                square
            } else {
                let through = match (a.core_width(), std::cmp::Reverse(a.id))
                    >= (b.core_width(), std::cmp::Reverse(b.id))
                {
                    true => a,
                    false => b,
                };
                abut(me, end, through).unwrap_or(square)
            }
        }
    }
}

/// `me`'s end abutting the side of `through`: its core goes on to the
/// other wall's core, and its finishes stop at the other wall's face.
fn abut(me: &WallGeom, end: End, through: &WallGeom) -> Option<EndCut> {
    let inward = me.inward(end);
    let towards = dot(inward, through.side);
    if towards.abs() < PARALLEL {
        return None;
    }
    let (face, core, normal) = match towards > 0.0 {
        true => (0, through.core.start, through.side),
        false => (
            through.layers.len(),
            through.core.end,
            scale(through.side, -1.0),
        ),
    };
    let plane = |k: usize| HalfPlane {
        point: add(through.start, scale(through.side, through.boundary(k))),
        normal,
    };
    Some(EndCut::Abut {
        core: plane(core),
        finish: plane(face),
    })
}

/// A corner of two walls: both are cut along the line through the corners
/// of their faces, so each layer meets the matching layer of the other.
fn miter(me: &WallGeom, end: End, other: &WallGeom, other_end: End) -> Option<EndCut> {
    let p = me.point(end);
    // A wall's two faces, the one on the left of `inward` first.
    let faces = |w: &WallGeom, inward: Point| {
        let (exterior, interior) = (
            w.line_at(w.boundary(0)),
            w.line_at(w.boundary(w.layers.len())),
        );
        match dot(left(inward), w.side) > 0.0 {
            true => (exterior, interior),
            false => (interior, exterior),
        }
    };
    let (my_inward, other_inward) = (me.inward(end), other.inward(other_end));
    let (my_left, my_right) = faces(me, my_inward);
    let (other_left, other_right) = faces(other, other_inward);
    let inner = my_left.intersection(&other_right)?;
    let outer = my_right.intersection(&other_left)?;
    // Very sharp corners reach far: cut them square instead.
    let limit = 10.0 * (me.width() + other.width());
    if distance(inner, p) > limit || distance(outer, p) > limit || distance(inner, outer) < 1e-9 {
        return None;
    }
    let mut normal = left(sub(outer, inner));
    if dot(normal, my_inward) < 0.0 {
        normal = scale(normal, -1.0);
    }
    Some(EndCut::All(HalfPlane {
        point: inner,
        normal,
    }))
}

/// What's left of a convex polygon outside the band between two parallel
/// lines: up to two convex pieces.
fn cut_band(polygon: &[Point], (a, b): (Line, Line)) -> Vec<Vec<Point>> {
    let normal = left(a.direction);
    let (sa, sb) = (dot(a.point, normal), dot(b.point, normal));
    if (sa - sb).abs() < 1e-9 {
        return vec![polygon.to_vec()];
    }
    let (low, high) = if sa < sb { (a, b) } else { (b, a) };
    let below = HalfPlane {
        point: low.point,
        normal: scale(normal, -1.0),
    };
    let above = HalfPlane {
        point: high.point,
        normal,
    };
    [clip(polygon, &below), clip(polygon, &above)]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::area;

    #[test]
    fn a_band_cuts_a_square_in_two() {
        let square = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let band = (
            Line {
                point: [1.0, 0.0],
                direction: [0.0, 1.0],
            },
            Line {
                point: [2.0, 9.0],
                direction: [0.0, -1.0],
            },
        );
        let pieces = cut_band(&square, band);
        assert_eq!(pieces.len(), 2);
        let areas: Vec<f64> = pieces.iter().map(|p| area(p)).collect();
        assert_eq!(areas.iter().sum::<f64>(), 12.0);
    }
}
