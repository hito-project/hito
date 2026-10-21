//! The Slab tool (`SB`) and the Vertical opening tool: sketch a closed
//! boundary (#39).
//!
//! As in Revit's sketch mode, the boundary is drawn as lines: each point
//! after the first ends one line and starts the next, and entering the
//! first point again closes the boundary and finishes the slab. Points are
//! clicked in a plan view (#183) or typed on the command line: absolute
//! (`5,0`), or relative to the last point (`@4<90`, `@4,0`).
//!
//! A slab's boundary can also be picked from walls and beams, as Revit's
//! Pick Walls: while the Slab tool runs, a click on a wall in plan adds a
//! line along the wall's interior face, and a click on a beam a line along
//! its centreline. Consecutive picked lines meet where they cross, so the
//! corners come from intersecting each line with the next. The boundary
//! closes when the last line picked meets the first, or when the first is
//! picked again. Picked lines and points mix in one boundary.
//!
//! A boundary that would cross or touch itself is refused as it's drawn.
//!
//! Slabs go on the active level (the level picked in the project browser,
//! or else the lowest level), their top on it, with the project's first
//! slab type until the type selector (#26) exists. Each slab is one
//! transaction and one undo step, and the tool stays active for the next.
//!
//! The Vertical opening tool, as Revit's Opening > Vertical, first asks
//! for the slab to cut, picked in a plan view, and then sketches the
//! opening with points. It stays active for more openings in the same slab
//! until Esc.

use hito_architectural::wall::{self, WALL};
use hito_core::{ElementId, StoreError, StoreView, Transaction};
use hito_structural::beam::{self, BEAM};
use hito_structural::level::LEVEL;
use hito_structural::outline::{self, Point, TOLERANCE};
use hito_structural::slab::{self, Placement, SLAB, SLAB_TYPE};

use super::placement;
use crate::command::{ClassRef, Context, Input, Next, Prompt, Tool, Wants};
use crate::project;

/// What the Vertical opening tool picks first.
const SLABS: &[ClassRef] = &[ClassRef {
    schema: hito_structural::SCHEMA,
    name: SLAB,
}];

/// What a slab's boundary is picked from.
const SUPPORTS: &[ClassRef] = &[
    ClassRef {
        schema: hito_architectural::SCHEMA,
        name: WALL,
    },
    ClassRef {
        schema: hito_structural::SCHEMA,
        name: BEAM,
    },
];

/// Starts the Slab tool.
pub fn start() -> Box<dyn Tool> {
    Box::new(SketchTool::new(Target::Slab))
}

/// Starts the Vertical opening tool.
pub fn start_opening() -> Box<dyn Tool> {
    Box::new(SketchTool::new(Target::Opening))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Slab,
    Opening,
}

/// One piece of a boundary being sketched.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Item {
    /// A corner, clicked or typed.
    Point(Point),
    /// A line picked from an element: a wall's interior face or a beam's
    /// centreline.
    Line {
        element: ElementId,
        line: [Point; 2],
    },
}

#[derive(Debug, Clone)]
struct SketchTool {
    target: Target,
    level: Option<ElementId>,
    /// The slab an opening cuts, once it's picked.
    host: Option<ElementId>,
    items: Vec<Item>,
    closed: bool,
}

impl SketchTool {
    fn new(target: Target) -> Self {
        Self {
            target,
            level: None,
            host: None,
            items: Vec::new(),
            closed: false,
        }
    }

    fn corners(&self) -> Vec<Point> {
        corners(&self.items, self.closed)
    }

    /// Closes the boundary if it makes a valid outline closed.
    fn close(&mut self) -> Result<(), &'static str> {
        if self.items.len() < 2 {
            return Err("sketch-too-few");
        }
        let closed = corners(&self.items, true);
        match outline::check(&closed) {
            Ok(()) => {
                self.closed = true;
                Ok(())
            }
            Err(_) if closed.len() < 3 => Err("sketch-too-few"),
            Err(_) => Err("sketch-crossing"),
        }
    }

    /// Adds `item`, unless the open boundary would then cross or touch
    /// itself.
    fn add(&mut self, item: Item) -> Result<(), &'static str> {
        self.items.push(item);
        if outline::check_open(&self.corners()).is_err() {
            self.items.pop();
            return Err("sketch-crossing");
        }
        Ok(())
    }
}

impl Tool for SketchTool {
    fn start(&mut self, context: &Context<'_>) {
        self.level = context.level;
    }

    fn prompt(&self) -> Prompt {
        let (first, next) = match self.target {
            Target::Opening if self.host.is_none() => {
                return Prompt {
                    message: "opening-prompt-slab",
                    wants: Wants::Element(SLABS),
                };
            }
            Target::Slab => ("slab-prompt-start", "slab-prompt-next"),
            Target::Opening => ("opening-prompt-start", "opening-prompt-next"),
        };
        let from = self.corners().last().map(|&[x, y]| [x, y, 0.0]);
        let message = if from.is_some() { next } else { first };
        let wants = match (self.target, from) {
            (Target::Slab, from) => Wants::PointOrElement {
                from,
                classes: SUPPORTS,
            },
            (Target::Opening, None) => Wants::Point,
            (Target::Opening, Some(from)) => Wants::PointFrom(from),
        };
        Prompt { message, wants }
    }

    fn pick(&mut self, element: ElementId, model: &StoreView<'_>) -> Next {
        match self.target {
            Target::Opening => self.pick_host(element, model),
            Target::Slab => self.pick_line(element, model),
        }
    }

    fn input(&mut self, input: Input) -> Next {
        let Input::Point([x, y, _]) = input else {
            return Next::Reject("input-not-a-point");
        };
        let point = [x, y];
        let near = |a: Point| (a[0] - x).hypot(a[1] - y) <= TOLERANCE;
        let corners = self.corners();
        match (corners.first(), corners.last()) {
            (_, Some(&last)) if near(last) => Next::Reject("sketch-same-point"),
            (Some(&first), _) if near(first) => match self.close() {
                Ok(()) => Next::Repeat,
                Err(message) => Next::Reject(message),
            },
            _ => match self.add(Item::Point(point)) {
                Ok(()) => Next::Ask,
                Err(message) => Next::Reject(message),
            },
        }
    }

    /// An opening's next one goes in the same slab; a slab's starts again.
    fn after_commit(&self) -> Option<Box<dyn Tool>> {
        Some(Box::new(SketchTool {
            items: Vec::new(),
            closed: false,
            ..self.clone()
        }))
    }

    fn commit(self: Box<Self>, tx: &mut Transaction<'_>) -> Result<(), StoreError> {
        let model = project::physical_model(tx)?;
        let view = tx.view();
        let corners = self.corners();
        match self.target {
            Target::Slab => {
                // Without a level or a slab type, the command fails and
                // says so.
                let level = placement::base_level(&view, self.level)
                    .ok_or_else(|| StoreError::UnknownClass(hito_structural::class(LEVEL)))?;
                let slab_type = default_type(&view)
                    .ok_or_else(|| StoreError::UnknownClass(hito_structural::class(SLAB_TYPE)))?;
                let placement = Placement::new(slab_type, level, &corners);
                slab::insert(tx, model, &placement).map(drop)
            }
            Target::Opening => {
                let host = self.host.expect("the slab is picked first");
                // The slab's rules refuse an opening that isn't inside it.
                slab::insert_opening(tx, model, host, &corners).map(drop)
            }
        }
    }
}

impl SketchTool {
    /// The Vertical opening's slab.
    fn pick_host(&mut self, element: ElementId, model: &StoreView<'_>) -> Next {
        let class = hito_structural::class(SLAB);
        let is_slab = model
            .get(element)
            .is_some_and(|e| model.registry().is_a(e.class(), &class));
        if self.host.is_some() || !is_slab {
            return Next::Reject("opening-pick-a-slab");
        }
        self.host = Some(element);
        Next::Ask
    }

    /// A wall's interior face or a beam's centreline, for a slab's
    /// boundary.
    fn pick_line(&mut self, element: ElementId, model: &StoreView<'_>) -> Next {
        let is = |name| {
            let class = match name {
                WALL => hito_architectural::class(WALL),
                _ => hito_structural::class(BEAM),
            };
            model
                .get(element)
                .is_some_and(|e| model.registry().is_a(e.class(), &class))
        };
        let line = if is(WALL) {
            wall::interior_face(model, element)
        } else if is(BEAM) {
            beam::line(model, element)
        } else {
            None
        };
        let Some((a, b)) = line else {
            return Next::Reject("slab-pick-wall-or-beam");
        };
        let picked = |item: &Item| matches!(item, Item::Line { element: e, .. } if *e == element);
        // Picking the first line again closes the boundary.
        if self.items.first().is_some_and(picked) {
            return match self.close() {
                Ok(()) => Next::Repeat,
                Err(message) => Next::Reject(message),
            };
        }
        if self.items.iter().any(picked) {
            return Next::Reject("sketch-picked-twice");
        }
        let line = Item::Line {
            element,
            line: [a, b],
        };
        // The line picked meeting the first closes the boundary. Left
        // open, it would cross the first line, so this comes first.
        let meets_first = match self.items.first() {
            Some(Item::Line { line: first, .. }) => {
                self.items.len() >= 2 && segments_meet([a, b], *first)
            }
            _ => false,
        };
        if meets_first {
            self.items.push(line);
            if self.close().is_ok() {
                return Next::Repeat;
            }
            self.items.pop();
        }
        match self.add(line) {
            Ok(()) => Next::Ask,
            Err(message) => Next::Reject(message),
        }
    }
}

/// The boundary's corners, in order. A point is a corner. A line's end
/// that meets the next line, or its start that meets the one before, is
/// where the two lines cross; its other ends are its own. Closed, the last
/// item is followed by the first.
fn corners(items: &[Item], closed: bool) -> Vec<Point> {
    let n = items.len();
    let around = closed && n > 1;
    let prev = |i: usize| match i {
        0 if around => Some(n - 1),
        0 => None,
        i => Some(i - 1),
    };
    let next = |i: usize| match i + 1 {
        j if j < n => Some(j),
        _ if around => Some(0),
        _ => None,
    };
    let line = |i: Option<usize>| match i.map(|i| items[i]) {
        Some(Item::Line { line, .. }) => Some(line),
        _ => None,
    };
    let point = |i: Option<usize>| match i.map(|i| items[i]) {
        Some(Item::Point(p)) => Some(p),
        _ => None,
    };
    let mut out: Vec<Point> = Vec::new();
    for (i, &item) in items.iter().enumerate() {
        let [a, b] = match item {
            Item::Point(p) => {
                out.push(p);
                continue;
            }
            Item::Line { line, .. } => line,
        };
        let crossing_next = line(next(i)).and_then(|l| intersection([a, b], l));
        let crossing_prev = line(prev(i)).and_then(|l| intersection(l, [a, b]));
        let nearer = |to: Point| match distance(a, to) <= distance(b, to) {
            true => (a, b),
            false => (b, a),
        };
        // Which end the boundary comes in by, and which it leaves by.
        let (start, end) = if let Some(x) = crossing_next {
            let (near, far) = nearer(x);
            (far, near)
        } else if let Some(x) = crossing_prev {
            nearer(x)
        } else if let Some(p) = point(prev(i)) {
            nearer(p)
        } else if let Some(q) = point(next(i)) {
            let (near, far) = nearer(q);
            (far, near)
        } else {
            (a, b)
        };
        // A crossing with the line before is that line's end.
        if crossing_prev.is_none() {
            out.push(start);
        }
        out.push(crossing_next.unwrap_or(end));
    }
    out.dedup_by(|a, b| distance(*a, *b) <= TOLERANCE);
    if out.len() > 1 && distance(out[0], out[out.len() - 1]) <= TOLERANCE {
        out.pop();
    }
    out
}

fn distance(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Where the lines through `p` and `q` cross, `None` if they're parallel.
fn intersection(p: [Point; 2], q: [Point; 2]) -> Option<Point> {
    crossing(p, q).map(|(t, _)| along(p, t))
}

/// How far along `p` and along `q` (0 at their starts, 1 at their ends)
/// the lines through them cross, `None` if they're parallel.
fn crossing([a, b]: [Point; 2], [c, d]: [Point; 2]) -> Option<(f64, f64)> {
    let r = [b[0] - a[0], b[1] - a[1]];
    let s = [d[0] - c[0], d[1] - c[1]];
    let denominator = r[0] * s[1] - r[1] * s[0];
    let scale = (r[0].hypot(r[1]) * s[0].hypot(s[1])).max(f64::MIN_POSITIVE);
    if (denominator / scale).abs() < 1e-9 {
        return None;
    }
    let w = [c[0] - a[0], c[1] - a[1]];
    let t = (w[0] * s[1] - w[1] * s[0]) / denominator;
    let u = (w[0] * r[1] - w[1] * r[0]) / denominator;
    Some((t, u))
}

fn along([a, b]: [Point; 2], t: f64) -> Point {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// Whether the segments `p` and `q` cross or touch, within the tolerance.
fn segments_meet(p: [Point; 2], q: [Point; 2]) -> bool {
    let within = |t: f64, [a, b]: [Point; 2]| {
        let slack = TOLERANCE / distance(a, b).max(f64::MIN_POSITIVE);
        (-slack..=1.0 + slack).contains(&t)
    };
    crossing(p, q).is_some_and(|(t, u)| within(t, p) && within(u, q))
}

/// The slab type new slabs get: the project's first.
fn default_type(view: &StoreView<'_>) -> Option<ElementId> {
    placement::first_of(view, &hito_structural::class(SLAB_TYPE))
}

#[cfg(test)]
mod tests {
    use hito_core::{Element, ElementStore, SchemaRegistry, Value};
    use hito_structural::level;

    use super::*;
    use crate::command::{Command, InputFormat, Outcome, Registry, Run, Session};
    use crate::i18n::{I18n, Locale};

    struct Fixture {
        session: Session,
        i18n: I18n,
        format: InputFormat,
        store: ElementStore,
    }

    impl Fixture {
        /// A new project, as the shell makes one, with the Slab and
        /// Vertical opening commands.
        fn new() -> Self {
            let i18n = I18n::new(Locale::EsAr);
            let mut schemas = SchemaRegistry::new();
            hito_structural::register(&mut schemas).unwrap();
            let store = project::new_project(schemas, &i18n);
            let mut registry = Registry::new();
            let commands = [
                ("SLAB", "command-slab", Some("SB"), start as fn() -> _),
                (
                    "VERTICALOPENING",
                    "command-vertical-opening",
                    None,
                    start_opening,
                ),
            ];
            for (name, label, alias, start) in commands {
                let run = Run::Tool(start);
                let command = Command {
                    name,
                    label,
                    alias,
                    run,
                };
                registry.register(command).unwrap();
            }
            Self {
                session: Session::new(registry),
                i18n,
                format: InputFormat::new(),
                store,
            }
        }

        fn enter(&mut self, text: &str) -> Outcome {
            let Self {
                session,
                i18n,
                format,
                store,
            } = self;
            session.enter(text, i18n, format, store)
        }

        /// Picks an element, as a click in a plan view does.
        fn pick(&mut self, element: ElementId) -> Outcome {
            let Self {
                session,
                i18n,
                store,
                ..
            } = self;
            session.input(Input::Element(element), i18n, store)
        }

        fn enter_all(&mut self, points: &[&str]) -> Vec<Outcome> {
            points.iter().map(|p| self.enter(p)).collect()
        }

        fn slabs(&self) -> Vec<ElementId> {
            slab::slabs(&self.store.view())
        }
    }

    #[test]
    fn a_slab_is_sketched_corner_by_corner_and_closed_on_its_first_point() {
        let mut f = Fixture::new();
        f.enter("SB");
        assert_eq!(f.session.prompt().unwrap().message, "slab-prompt-start");
        let outcomes = f.enter_all(&["0,0", "@6<0", "@4<90", "@-6,0"]);
        assert!(outcomes.iter().all(|o| matches!(o, Outcome::Nothing)));
        assert_eq!(
            f.session.prompt().unwrap().wants.point(),
            Some(Wants::PointFrom([0.0, 4.0, 0.0]))
        );
        let Outcome::Committed(changes) = f.enter("0,0") else {
            panic!("closing the boundary commits the slab");
        };
        assert_eq!(changes.command().map(|c| c.name()), Some("SLAB"));

        let slabs = f.slabs();
        assert_eq!(slabs.len(), 1);
        let view = f.store.view();
        let boundary = slab::boundary(&view, slabs[0]);
        let expected = [[0.0, 0.0], [6.0, 0.0], [6.0, 4.0], [0.0, 4.0]];
        for (corner, expected) in boundary.iter().zip(expected) {
            assert!((corner[0] - expected[0]).abs() < 1e-9);
            assert!((corner[1] - expected[1]).abs() < 1e-9);
        }
        assert_eq!(boundary.len(), 4);
        let ground = level::levels(&view)[0].0;
        assert_eq!(level::level_of(&view, slabs[0]), Some(ground));
        let (bottom, top) = slab::elevations(&view, slabs[0]).unwrap();
        assert!((top - 0.0).abs() < 1e-12 && (bottom + 0.15).abs() < 1e-12);

        // The tool stays active for the next slab.
        assert_eq!(f.session.prompt().unwrap().message, "slab-prompt-start");
    }

    #[test]
    fn a_boundary_that_would_cross_or_close_too_early_is_refused() {
        let mut f = Fixture::new();
        f.enter("SB");
        f.enter_all(&["0,0", "4,0"]);
        // Closing with two corners is refused, and so is the same point.
        assert!(matches!(f.enter("0,0"), Outcome::Nothing));
        assert!(f.session.message().is_some());
        assert!(matches!(f.enter("4,0"), Outcome::Nothing));
        f.enter_all(&["4,4", "2,4"]);
        // Back across the first line.
        assert!(matches!(f.enter("2,-1"), Outcome::Nothing));
        assert!(f.session.message().is_some());
        // The refused point wasn't kept: the next one goes on from 2,4.
        assert_eq!(
            f.session.prompt().unwrap().wants.point(),
            Some(Wants::PointFrom([2.0, 4.0, 0.0]))
        );
        assert!(matches!(f.enter("0,4"), Outcome::Nothing));
        assert!(matches!(f.enter("0,0"), Outcome::Committed(_)));
        assert_eq!(f.slabs().len(), 1);
    }

    #[test]
    fn slabs_go_on_the_active_level() {
        let mut f = Fixture::new();
        let mut tx = f.store.transaction();
        let model = project::physical_model(&mut tx).unwrap();
        let first = Element::new(hito_structural::class(LEVEL), model)
            .with_property(level::NAME, "Nivel 2")
            .with_property(level::ELEVATION, 2.8);
        let first = tx.insert(first).unwrap();
        tx.commit().unwrap();
        f.session.set_level(Some(first));
        f.enter("SB");
        f.enter_all(&["0,0", "5,0", "5,5", "0,0"]);
        let slab = f.slabs()[0];
        let view = f.store.view();
        assert_eq!(level::level_of(&view, slab), Some(first));
        assert_eq!(
            f.store.property(slab, slab::TOP_ELEVATION),
            Some(&Value::Real(2.8))
        );
    }

    #[test]
    fn a_vertical_opening_cuts_the_slab_picked_first() {
        let mut f = Fixture::new();
        f.enter("SB");
        f.enter_all(&["0,0", "6,0", "6,4", "0,4", "0,0"]);
        f.enter_all(&["10,0", "16,0", "16,4", "10,0"]);
        f.session.cancel(&f.i18n);
        let [floor, other] = f.slabs()[..] else {
            panic!("two slabs");
        };

        f.enter("VERTICALOPENING");
        assert_eq!(
            f.session.prompt().unwrap(),
            Prompt {
                message: "opening-prompt-slab",
                wants: Wants::Element(SLABS),
            }
        );
        // The slab can't be typed, and only a slab can be picked.
        f.enter("1,1");
        assert_eq!(f.session.prompt().unwrap().message, "opening-prompt-slab");
        let level = level::levels(&f.store.view())[0].0;
        f.pick(level);
        assert!(f.session.message().is_some());
        f.pick(floor);
        assert_eq!(f.session.prompt().unwrap().message, "opening-prompt-start");
        let outcomes = f.enter_all(&["1,1", "@1,0", "@0,2", "@-1,0", "1,1"]);
        assert!(matches!(outcomes[4], Outcome::Committed(_)));
        assert_eq!(slab::openings(&f.store.view(), floor).len(), 1);

        // The next opening goes in the same slab, so one outside it, even
        // inside another slab, can't be made.
        assert_eq!(f.session.prompt().unwrap().message, "opening-prompt-start");
        let outcomes = f.enter_all(&["11,1", "12,1", "12,2", "11,1"]);
        assert!(matches!(outcomes[3], Outcome::Nothing));
        assert!(f.session.message().is_some());
        assert_eq!(slab::openings(&f.store.view(), floor).len(), 1);
        assert!(slab::openings(&f.store.view(), other).is_empty());
    }

    fn line(element: ElementId, a: Point, b: Point) -> Item {
        Item::Line {
            element,
            line: [a, b],
        }
    }

    fn same(corners: &[Point], expected: &[Point]) -> bool {
        corners.len() == expected.len()
            && corners
                .iter()
                .zip(expected)
                .all(|(a, b)| distance(*a, *b) < 1e-9)
    }

    #[test]
    fn picked_lines_meet_where_they_cross() {
        let id = || ElementId::generate();
        // Four faces running past each other at the corners, in any
        // direction.
        let lines = [
            line(id(), [-0.1, 0.0], [6.1, 0.0]),
            line(id(), [6.0, -0.1], [6.0, 4.1]),
            line(id(), [-0.1, 4.0], [6.1, 4.0]),
            line(id(), [0.0, 4.1], [0.0, -0.1]),
        ];
        let rectangle = [[6.0, 0.0], [6.0, 4.0], [0.0, 4.0], [0.0, 0.0]];
        assert!(same(&corners(&lines, true), &rectangle));
        // Open, the first line's start and the last one's end are their own.
        let open = corners(&lines, false);
        assert!(same(
            &open,
            &[[-0.1, 0.0], [6.0, 0.0], [6.0, 4.0], [0.0, 4.0], [0.0, -0.1]]
        ));

        // Mixed with points, a line runs from the end nearer the point
        // before it.
        let mixed = [
            Item::Point([0.0, 0.0]),
            line(id(), [6.0, 4.0], [6.0, 0.0]),
            Item::Point([0.0, 4.0]),
        ];
        let expected = [[0.0, 0.0], [6.0, 0.0], [6.0, 4.0], [0.0, 4.0]];
        assert!(same(&corners(&mixed, true), &expected));
    }

    #[test]
    fn parallel_lines_keep_their_ends() {
        let id = || ElementId::generate();
        let lines = [
            line(id(), [0.0, 0.0], [3.0, 0.0]),
            line(id(), [3.0, 0.0], [6.0, 0.0]),
        ];
        assert!(same(
            &corners(&lines, false),
            &[[0.0, 0.0], [3.0, 0.0], [6.0, 0.0]]
        ));
    }

    #[test]
    fn a_slab_boundary_picked_from_beams_and_points() {
        let mut f = Fixture::new();
        let view = f.store.view();
        let ground = level::levels(&view)[0].0;
        let beam_type = placement::first_of(&view, &hito_structural::class(beam::BEAM_TYPE));
        let mut tx = f.store.transaction();
        let model = project::physical_model(&mut tx).unwrap();
        let mut beam = |a, b| {
            let placement = beam::Placement::new(
                beam_type.unwrap(),
                ground,
                beam::End::Point(a),
                beam::End::Point(b),
            );
            beam::insert(&mut tx, model, &placement).unwrap()
        };
        let south = beam([0.0, 0.0], [6.0, 0.0]);
        let east = beam([6.0, 0.0], [6.0, 4.0]);
        tx.commit().unwrap();

        f.enter("SB");
        assert!(matches!(f.pick(south), Outcome::Nothing));
        assert!(matches!(f.pick(east), Outcome::Nothing));
        // The same beam twice is refused; a level isn't a support.
        f.pick(east);
        assert!(f.session.message().is_some());
        f.pick(ground);
        assert!(f.session.message().is_some());
        assert_eq!(
            f.session.prompt().unwrap().wants.point(),
            Some(Wants::PointFrom([6.0, 4.0, 0.0]))
        );
        f.enter("0,4");
        let Outcome::Committed(_) = f.enter("0,0") else {
            panic!("closed on the first corner");
        };
        let boundary = slab::boundary(&f.store.view(), f.slabs()[0]);
        let expected = [[0.0, 0.0], [6.0, 0.0], [6.0, 4.0], [0.0, 4.0]];
        assert!(same(&boundary, &expected), "{boundary:?}");
    }

    #[test]
    fn new_projects_have_a_generic_slab_type() {
        let f = Fixture::new();
        let view = f.store.view();
        let slab_type = default_type(&view).unwrap();
        assert_eq!(
            f.store.property(slab_type, slab::NAME),
            Some(&Value::Text("Genérico - 150 mm".into()))
        );
        assert_eq!(slab::thickness(&view, slab_type), Some(0.15));
        // Of the generic concrete, as the other generic types.
        let concrete = slab::material(&view, slab_type).unwrap();
        assert_eq!(
            f.store.property(concrete, hito_structural::material::NAME),
            Some(&Value::Text("Hormigón".into()))
        );
    }
}
