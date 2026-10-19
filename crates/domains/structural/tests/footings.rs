//! Isolated footings (#41), through the core's public API only. The
//! Isolated footing tool builds on this.

use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use hito_core::{
    Element, ElementId, ElementStore, Model, ModelId, ModelKind, Relationship, SchemaRegistry,
    StoreError, Value,
};
use hito_geometry::{Csg, GeometryCache, GeometryError, Kernel, Mesh};
use hito_structural::column::{self, COLUMN_TYPE};
use hito_structural::footing::{
    self, BOTTOM_ELEVATION, HEIGHT_OFFSET, Placement, THICKNESS, TOP_ELEVATION, X, Y,
};
use hito_structural::level::{ATTACHED_TO_LEVEL, ELEVATION, LEVEL, LEVEL_OFFSET, NAME};
use hito_structural::section::{self, Section};
use hito_structural::{class, grid, material};

struct Project {
    store: ElementStore,
    model: ModelId,
    foundation: ElementId,
    ground: ElementId,
    /// 1.8 x 1.2 x 0.45 m of H-25.
    pad: ElementId,
    h25: ElementId,
    /// 30 x 30 cm of H-25.
    c30: ElementId,
}

fn project() -> Project {
    let mut registry = SchemaRegistry::new();
    hito_structural::register(&mut registry).unwrap();
    let mut store = ElementStore::new(registry);
    let mut tx = store.transaction();
    let model = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
    let level = |name: &str, elevation: f64| {
        Element::new(class(LEVEL), model)
            .with_property(NAME, name)
            .with_property(ELEVATION, elevation)
    };
    let foundation = tx.insert(level("Fundación", -1.5)).unwrap();
    let ground = tx.insert(level("Planta baja", 0.0)).unwrap();
    let h25 = Element::new(class(material::CONCRETE), model).with_property(material::NAME, "H-25");
    let h25 = tx.insert(h25).unwrap();
    let square = Section::Rectangle {
        width: 0.3,
        depth: 0.3,
    };
    let c30 = section::insert(&mut tx, model, COLUMN_TYPE, "C 30", square, Some(h25)).unwrap();
    let pad = footing::insert_type(
        &mut tx,
        model,
        "Base 180 x 120 x 45",
        [1.8, 1.2, 0.45],
        Some(h25),
    )
    .unwrap();
    tx.commit().unwrap();
    Project {
        store,
        model,
        foundation,
        ground,
        pad,
        h25,
        c30,
    }
}

impl Project {
    /// A 3 m column on `level` at `at`, its base `offset` above the level.
    fn column(&mut self, level: ElementId, at: [f64; 2], offset: f64) -> ElementId {
        let placement = column::Placement::new(self.c30, level, at).with_base_offset(offset);
        self.place_column(&placement)
    }

    fn place_column(&mut self, placement: &column::Placement) -> ElementId {
        let mut tx = self.store.transaction();
        let id = column::insert(&mut tx, self.model, placement).unwrap();
        tx.commit().unwrap();
        id
    }

    fn footing(&mut self, placement: &Placement) -> Result<ElementId, StoreError> {
        let mut tx = self.store.transaction();
        let id = footing::insert(&mut tx, self.model, placement)?;
        tx.commit()?;
        Ok(id)
    }

    fn set(
        &mut self,
        id: ElementId,
        name: &str,
        value: impl Into<Value>,
    ) -> Result<(), StoreError> {
        let mut tx = self.store.transaction();
        tx.set_property(id, name, value)?;
        tx.commit().map(drop)
    }

    fn real(&self, id: ElementId, name: &str) -> f64 {
        match self.store.property(id, name) {
            Some(Value::Real(x)) => *x,
            other => panic!("{name} is {other:?}"),
        }
    }
}

fn broken_rules(result: Result<impl Sized, StoreError>) -> Vec<String> {
    match result {
        Err(StoreError::Violations(violations)) => violations.into_iter().map(|v| v.rule).collect(),
        Err(other) => panic!("expected rule violations, got {other}"),
        Ok(_) => panic!("expected rule violations, the commit succeeded"),
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn a_footing_type_is_a_length_a_width_a_thickness_and_a_material() {
    let p = project();
    let view = p.store.view();
    for (name, value) in [
        (footing::LENGTH, 1.8),
        (footing::WIDTH, 1.2),
        (THICKNESS, 0.45),
    ] {
        assert_eq!(view.property(p.pad, name), Some(&Value::Real(value)));
    }
    assert_eq!(footing::material(&view, p.pad), Some(p.h25));
}

#[test]
fn footing_types_need_a_unique_name_and_a_size() {
    let mut p = project();
    let mut insert = |name: &str, size: [f64; 3]| {
        let mut tx = p.store.transaction();
        footing::insert_type(&mut tx, p.model, name, size, None).unwrap();
        broken_rules(tx.commit())
    };
    assert_eq!(
        insert("Base 180 x 120 x 45", [1.0, 1.0, 0.3]),
        ["FootingTypeName"]
    );
    assert_eq!(insert("", [1.0, 1.0, 0.3]), ["FootingTypeName"]);
    assert_eq!(insert("Plana", [1.0, 1.0, 0.0]), ["FootingTypeSize"]);
    assert_eq!(insert("Angosta", [1.0, -1.0, 0.3]), ["FootingTypeSize"]);
}

#[test]
fn a_free_footing_hangs_from_its_level_and_follows_it() {
    let mut p = project();
    let placement = Placement::at(p.pad, p.foundation, [3.0, 4.0]);
    let base = p.footing(&placement).unwrap();
    assert!(close(p.real(base, TOP_ELEVATION), -1.5));
    assert!(close(p.real(base, BOTTOM_ELEVATION), -1.95));
    assert_eq!(footing::centre(&p.store.view(), base), Some([3.0, 4.0]));

    p.set(p.foundation, ELEVATION, -1.8).unwrap();
    assert!(close(p.real(base, TOP_ELEVATION), -1.8));
    p.set(base, HEIGHT_OFFSET, 0.1).unwrap();
    assert!(close(p.real(base, TOP_ELEVATION), -1.7));
    p.set(p.pad, THICKNESS, 0.6).unwrap();
    assert!(close(p.real(base, BOTTOM_ELEVATION), -2.3));

    let offset = p.footing(&Placement::at(p.pad, p.ground, [0.0, 0.0]).with_offset(-1.2));
    let (bottom, top) = footing::elevations(&p.store.view(), offset.unwrap()).unwrap();
    assert!(close(top, -1.2) && close(bottom, -1.8));
}

#[test]
fn a_footing_under_a_column_follows_it() {
    let mut p = project();
    let column = p.column(p.foundation, [6.0, 0.0], 0.0);
    let base = p.footing(&Placement::under(p.pad, column)).unwrap();
    let view = p.store.view();
    assert_eq!(footing::column(&view, base), Some(column));
    assert_eq!(footing::footings_under(&view, column), [base]);
    assert_eq!(
        hito_structural::level::level_of(&view, base),
        Some(p.foundation)
    );
    assert_eq!(footing::centre(&view, base), Some([6.0, 0.0]));
    assert!(close(p.real(base, X), 6.0) && close(p.real(base, Y), 0.0));
    assert!(close(p.real(base, TOP_ELEVATION), -1.5));

    // The column moves, and so does its footing, in the same transaction.
    let mut tx = p.store.transaction();
    tx.set_property(column, column::X, 7.5).unwrap();
    tx.set_property(column, LEVEL_OFFSET, -0.2).unwrap();
    let changes = tx.commit().unwrap();
    assert!(changes.elements().iter().any(|c| c.id() == base));
    assert!(close(p.real(base, X), 7.5));
    assert!(close(p.real(base, TOP_ELEVATION), -1.7));
    assert!(close(p.real(base, BOTTOM_ELEVATION), -2.15));

    // Deleting the column leaves the footing where it was.
    let mut tx = p.store.transaction();
    assert_eq!(tx.delete(column).unwrap(), [column]);
    tx.commit().unwrap();
    assert_eq!(footing::column(&p.store.view(), base), None);
    assert_eq!(footing::centre(&p.store.view(), base), Some([7.5, 0.0]));
}

#[test]
fn a_footing_needs_a_type_a_position_and_one_column_at_most() {
    let mut p = project();
    let mut tx = p.store.transaction();
    let lost = tx
        .insert(Element::new(class(footing::FOOTING), p.model).with_type(p.pad))
        .unwrap();
    tx.relate(Relationship::new(class(ATTACHED_TO_LEVEL), lost, p.ground))
        .unwrap();
    assert_eq!(broken_rules(tx.commit()), ["Footing"]);

    let a = p.column(p.ground, [0.0, 0.0], 0.0);
    let b = p.column(p.ground, [5.0, 0.0], 0.0);
    let base = p.footing(&Placement::under(p.pad, a)).unwrap();
    let mut tx = p.store.transaction();
    tx.relate(Relationship::new(class(footing::UNDER), base, b))
        .unwrap();
    assert_eq!(broken_rules(tx.commit()), ["Footing"]);
}

#[test]
fn only_columns_have_footings_under_them_and_they_move_with_the_grids() {
    let mut p = project();
    // A footing can't go under a level, or another footing.
    let free = p
        .footing(&Placement::at(p.pad, p.ground, [0.0, 0.0]))
        .unwrap();
    let mut tx = p.store.transaction();
    let other = footing::insert(
        &mut tx,
        p.model,
        &Placement::at(p.pad, p.ground, [5.0, 0.0]),
    )
    .unwrap();
    let under = Relationship::new(class(footing::UNDER), other, free);
    assert!(matches!(
        tx.relate(under),
        Err(StoreError::InvalidRelationship(_))
    ));
    tx.roll_back();

    // A column on grids: its footing follows when a grid moves.
    let mut tx = p.store.transaction();
    let a = tx
        .insert(grid::GridLine::new([2.0, -5.0], [2.0, 5.0]).element(p.model, "A"))
        .unwrap();
    let one = tx
        .insert(grid::GridLine::new([-5.0, 1.0], [5.0, 1.0]).element(p.model, "1"))
        .unwrap();
    tx.commit().unwrap();
    let placement = column::Placement::new(p.c30, p.foundation, [2.0, 1.0]).on_grids([a, one]);
    let on_grids = p.place_column(&placement);
    let base = p.footing(&Placement::under(p.pad, on_grids)).unwrap();
    p.set(a, grid::START_X, 3.0).unwrap();
    p.set(a, grid::END_X, 3.0).unwrap();
    assert_eq!(footing::centre(&p.store.view(), base), Some([3.0, 1.0]));
}

struct NoKernel;

impl Kernel for NoKernel {
    fn evaluate(&self, _: &Csg) -> Result<Mesh, GeometryError> {
        Err(GeometryError::Kernel("no kernel in this test".into()))
    }
}

#[test]
fn a_footing_is_a_box_turned_as_it_says() {
    let mut p = project();
    let placement = Placement::at(p.pad, p.foundation, [10.0, 20.0]).with_rotation(FRAC_PI_2);
    let base = p.footing(&placement).unwrap();
    let column = p.column(p.ground, [0.0, 0.0], -0.5);
    let under = p.footing(&Placement::under(p.pad, column)).unwrap();
    let mut cache = GeometryCache::new(Arc::new(NoKernel), hito_structural::generators());
    cache.rebuild(&p.store);

    let mesh = cache.mesh(base).unwrap();
    assert!(mesh.is_closed());
    assert!((mesh.volume() - 1.8 * 1.2 * 0.45).abs() < 1e-9);
    let (lo, hi) = mesh.bounds().unwrap();
    // Turned a quarter: its length runs along y.
    assert!(close(hi[0] - lo[0], 1.2) && close(hi[1] - lo[1], 1.8));
    assert!(close(lo[2], -1.95) && close(hi[2], -1.5));

    // The footing under a column moves with it.
    let mut tx = p.store.transaction();
    tx.set_property(column, column::Y, 3.0).unwrap();
    let changes = tx.commit().unwrap();
    assert!(cache.update(&p.store, &changes).updated.contains(&under));
    let (lo, hi) = cache.mesh(under).unwrap().bounds().unwrap();
    assert!(close((lo[1] + hi[1]) / 2.0, 3.0));
    assert!(close(hi[2], -0.5));
}
