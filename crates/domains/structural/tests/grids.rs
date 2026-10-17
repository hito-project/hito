//! Structural grids (#34), through the core's public API only. The
//! placement tool, the views and their label bubbles come later; this is
//! what they build on.

use hito_core::{ElementId, ElementStore, Model, ModelId, ModelKind, SchemaRegistry, StoreError};
use hito_structural::class;
use hito_structural::grid::{self, END_X, GRID, GridLine, NAME, START_X};

struct Project {
    store: ElementStore,
    physical: ModelId,
}

fn project() -> Project {
    let mut registry = SchemaRegistry::new();
    hito_structural::register(&mut registry).unwrap();
    let mut store = ElementStore::new(registry);
    let mut tx = store.transaction();
    let physical = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
    tx.commit().unwrap();
    Project { store, physical }
}

impl Project {
    fn grid(
        &mut self,
        name: &str,
        start: [f64; 2],
        end: [f64; 2],
    ) -> Result<ElementId, StoreError> {
        let element = GridLine::new(start, end).element(self.physical, name);
        let mut tx = self.store.transaction();
        let id = tx.insert(element)?;
        tx.commit()?;
        Ok(id)
    }

    fn set(
        &mut self,
        id: ElementId,
        name: &str,
        value: impl Into<hito_core::Value>,
    ) -> Result<(), StoreError> {
        let mut tx = self.store.transaction();
        tx.set_property(id, name, value)?;
        tx.commit().map(|_| ())
    }
}

/// The names of the rules a failed commit broke.
fn broken_rules(result: Result<impl Sized, StoreError>) -> Vec<String> {
    match result {
        Err(StoreError::Violations(violations)) => violations.into_iter().map(|v| v.rule).collect(),
        Err(other) => panic!("expected rule violations, got {other}"),
        Ok(_) => panic!("expected rule violations, the commit succeeded"),
    }
}

/// A 3 × 2 grid: A, B, C along x every 5 m, and 1, 2 along y every 4 m.
fn layout(p: &mut Project) -> Vec<ElementId> {
    let mut ids = Vec::new();
    for (i, name) in ["A", "B", "C"].into_iter().enumerate() {
        let x = 5.0 * i as f64;
        ids.push(p.grid(name, [x, -1.0], [x, 5.0]).unwrap());
    }
    for (i, name) in ["1", "2"].into_iter().enumerate() {
        let y = 4.0 * i as f64;
        ids.push(p.grid(name, [-1.0, y], [11.0, y]).unwrap());
    }
    ids
}

#[test]
fn grids_are_spatial_location_elements() {
    let p = project();
    let registry = p.store.registry();
    assert!(registry.is_a(&class(GRID), &hito_core::ClassId::SPATIAL_LOCATION_ELEMENT));
}

#[test]
fn grids_are_placed_moved_and_relabelled() {
    let mut p = project();
    let a = p.grid("A", [0.0, 0.0], [0.0, 10.0]).unwrap();
    p.set(a, NAME, "A'").unwrap();
    p.set(a, START_X, 1.0).unwrap();
    p.set(a, END_X, 1.0).unwrap();
    let line = GridLine::of(&p.store.view(), a).unwrap();
    assert_eq!(line, GridLine::new([1.0, 0.0], [1.0, 10.0]));
    assert_eq!(grid::find(&p.store.view(), "A'"), Some(a));
    assert_eq!(grid::find(&p.store.view(), "A"), None);
}

#[test]
fn every_grid_has_a_unique_label() {
    let mut p = project();
    let a = p.grid("A", [0.0, 0.0], [0.0, 10.0]).unwrap();
    let b = p.grid("B", [5.0, 0.0], [5.0, 10.0]).unwrap();

    assert_eq!(
        broken_rules(p.grid("A", [9.0, 0.0], [9.0, 10.0])),
        ["GridName"]
    );
    assert_eq!(broken_rules(p.set(b, NAME, "A")), ["GridName"]);
    assert_eq!(broken_rules(p.set(b, NAME, " ")), ["GridName"]);

    // Swapping two labels in one transaction is fine.
    let mut tx = p.store.transaction();
    tx.set_property(a, NAME, "B").unwrap();
    tx.set_property(b, NAME, "A").unwrap();
    tx.commit().unwrap();
}

#[test]
fn every_grid_is_a_line_with_two_ends() {
    let mut p = project();
    assert_eq!(
        broken_rules(p.grid("A", [2.0, 3.0], [2.0, 3.0])),
        ["GridLine"]
    );

    let a = p.grid("A", [0.0, 0.0], [0.0, 10.0]).unwrap();
    let mut tx = p.store.transaction();
    tx.clear_property(a, START_X).unwrap();
    assert_eq!(broken_rules(tx.commit()), ["GridLine"]);
}

#[test]
fn new_grids_get_the_next_free_label() {
    let mut p = project();
    layout(&mut p);
    let view = p.store.view();
    assert_eq!(grid::next_free_label(&view, "C").as_deref(), Some("D"));
    assert_eq!(grid::next_free_label(&view, "2").as_deref(), Some("3"));
    // After A, B and C are taken.
    assert_eq!(grid::next_free_label(&view, "A").as_deref(), Some("D"));
}

/// Grid intersections are the snap points for placing elements.
#[test]
fn every_crossing_is_a_snap_point() {
    let mut p = project();
    let ids = layout(&mut p);
    let crossings = grid::intersections(&p.store.view());
    assert_eq!(crossings.len(), 6);

    let mut points: Vec<_> = crossings.iter().map(|c| c.point).collect();
    points.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(
        points,
        [
            [0.0, 0.0],
            [0.0, 4.0],
            [5.0, 0.0],
            [5.0, 4.0],
            [10.0, 0.0],
            [10.0, 4.0]
        ]
    );

    // B and 2 cross at (5, 4).
    let (b, two) = (ids[1], ids[4]);
    let b2 = crossings
        .iter()
        .find(|c| c.grids.contains(&b) && c.grids.contains(&two))
        .unwrap();
    assert_eq!(b2.point, [5.0, 4.0]);

    // Moving grid 2 moves its crossings.
    p.set(two, grid::START_Y, 6.0).unwrap();
    p.set(two, grid::END_Y, 6.0).unwrap();
    let crossings = grid::intersections(&p.store.view());
    // Grid 2 now runs above the letter grids, which end at y = 5.
    assert_eq!(crossings.len(), 3);
}
