//! A small project to draw walls in: two levels, materials, two wall types
//! and a test slab class to attach walls to.

#![allow(dead_code)]

use hito_architectural::footprint::{self, Footprint};
use hito_architectural::plan::{Point, area, overlap};
use hito_architectural::wall::{self, Placement, Top};
use hito_architectural::wall_type::{self, Function, Layer};
use hito_core::{
    ClassDef, ClassId, Element, ElementId, ElementStore, Model, ModelId, ModelKind, Schema,
    SchemaRegistry, SchemaVersion, StoreError, Value,
};
use hito_structural::level::{self, HORIZONTAL_ELEMENT};
use hito_structural::material::{self, CONCRETE, MATERIAL};

pub struct Project {
    pub store: ElementStore,
    pub model: ModelId,
    pub ground: ElementId,
    pub first: ElementId,
    /// 2 cm render, 18 cm brick, 2 cm plaster: 22 cm.
    pub brick: ElementId,
    /// 20 cm of concrete.
    pub concrete: ElementId,
    pub render: ElementId,
    pub hollow_brick: ElementId,
    pub plaster: ElementId,
    pub h25: ElementId,
}

/// A slab, as #39 will add.
pub fn slab_class() -> ClassId {
    ClassId::new("TestSlabs", "Slab")
}

pub fn project() -> Project {
    let mut registry = SchemaRegistry::new();
    hito_architectural::register(&mut registry).unwrap();
    let slabs = Schema::new("TestSlabs", SchemaVersion::new(1, 0, 0)).class(ClassDef::new(
        "Slab",
        hito_structural::class(HORIZONTAL_ELEMENT),
    ));
    registry.register(slabs).unwrap();
    let mut store = ElementStore::new(registry);
    let mut tx = store.transaction();
    let model = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
    let level = |name: &str, elevation: f64| {
        Element::new(hito_structural::class(level::LEVEL), model)
            .with_property(level::NAME, name)
            .with_property(level::ELEVATION, elevation)
    };
    let ground = tx.insert(level("Planta baja", 0.0)).unwrap();
    let first = tx.insert(level("Primer piso", 3.0)).unwrap();
    let material = |class: &str, name: &str| {
        Element::new(hito_structural::class(class), model).with_property(material::NAME, name)
    };
    let render = tx.insert(material(MATERIAL, "Revoque exterior")).unwrap();
    let hollow_brick = tx.insert(material(MATERIAL, "Ladrillo hueco")).unwrap();
    let plaster = tx.insert(material(MATERIAL, "Revoque interior")).unwrap();
    let h25 = tx.insert(material(CONCRETE, "H-25")).unwrap();
    let brick = wall_type::insert(
        &mut tx,
        model,
        "Ladrillo 18 revocado",
        &[
            Layer::new(Function::Finish1, 0.02).with_material(render),
            Layer::new(Function::Structure, 0.18).with_material(hollow_brick),
            Layer::new(Function::Finish2, 0.02).with_material(plaster),
        ],
    )
    .unwrap();
    let concrete = wall_type::insert(
        &mut tx,
        model,
        "Hormigón 20",
        &[Layer::new(Function::Structure, 0.2).with_material(h25)],
    )
    .unwrap();
    tx.commit().unwrap();
    Project {
        store,
        model,
        ground,
        first,
        brick,
        concrete,
        render,
        hollow_brick,
        plaster,
        h25,
    }
}

impl Project {
    /// A 3 m unconnected wall of `wall_type` on the ground floor.
    pub fn placement(&self, wall_type: ElementId, start: Point, end: Point) -> Placement {
        Placement::new(wall_type, self.ground, start, end)
    }

    pub fn wall(&mut self, wall_type: ElementId, start: Point, end: Point) -> ElementId {
        let placement = self.placement(wall_type, start, end);
        self.place(&placement).unwrap()
    }

    pub fn place(&mut self, placement: &Placement) -> Result<ElementId, StoreError> {
        let mut tx = self.store.transaction();
        let id = wall::insert(&mut tx, self.model, placement)?;
        tx.commit()?;
        Ok(id)
    }

    /// Walls drawn as one chain through `points`, in one transaction.
    pub fn chain(&mut self, wall_type: ElementId, points: &[Point]) -> Vec<ElementId> {
        let mut tx = self.store.transaction();
        let mut walls = Vec::new();
        for pair in points.windows(2) {
            let placement = Placement::new(wall_type, self.ground, pair[0], pair[1]);
            walls.push(wall::insert(&mut tx, self.model, &placement).unwrap());
        }
        tx.commit().unwrap();
        walls
    }

    pub fn set(
        &mut self,
        id: ElementId,
        name: &str,
        value: impl Into<Value>,
    ) -> Result<(), StoreError> {
        let mut tx = self.store.transaction();
        tx.set_property(id, name, value)?;
        tx.commit().map(|_| ())
    }

    pub fn real(&self, id: ElementId, name: &str) -> f64 {
        match self.store.property(id, name) {
            Some(Value::Real(x)) => *x,
            other => panic!("{name} is {other:?}"),
        }
    }

    pub fn footprint(&self, wall: ElementId) -> Footprint {
        footprint::footprint(&self.store.view(), wall).expect("the wall is complete")
    }

    /// The outlines of every layer of `walls`, in their single slice.
    pub fn pieces(&self, walls: &[ElementId]) -> Vec<Vec<Point>> {
        walls
            .iter()
            .flat_map(|&w| {
                let footprint = self.footprint(w);
                assert_eq!(footprint.slices.len(), 1, "one slice");
                footprint.slices[0]
                    .layers
                    .iter()
                    .flat_map(|l| l.polygons.clone())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The polygons of layer `i` of `wall`, in its single slice.
    pub fn layer(&self, wall: ElementId, i: usize) -> Vec<Vec<Point>> {
        let footprint = self.footprint(wall);
        assert_eq!(footprint.slices.len(), 1, "one slice");
        footprint.slices[0].layers[i].polygons.clone()
    }
}

/// Total area of some polygons.
pub fn total(polygons: &[Vec<Point>]) -> f64 {
    polygons.iter().map(|p| area(p)).sum()
}

/// Checks that no two of the polygons overlap.
pub fn assert_disjoint(polygons: &[Vec<Point>]) {
    for (i, a) in polygons.iter().enumerate() {
        for b in &polygons[i + 1..] {
            let shared = overlap(a, b);
            assert!(shared < 1e-9, "{a:?} and {b:?} overlap by {shared} m²");
        }
    }
}

pub fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// The names of the rules a failed commit broke.
pub fn broken_rules(result: Result<impl Sized, StoreError>) -> Vec<String> {
    match result {
        Err(StoreError::Violations(violations)) => violations.into_iter().map(|v| v.rule).collect(),
        Err(other) => panic!("expected rule violations, got {other}"),
        Ok(_) => panic!("expected rule violations, the commit succeeded"),
    }
}

pub fn unconnected(height: f64) -> Top {
    Top::Unconnected { height }
}
