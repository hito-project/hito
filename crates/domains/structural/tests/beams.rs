//! Beams (#38), through the public API only.

use std::f64::consts::FRAC_PI_4;
use std::sync::Arc;

use hito_core::{
    Element, ElementId, ElementStore, Model, ModelId, ModelKind, Relationship, SchemaRegistry,
    StoreError, Value,
};
use hito_geometry::{Csg, GeometryCache, GeometryError, Kernel, Mesh};
use hito_structural::beam::{self, BEAM_TYPE, End, Placement};
use hito_structural::column::{self, COLUMN_TYPE, Top};
use hito_structural::section::{self, Section};
use hito_structural::{class, grid, level};

struct Project {
    store: ElementStore,
    model: ModelId,
    ground: ElementId,
    first: ElementId,
    /// 30 x 30 cm.
    square: ElementId,
    /// 40 cm across.
    round: ElementId,
    /// 20 x 50 cm.
    v20: ElementId,
}

fn project() -> Project {
    let mut registry = SchemaRegistry::new();
    hito_structural::register(&mut registry).unwrap();
    let mut store = ElementStore::new(registry);
    let mut tx = store.transaction();
    let model = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
    let level = |name: &str, elevation: f64| {
        Element::new(class(level::LEVEL), model)
            .with_property(level::NAME, name)
            .with_property(level::ELEVATION, elevation)
    };
    let ground = tx.insert(level("Planta baja", 0.0)).unwrap();
    let first = tx.insert(level("Primer piso", 3.0)).unwrap();
    let rect = |width, depth| Section::Rectangle { width, depth };
    let square =
        section::insert(&mut tx, model, COLUMN_TYPE, "C 30", rect(0.3, 0.3), None).unwrap();
    let circle = Section::Circle { diameter: 0.4 };
    let round = section::insert(&mut tx, model, COLUMN_TYPE, "C 40", circle, None).unwrap();
    let v20 =
        section::insert(&mut tx, model, BEAM_TYPE, "V 20 x 50", rect(0.2, 0.5), None).unwrap();
    tx.commit().unwrap();
    Project {
        store,
        model,
        ground,
        first,
        square,
        round,
        v20,
    }
}

impl Project {
    /// A column of `column_type` from the ground floor up to the first.
    fn column(&mut self, column_type: ElementId, at: [f64; 2]) -> ElementId {
        let placement = column::Placement::new(column_type, self.ground, at).with_top(Top::Level {
            level: self.first,
            offset: 0.0,
        });
        let mut tx = self.store.transaction();
        let id = column::insert(&mut tx, self.model, &placement).unwrap();
        tx.commit().unwrap();
        id
    }

    fn beam(&mut self, start: End, end: End) -> Result<ElementId, StoreError> {
        let placement = Placement::new(self.v20, self.first, start, end);
        let mut tx = self.store.transaction();
        let id = beam::insert(&mut tx, self.model, &placement)?;
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

    fn line(&self, beam: ElementId) -> ([f64; 2], [f64; 2]) {
        beam::line(&self.store.view(), beam).unwrap()
    }

    fn mesh_bounds(&self, beam: ElementId) -> Option<([f64; 3], [f64; 3])> {
        let mut cache = GeometryCache::new(Arc::new(NoKernel), hito_structural::generators());
        cache.rebuild(&self.store);
        cache.mesh(beam)?.bounds()
    }
}

/// A kernel for extrusions and sweeps only: they never reach it.
struct NoKernel;

impl Kernel for NoKernel {
    fn evaluate(&self, _: &Csg) -> Result<Mesh, GeometryError> {
        unreachable!("a beam is one sweep")
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn rule_broken(result: Result<impl Sized, StoreError>, rule: &str) -> bool {
    match result {
        Err(StoreError::Violations(v)) => v.iter().any(|v| v.rule == rule),
        _ => false,
    }
}

#[test]
fn a_beam_between_columns_runs_centre_to_centre() {
    let mut p = project();
    let a = p.column(p.square, [0.0, 0.0]);
    let b = p.column(p.square, [6.0, 0.0]);
    let beam = p.beam(End::Column(a), End::Column(b)).unwrap();
    assert_eq!(p.line(beam), ([0.0, 0.0], [6.0, 0.0]));
    assert_eq!(beam::columns(&p.store.view(), beam), [Some(a), Some(b)]);
    assert_eq!(
        p.store.property(beam, level::BASE_ELEVATION),
        Some(&Value::Real(3.0))
    );
}

#[test]
fn beams_follow_the_columns_they_frame_into() {
    let mut p = project();
    let a = p.column(p.square, [0.0, 0.0]);
    let b = p.column(p.square, [6.0, 0.0]);
    let beam = p.beam(End::Column(a), End::Column(b)).unwrap();
    p.set(b, column::Y, 1.0).unwrap();
    assert_eq!(p.line(beam), ([0.0, 0.0], [6.0, 1.0]));

    // And so through the grids the columns follow.
    let mut tx = p.store.transaction();
    let model = p.model;
    let grid_a = tx
        .insert(grid::GridLine::new([2.0, -1.0], [2.0, 9.0]).element(model, "A"))
        .unwrap();
    let grid_1 = tx
        .insert(grid::GridLine::new([-1.0, 4.0], [9.0, 4.0]).element(model, "1"))
        .unwrap();
    let placement = column::Placement::new(p.square, p.ground, [2.0, 4.0])
        .with_top(Top::Level {
            level: p.first,
            offset: 0.0,
        })
        .on_grids([grid_a, grid_1]);
    let on_grids = column::insert(&mut tx, model, &placement).unwrap();
    tx.commit().unwrap();
    let second = p.beam(End::Column(a), End::Column(on_grids)).unwrap();
    p.set(grid_1, grid::START_Y, 5.0).unwrap();
    p.set(grid_1, grid::END_Y, 5.0).unwrap();
    assert_eq!(p.line(second), ([0.0, 0.0], [2.0, 5.0]));
}

#[test]
fn deleting_a_column_leaves_the_beam_where_it_was() {
    let mut p = project();
    let a = p.column(p.square, [0.0, 0.0]);
    let beam = p.beam(End::Column(a), End::Point([4.0, 0.0])).unwrap();
    let mut tx = p.store.transaction();
    tx.delete(a).unwrap();
    tx.commit().unwrap();
    assert!(p.store.get(beam).is_some());
    assert_eq!(p.line(beam), ([0.0, 0.0], [4.0, 0.0]));
    assert_eq!(beam::columns(&p.store.view(), beam), [None, None]);
}

#[test]
fn beams_follow_their_level() {
    let mut p = project();
    let beam = p
        .beam(End::Point([0.0, 0.0]), End::Point([4.0, 0.0]))
        .unwrap();
    p.set(p.first, level::ELEVATION, 3.2).unwrap();
    assert_eq!(
        p.store.property(beam, level::BASE_ELEVATION),
        Some(&Value::Real(3.2))
    );
    let (min, max) = p.mesh_bounds(beam).unwrap();
    assert!(close(max[2], 3.2) && close(min[2], 2.7), "{min:?} {max:?}");
}

#[test]
fn the_body_stops_at_the_columns_faces() {
    let mut p = project();
    let a = p.column(p.square, [0.0, 0.0]);
    let b = p.column(p.round, [6.0, 0.0]);
    let beam = p.beam(End::Column(a), End::Column(b)).unwrap();
    let (min, max) = p.mesh_bounds(beam).unwrap();
    // From the square's face at 0.15 to the circle's at 5.8, 20 cm wide and
    // 50 cm deep, hanging from the first floor.
    let expected = ([0.15, -0.1, 2.5], [5.8, 0.1, 3.0]);
    for i in 0..3 {
        assert!(close(min[i], expected.0[i]), "{min:?}");
        assert!(close(max[i], expected.1[i]), "{max:?}");
    }

    // Turned 45°, the square reaches its corner along the beam.
    p.set(a, column::ROTATION, FRAC_PI_4).unwrap();
    let (min, _) = p.mesh_bounds(beam).unwrap();
    assert!(close(min[0], 0.15 * 2f64.sqrt()), "{min:?}");

    // A free end isn't trimmed.
    let free = p
        .beam(End::Point([0.0, 3.0]), End::Point([4.0, 3.0]))
        .unwrap();
    let (min, max) = p.mesh_bounds(free).unwrap();
    assert!(close(min[0], 0.0) && close(max[0], 4.0));
}

#[test]
fn a_beam_with_no_room_between_its_columns_has_no_body() {
    let mut p = project();
    let a = p.column(p.square, [0.0, 0.0]);
    let b = p.column(p.square, [0.3, 0.0]);
    let beam = p.beam(End::Column(a), End::Column(b)).unwrap();
    assert!(p.mesh_bounds(beam).is_none());
}

#[test]
fn a_beam_end_frames_into_a_column_it_lands_on() {
    let mut p = project();
    let square = p.column(p.square, [0.0, 0.0]);
    let round = p.column(p.round, [6.0, 0.0]);
    let view = p.store.view();
    // Anywhere on the column's section, at an elevation it reaches.
    assert_eq!(beam::column_at(&view, [0.1, -0.12], 3.0), Some(square));
    assert_eq!(beam::column_at(&view, [6.0, 0.19], 3.0), Some(round));
    assert_eq!(beam::column_at(&view, [6.19, 0.19], 3.0), None);
    assert_eq!(beam::column_at(&view, [0.0, 0.0], 1.5), Some(square));
    // Not above its top, nor at its base.
    assert_eq!(beam::column_at(&view, [0.0, 0.0], 3.5), None);
    assert_eq!(beam::column_at(&view, [0.0, 0.0], 0.0), None);
}

#[test]
fn beams_need_a_rectangular_type_and_two_different_ends() {
    let mut p = project();
    assert!(rule_broken(
        p.beam(End::Point([1.0, 1.0]), End::Point([1.0, 1.0005])),
        "BeamLine"
    ));
    let a = p.column(p.square, [0.0, 0.0]);
    assert!(rule_broken(
        p.beam(End::Column(a), End::Column(a)),
        "BeamEnds"
    ));

    let mut tx = p.store.transaction();
    let circle = Section::Circle { diameter: 0.3 };
    section::insert(&mut tx, p.model, BEAM_TYPE, "Round", circle, None).unwrap();
    assert!(rule_broken(tx.commit(), "BeamSection"));

    let beam = p
        .beam(End::Point([0.0, 0.0]), End::Point([4.0, 0.0]))
        .unwrap();
    let b = p.column(p.square, [4.0, 0.0]);
    let c = p.column(p.square, [4.0, 1.0]);
    let mut tx = p.store.transaction();
    for column in [b, c] {
        tx.relate(Relationship::new(class(beam::END_COLUMN), beam, column))
            .unwrap();
    }
    assert!(rule_broken(tx.commit(), "BeamEnds"));
}
