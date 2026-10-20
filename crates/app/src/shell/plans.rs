//! Plan views (#183) in the shell: what a level's plan shows, read from
//! the model, and what it's called.

use hito_core::{ElementId, ElementStore, StoreView, Value};
use hito_structural::{class, grid, level};

use crate::viewport::{Plan, PlanGrid};

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

/// What the plan of `level` shows besides the elements: its elevation, and
/// every grid, since grids show in every plan, as in Revit.
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
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{I18n, Locale};
    use crate::project;

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
