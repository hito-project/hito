//! Slabs (#39), through the core's public API only. The Slab tool builds
//! on this.

use std::sync::Arc;

use hito_core::{
    Element, ElementId, ElementStore, Model, ModelId, ModelKind, SchemaRegistry, StoreError, Value,
};
use hito_geometry::{Csg, GeometryCache, GeometryError, Kernel, Mesh};
use hito_structural::level::{ELEVATION, LEVEL, NAME};
use hito_structural::slab::{
    self, BOTTOM_ELEVATION, HEIGHT_OFFSET, Placement, THICKNESS, TOP_ELEVATION,
};
use hito_structural::{class, material};

struct Project {
    store: ElementStore,
    model: ModelId,
    ground: ElementId,
    first: ElementId,
    /// 12 cm of H-25.
    losa: ElementId,
    h25: ElementId,
}

const ROOM: [[f64; 2]; 4] = [[0.0, 0.0], [6.0, 0.0], [6.0, 4.0], [0.0, 4.0]];
const STAIRWELL: [[f64; 2]; 4] = [[1.0, 1.0], [2.0, 1.0], [2.0, 3.0], [1.0, 3.0]];

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
    let ground = tx.insert(level("Planta baja", 0.0)).unwrap();
    let first = tx.insert(level("Primer piso", 3.0)).unwrap();
    let h25 = Element::new(class(material::CONCRETE), model).with_property(material::NAME, "H-25");
    let h25 = tx.insert(h25).unwrap();
    let losa = slab::insert_type(&mut tx, model, "Losa H-25 12 cm", 0.12, Some(h25)).unwrap();
    tx.commit().unwrap();
    Project {
        store,
        model,
        ground,
        first,
        losa,
        h25,
    }
}

impl Project {
    fn slab(&mut self, placement: &Placement) -> Result<ElementId, StoreError> {
        let mut tx = self.store.transaction();
        let id = slab::insert(&mut tx, self.model, placement)?;
        tx.commit()?;
        Ok(id)
    }

    fn opening(&mut self, slab: ElementId, outline: &[[f64; 2]]) -> Result<ElementId, StoreError> {
        let mut tx = self.store.transaction();
        let id = slab::insert_opening(&mut tx, self.model, slab, outline)?;
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

/// The names of the rules a failed commit broke.
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
fn a_slab_type_is_a_thickness_and_a_material() {
    let p = project();
    let view = p.store.view();
    assert_eq!(slab::thickness(&view, p.losa), Some(0.12));
    assert_eq!(slab::material(&view, p.losa), Some(p.h25));
}

#[test]
fn slab_types_need_a_unique_name_and_a_thickness() {
    let mut p = project();
    let mut insert = |name: &str, thickness: f64| {
        let mut tx = p.store.transaction();
        slab::insert_type(&mut tx, p.model, name, thickness, None).unwrap();
        broken_rules(tx.commit())
    };
    assert_eq!(insert("Losa H-25 12 cm", 0.15), ["SlabTypeName"]);
    assert_eq!(insert(" ", 0.15), ["SlabTypeName"]);
    assert_eq!(insert("Losa 0", 0.0), ["SlabTypeThickness"]);
    assert_eq!(insert("Losa negativa", -0.1), ["SlabTypeThickness"]);
}

#[test]
fn a_slab_hangs_from_its_level_by_its_thickness() {
    let mut p = project();
    let floor = p.slab(&Placement::new(p.losa, p.first, &ROOM)).unwrap();
    assert!(close(p.real(floor, TOP_ELEVATION), 3.0));
    assert!(close(p.real(floor, BOTTOM_ELEVATION), 2.88));
    assert_eq!(slab::boundary(&p.store.view(), floor), ROOM);

    let raised = Placement::new(p.losa, p.ground, &ROOM).with_offset(0.05);
    let raised = p.slab(&raised).unwrap();
    let (bottom, top) = slab::elevations(&p.store.view(), raised).unwrap();
    assert!(close(bottom, -0.07) && close(top, 0.05));
}

#[test]
fn a_slab_follows_its_level_its_offset_and_its_type() {
    let mut p = project();
    let floor = p.slab(&Placement::new(p.losa, p.first, &ROOM)).unwrap();

    p.set(p.first, ELEVATION, 3.2).unwrap();
    assert!(close(p.real(floor, TOP_ELEVATION), 3.2));
    assert!(close(p.real(floor, BOTTOM_ELEVATION), 3.08));

    p.set(floor, HEIGHT_OFFSET, -0.1).unwrap();
    assert!(close(p.real(floor, TOP_ELEVATION), 3.1));

    // A thicker type moves every slab of that type down at the bottom.
    p.set(p.losa, THICKNESS, 0.2).unwrap();
    assert!(close(p.real(floor, BOTTOM_ELEVATION), 2.9));

    // Deleting the level deletes the slab, as in Revit.
    let mut tx = p.store.transaction();
    let deleted = tx.delete(p.first).unwrap();
    tx.commit().unwrap();
    assert!(deleted.contains(&floor));
}

#[test]
fn a_slab_needs_a_simple_boundary_and_a_type() {
    let mut p = project();
    let bow_tie = [[0.0, 0.0], [4.0, 4.0], [4.0, 0.0], [0.0, 4.0]];
    let result = p.slab(&Placement::new(p.losa, p.first, &bow_tie));
    assert_eq!(broken_rules(result), ["SlabBoundary"]);
    let line = [[0.0, 0.0], [4.0, 0.0]];
    let result = p.slab(&Placement::new(p.losa, p.first, &line));
    assert_eq!(broken_rules(result), ["SlabBoundary"]);

    let mut tx = p.store.transaction();
    let untyped = Element::new(class(slab::SLAB), p.model);
    let untyped = tx.insert(untyped).unwrap();
    let attached = class(hito_structural::level::ATTACHED_TO_LEVEL);
    tx.relate(hito_core::Relationship::new(attached, untyped, p.first))
        .unwrap();
    let mut broken = broken_rules(tx.commit());
    broken.sort();
    assert_eq!(broken, ["SlabBoundary", "SlabType"]);
}

#[test]
fn openings_go_inside_their_slab_and_apart() {
    let mut p = project();
    let floor = p.slab(&Placement::new(p.losa, p.first, &ROOM)).unwrap();
    let stairwell = p.opening(floor, &STAIRWELL).unwrap();
    assert_eq!(slab::openings(&p.store.view(), floor), [stairwell]);
    assert_eq!(slab::host(&p.store.view(), stairwell), Some(floor));

    let across = [[5.0, 1.0], [7.0, 1.0], [7.0, 2.0], [5.0, 2.0]];
    assert_eq!(broken_rules(p.opening(floor, &across)), ["SlabOpening"]);
    let overlapping = [[1.5, 2.0], [3.0, 2.0], [3.0, 3.5], [1.5, 3.5]];
    assert_eq!(
        broken_rules(p.opening(floor, &overlapping)),
        ["SlabOpening"]
    );
    let shaft = [[4.0, 1.0], [5.0, 1.0], [5.0, 2.0], [4.0, 2.0]];
    p.opening(floor, &shaft).unwrap();
    assert_eq!(slab::openings(&p.store.view(), floor).len(), 2);

    // An opening is found by where it's drawn: the slab around it.
    let view = p.store.view();
    assert_eq!(slab::slabs_around(&view, p.first, &shaft), [floor]);
    assert!(slab::slabs_around(&view, p.ground, &shaft).is_empty());
    assert!(slab::slabs_around(&view, p.first, &across).is_empty());

    // Deleting the slab deletes its openings.
    let mut tx = p.store.transaction();
    let deleted = tx.delete(floor).unwrap();
    tx.commit().unwrap();
    assert_eq!(deleted.len(), 3);
    assert!(p.store.get(stairwell).is_none());
}

/// Slabs have no booleans: openings are holes in the profile.
struct NoKernel;

impl Kernel for NoKernel {
    fn evaluate(&self, _: &Csg) -> Result<Mesh, GeometryError> {
        Err(GeometryError::Kernel("no kernel in this test".into()))
    }
}

#[test]
fn a_slab_is_its_boundary_less_its_openings_extruded_by_its_thickness() {
    let mut p = project();
    // Far from the origin, as in real coordinates.
    let far = |o: &[[f64; 2]]| -> Vec<[f64; 2]> {
        o.iter()
            .map(|[x, y]| [x + 5_500_000.0, y + 6_100_000.0])
            .collect()
    };
    let floor = p
        .slab(&Placement::new(p.losa, p.first, &far(&ROOM)))
        .unwrap();
    let mut cache = GeometryCache::new(Arc::new(NoKernel), hito_structural::generators());
    cache.rebuild(&p.store);
    let mesh = cache.mesh(floor).unwrap();
    assert!(mesh.is_closed());
    assert!((mesh.volume() - 24.0 * 0.12).abs() < 1e-6);
    let (lo, hi) = mesh.bounds().unwrap();
    assert!(close(lo[2], 2.88) && close(hi[2], 3.0));

    let mut tx = p.store.transaction();
    slab::insert_opening(&mut tx, p.model, floor, &far(&STAIRWELL)).unwrap();
    let changes = tx.commit().unwrap();
    let regenerated = cache.update(&p.store, &changes);
    assert!(regenerated.updated.contains(&floor));
    let mesh = cache.mesh(floor).unwrap();
    assert!(mesh.is_closed());
    assert!((mesh.volume() - 22.0 * 0.12).abs() < 1e-6);

    // Moving the level moves the slab's geometry.
    let mut tx = p.store.transaction();
    tx.set_property(p.first, ELEVATION, 3.5).unwrap();
    let changes = tx.commit().unwrap();
    cache.update(&p.store, &changes);
    let (lo, _) = cache.mesh(floor).unwrap().bounds().unwrap();
    assert!(close(lo[2], 3.38));
}
