//! Plan views (#183) in the shell: what a level's plan shows, read from
//! the model, and what it's called.

use std::collections::BTreeSet;

use glam::DVec2;
use hito_architectural::opening::{self, OPENING};
use hito_core::{ElementId, ElementStore, StoreView, Value};
use hito_structural::section::{self, Section};
use hito_structural::{class, column, grid, level};

use crate::command::ClassRef;
use crate::viewport::plan::CUT_HEIGHT;
use crate::viewport::{Centre, Plan, PlanGrid, Segment, Symbol};

/// The sides of the polygon a round column snaps by.
const CIRCLE_SIDES: usize = 32;

/// The largest angle one straight piece of a door's swing arc turns
/// through: 5 degrees, so the arc looks round at any zoom a plan is read
/// at.
const ARC_STEP: f64 = std::f64::consts::PI / 36.0;

/// Every level, lowest first, for each to have a plan view.
pub(super) fn levels(store: &ElementStore) -> Vec<ElementId> {
    let view = store.view();
    level::levels(&view).into_iter().map(|(id, _)| id).collect()
}

/// A plan view's name: its level's, as Revit names a level's floor plan.
pub(super) fn name(store: &ElementStore, level: ElementId) -> String {
    match store.property(level, level::NAME) {
        Some(Value::Text(name)) => name.clone(),
        _ => String::new(),
    }
}

/// What the plan of `level` shows besides the elements' meshes: its
/// elevation, every grid, since grids show in every plan as in Revit, and
/// which elements show from above where the cut misses them.
pub(super) fn content(store: &ElementStore, level: ElementId) -> Plan {
    let view = store.view();
    let elevation = match view.property(level, level::ELEVATION) {
        Some(Value::Real(e)) => *e,
        _ => 0.0,
    };
    Plan {
        elevation,
        grids: grids(&view),
        snaps: grid::intersections(&view)
            .into_iter()
            .map(|i| i.point)
            .collect(),
        from_above: view
            .elements()
            .map(|e| e.id())
            .filter(|&id| level::level_of(&view, id) == Some(level))
            .chain(reaching_up(&view, elevation))
            .collect(),
        symbols: symbols(&view, elevation + CUT_HEIGHT),
        centres: centres(&view, elevation),
        pickable: BTreeSet::new(),
    }
}

/// The elements of `classes`, or of classes derived from them: what a
/// prompt that wants one of them can pick.
pub(super) fn of_classes(store: &ElementStore, classes: &[ClassRef]) -> BTreeSet<ElementId> {
    let registry = store.registry();
    let classes: Vec<_> = classes.iter().map(|c| c.id()).collect();
    store
        .elements()
        .filter(|e| classes.iter().any(|c| registry.is_a(e.class(), c)))
        .map(|e| e.id())
        .collect()
}

/// The columns that reach up to `elevation` from below it, such as a
/// Level 1 column whose top is at Level 2. A plan shows them from above,
/// so its beams can be placed by clicking them.
fn reaching_up(view: &StoreView<'_>, elevation: f64) -> Vec<ElementId> {
    column::columns(view)
        .into_iter()
        .filter(|&c| {
            column::elevations(view, c).is_some_and(|(bottom, top)| {
                bottom < elevation - column::TOLERANCE && elevation <= top + column::TOLERANCE
            })
        })
        .collect()
}

/// The columns at `elevation`, standing on it or reaching up to it from
/// below, for clicks on or near them to snap to their centres. What a
/// beam's end frames into (`beam::column_at`), seen from the plan.
fn centres(view: &StoreView<'_>, elevation: f64) -> Vec<Centre> {
    column::columns(view)
        .into_iter()
        .filter(|&c| {
            column::elevations(view, c).is_some_and(|(bottom, top)| {
                bottom <= elevation + column::TOLERANCE && elevation <= top + column::TOLERANCE
            })
        })
        .filter_map(|c| {
            let point = column::location(view, c)?;
            let section = section::of(view, c)?;
            let rotation = match view.property(c, column::ROTATION) {
                Some(Value::Real(r)) => *r,
                _ => 0.0,
            };
            let local: Vec<DVec2> = match section {
                Section::Rectangle { width, depth } => {
                    let [w, d] = [width / 2.0, depth / 2.0];
                    [(-w, -d), (w, -d), (w, d), (-w, d)].map(DVec2::from).into()
                }
                Section::Circle { diameter } => (0..CIRCLE_SIDES)
                    .map(|i| {
                        let angle = std::f64::consts::TAU * i as f64 / CIRCLE_SIDES as f64;
                        diameter / 2.0 * DVec2::from_angle(angle)
                    })
                    .collect(),
            };
            let turn = DVec2::from_angle(rotation);
            let centre = DVec2::from(point);
            let outline = local
                .into_iter()
                .map(|p| (centre + turn.rotate(p)).into())
                .collect();
            Some(Centre { point, outline })
        })
        .collect()
}

/// Every grid with both ends, in the order they were drawn.
fn grids(view: &StoreView<'_>) -> Vec<PlanGrid> {
    let registry = view.registry();
    let mut grids: Vec<PlanGrid> = view
        .elements()
        .filter(|e| registry.is_a(e.class(), &class(grid::GRID)))
        .filter_map(|e| {
            let line = grid::GridLine::of(view, e.id())?;
            let label = match e.properties().get(grid::NAME) {
                Some(Value::Text(label)) => label.clone(),
                _ => String::new(),
            };
            Some(PlanGrid {
                id: e.id(),
                label,
                start: line.start,
                end: line.end,
            })
        })
        .collect();
    grids.sort_by_key(|g| g.id);
    grids
}

/// Every door's and window's plan symbol, cut at height `z`, as Revit
/// draws them: the jambs heavy, as cut, and the leaf, its swing and the
/// window's lines thin. A door or window the cut misses, such as a high
/// window, gets an empty symbol, so it doesn't show.
fn symbols(view: &StoreView<'_>, z: f64) -> Vec<Symbol> {
    let class = hito_architectural::class(OPENING);
    let registry = view.registry();
    view.elements()
        .filter(|e| registry.is_a(e.class(), &class))
        .map(|e| {
            let id = e.id();
            let crossed = opening::cut(view, id).is_some_and(|c| c.bottom <= z && z <= c.top);
            let symbol = crossed.then(|| opening::plan_symbol(view, id)).flatten();
            let Some(symbol) = symbol else {
                return Symbol {
                    id,
                    cut: Vec::new(),
                    thin: Vec::new(),
                };
            };
            let cut = symbol.outlines.iter().flat_map(|o| closed(o)).collect();
            let lines = symbol.lines.iter().map(|&[a, b]| [a.into(), b.into()]);
            let arcs = symbol.arcs.iter().flat_map(arc);
            Symbol {
                id,
                cut,
                thin: lines.chain(arcs).collect(),
            }
        })
        .collect()
}

/// The sides of a closed outline.
fn closed(outline: &[[f64; 2]]) -> Vec<Segment> {
    let points: Vec<DVec2> = outline.iter().map(|&p| p.into()).collect();
    let next = points.iter().cycle().skip(1);
    points.iter().zip(next).map(|(&a, &b)| [a, b]).collect()
}

/// An arc as straight pieces, none turning more than [`ARC_STEP`].
fn arc(arc: &opening::Arc) -> Vec<Segment> {
    let steps = (arc.sweep.abs() / ARC_STEP).ceil().max(1.0) as usize;
    let centre = DVec2::from(arc.centre);
    let at = |i: usize| {
        let angle = arc.start + arc.sweep * i as f64 / steps as f64;
        centre + arc.radius * DVec2::from_angle(angle)
    };
    (0..steps).map(|i| [at(i), at(i + 1)]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{I18n, Locale};
    use hito_core::SchemaRegistry;

    use crate::command::{InputFormat, Session};
    use crate::project;
    use crate::shell::registry;

    /// A new project with a 6 m wall along x, a door on its interior side
    /// (-y) at 2 m and a window at 4.5 m, placed as the tools place them.
    fn door_and_window() -> ElementStore {
        let i18n = I18n::new(Locale::En);
        let mut schemas = SchemaRegistry::new();
        hito_structural::register(&mut schemas).unwrap();
        hito_architectural::register(&mut schemas).unwrap();
        let mut store = project::new_project(schemas, &i18n);
        let mut session = Session::new(registry());
        let format = InputFormat::new();
        for text in ["WA", "0,0", "6,0"] {
            session.enter(text, &i18n, &format, &mut store);
        }
        session.cancel(&i18n);
        for (command, point) in [("DR", "2,-0.05"), ("WN", "4.5,0")] {
            session.enter(command, &i18n, &format, &mut store);
            session.enter(point, &i18n, &format, &mut store);
            session.cancel(&i18n);
        }
        store
    }

    fn of_kind(store: &ElementStore, symbols: &[Symbol], kind: &str) -> Symbol {
        let class = hito_architectural::class(kind);
        let found = symbols
            .iter()
            .find(|s| store.get(s.id).unwrap().class() == &class);
        found.expect("a symbol").clone()
    }

    #[test]
    fn doors_and_windows_draw_their_plan_symbol_where_the_cut_crosses_them() {
        let store = door_and_window();
        let level = levels(&store)[0];
        let plan = content(&store, level);
        assert_eq!(plan.symbols.len(), 2);

        let door = of_kind(&store, &plan.symbols, opening::DOOR);
        assert_eq!(door.cut.len(), 8, "two jambs, cut");
        // The leaf, then the swing arc in 5 degree pieces, on the interior
        // side the door was placed from.
        assert_eq!(door.thin.len(), 1 + 18);
        let ends = door.thin.iter().flatten();
        assert!(ends.clone().all(|p| p.y < 1e-9), "swings to -y");
        let arc = &door.thin[1..];
        for pair in arc.windows(2) {
            assert_eq!(pair[0][1], pair[1][0], "the arc is joined up");
        }

        let window = of_kind(&store, &plan.symbols, opening::WINDOW);
        assert_eq!(window.cut.len(), 8, "two jambs, cut");
        assert_eq!(window.thin.len(), 3, "both wall faces and the glass");

        // Cut above them both, they don't show.
        let high = symbols(&store.view(), 2.5);
        assert!(high.iter().all(|s| s.cut.is_empty() && s.thin.is_empty()));
    }

    #[test]
    fn a_plan_shows_every_grid_and_snaps_where_they_cross() {
        let mut store = project::tests::structural(&I18n::new(Locale::En));
        let mut tx = store.transaction();
        let model = project::physical_model(&mut tx).unwrap();
        for (label, a, b) in [
            ("1", [0.0, 0.0], [0.0, 10.0]),
            ("A", [-2.0, 4.0], [8.0, 4.0]),
        ] {
            tx.insert(grid::GridLine::new(a, b).element(model, label))
                .unwrap();
        }
        tx.commit().unwrap();
        let level = levels(&store)[0];
        assert_eq!(name(&store, level), "Level 1");
        let plan = content(&store, level);
        assert_eq!(plan.elevation, 0.0);
        let labels: Vec<_> = plan.grids.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, ["1", "A"]);
        assert_eq!(plan.snaps, [[0.0, 4.0]]);
    }
}
