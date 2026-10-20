//! Columns (#37) and their section types, through the public API only.

use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use hito_core::{
    Element, ElementId, ElementStore, Model, ModelId, ModelKind, Relationship, SchemaRegistry,
    StoreError, Value,
};
use hito_geometry::{Csg, GeometryCache, GeometryError, Kernel, Mesh};
use hito_structural::column::{self, COLUMN_TYPE, Placement, Top};
use hito_structural::section::{self, SECTION_MATERIAL, Section, Shape};
use hito_structural::{class, grid, level, material};

struct Project {
    store: ElementStore,
    model: ModelId,
    ground: ElementId,
    first: ElementId,
    /// 30 x 40 cm, of H-25.
    rectangular: ElementId,
    /// 40 cm across.
    circular: ElementId,
    h25: ElementId,
}

fn project() -> Project {
    let mut registry = SchemaRegistry::new();
    hito_structural::register(&mut registry).unwrap();
    let mut store = ElementStore::new(registry);
    let mut tx = store.transaction();
    let model = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
    let definitions = tx.insert_model(Model::new(ModelKind::Definition)).unwrap();
    let level = |name: &str, elevation: f64| {
        Element::new(class(level::LEVEL), model)
            .with_property(level::NAME, name)
            .with_property(level::ELEVATION, elevation)
    };
    let ground = tx.insert(level("Planta baja", 0.0)).unwrap();
    let first = tx.insert(level("Primer piso", 3.0)).unwrap();
    let h25 = Element::new(class(material::CONCRETE), definitions)
        .with_property(material::NAME, "H-25")
        .with_property(material::COMPRESSIVE_STRENGTH, 25e6);
    let h25 = tx.insert(h25).unwrap();
    let rect = Section::Rectangle {
        width: 0.3,
        depth: 0.4,
    };
    let rectangular = section::insert(
        &mut tx,
        definitions,
        COLUMN_TYPE,
        "C 30 x 40",
        rect,
        Some(h25),
    )
    .unwrap();
    let round = Section::Circle { diameter: 0.4 };
    let circular = section::insert(&mut tx, definitions, COLUMN_TYPE, "C 40", round, None).unwrap();
    tx.commit().unwrap();
    Project {
        store,
        model,
        ground,
        first,
        rectangular,
        circular,
        h25,
    }
}

impl Project {
    fn insert(&mut self, placement: Placement) -> Result<ElementId, StoreError> {
        let mut tx = self.store.transaction();
        let id = column::insert(&mut tx, self.model, &placement)?;
        tx.commit()?;
        Ok(id)
    }

    /// A rectangular column on the ground floor, up to the first.
    fn column(&mut self, at: [f64; 2]) -> ElementId {
        let placement = Placement::new(self.rectangular, self.ground, at).with_top(Top::Level {
            level: self.first,
            offset: 0.0,
        });
        self.insert(placement).unwrap()
    }

    fn grid(&mut self, label: &str, start: [f64; 2], end: [f64; 2]) -> ElementId {
        let mut tx = self.store.transaction();
        let id = tx
            .insert(grid::GridLine::new(start, end).element(self.model, label))
            .unwrap();
        tx.commit().unwrap();
        id
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

    fn delete(&mut self, id: ElementId) {
        let mut tx = self.store.transaction();
        tx.delete(id).unwrap();
        tx.commit().unwrap();
    }

    fn real(&self, id: ElementId, name: &str) -> f64 {
        match self.store.property(id, name) {
            Some(Value::Real(x)) => *x,
            other => panic!("{name} is {other:?}"),
        }
    }

    fn location(&self, id: ElementId) -> [f64; 2] {
        column::location(&self.store.view(), id).unwrap()
    }
}

fn close(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
}

fn rule_broken(result: Result<impl Sized, StoreError>, rule: &str) -> bool {
    match result {
        Err(StoreError::Violations(v)) => v.iter().any(|v| v.rule == rule),
        _ => false,
    }
}

#[test]
fn column_types_have_a_section_and_a_material() {
    let p = project();
    let view = p.store.view();
    assert_eq!(
        section::of(&view, p.rectangular),
        Some(Section::Rectangle {
            width: 0.3,
            depth: 0.4
        })
    );
    assert_eq!(
        section::of(&view, p.circular),
        Some(Section::Circle { diameter: 0.4 })
    );
    assert_eq!(section::material_of(&view, p.rectangular), Some(p.h25));
    assert_eq!(section::material_of(&view, p.circular), None);
    assert_eq!(
        p.store.property(p.circular, section::SHAPE),
        Some(&Value::Text(Shape::Circular.name().into()))
    );
}

#[test]
fn a_column_takes_its_section_and_material_from_its_type() {
    let mut p = project();
    let column = p.column([1.0, 2.0]);
    let view = p.store.view();
    assert_eq!(
        section::of(&view, column),
        section::of(&view, p.rectangular)
    );
    assert_eq!(section::material_of(&view, column), Some(p.h25));
}

#[test]
fn section_types_must_be_complete_and_uniquely_named() {
    let mut p = project();
    let mut tx = p.store.transaction();
    let model = p.model;
    let square = Section::Rectangle {
        width: 0.3,
        depth: 0.3,
    };
    section::insert(&mut tx, model, COLUMN_TYPE, "C 30 x 40", square, None).unwrap();
    assert!(rule_broken(tx.commit(), "SectionTypeName"));

    let mut tx = p.store.transaction();
    let flat = Section::Rectangle {
        width: 0.3,
        depth: 0.0,
    };
    section::insert(&mut tx, model, COLUMN_TYPE, "Flat", flat, None).unwrap();
    assert!(rule_broken(tx.commit(), "Section"));

    assert!(rule_broken(
        p.set(p.circular, section::SHAPE, "Hexagonal"),
        "Section"
    ));
    // A circle has no width: switching shape needs the other dimensions.
    assert!(rule_broken(
        p.set(p.circular, section::SHAPE, Shape::Rectangular.name()),
        "Section"
    ));

    let mut tx = p.store.transaction();
    let other =
        Element::new(class(material::CONCRETE), p.model).with_property(material::NAME, "H-30");
    let other = tx.insert(other).unwrap();
    tx.relate(Relationship::new(
        class(SECTION_MATERIAL),
        p.rectangular,
        other,
    ))
    .unwrap();
    assert!(rule_broken(tx.commit(), "SectionMaterial"));
}

#[test]
fn columns_follow_their_levels() {
    let mut p = project();
    let column = p.column([0.0, 0.0]);
    assert_eq!(p.real(column, level::BASE_ELEVATION), 0.0);
    assert_eq!(p.real(column, column::TOP_ELEVATION), 3.0);
    assert_eq!(p.real(column, column::HEIGHT), 3.0);

    p.set(p.first, level::ELEVATION, 3.2).unwrap();
    assert_eq!(p.real(column, column::TOP_ELEVATION), 3.2);
    assert_eq!(p.real(column, column::HEIGHT), 3.2);

    p.set(p.ground, level::ELEVATION, -0.5).unwrap();
    assert_eq!(p.real(column, level::BASE_ELEVATION), -0.5);
    assert!((p.real(column, column::HEIGHT) - 3.7).abs() < 1e-12);
}

#[test]
fn offsets_raise_or_lower_the_base_and_the_top() {
    let mut p = project();
    let placement = Placement::new(p.rectangular, p.ground, [0.0, 0.0])
        .with_base_offset(-1.2)
        .with_top(Top::Level {
            level: p.first,
            offset: 0.5,
        });
    let column = p.insert(placement).unwrap();
    assert_eq!(
        column::elevations(&p.store.view(), column),
        Some((-1.2, 3.5))
    );
    assert_eq!(p.real(column, column::HEIGHT), 4.7);
}

#[test]
fn an_unconnected_column_keeps_its_height() {
    let mut p = project();
    let placement =
        Placement::new(p.circular, p.first, [0.0, 0.0]).with_top(Top::Unconnected { height: 2.5 });
    let column = p.insert(placement).unwrap();
    assert_eq!(
        column::elevations(&p.store.view(), column),
        Some((3.0, 5.5))
    );
    p.set(p.first, level::ELEVATION, 3.5).unwrap();
    assert_eq!(
        column::elevations(&p.store.view(), column),
        Some((3.5, 6.0))
    );
}

#[test]
fn deleting_the_top_level_leaves_the_column_at_its_height() {
    let mut p = project();
    let column = p.column([0.0, 0.0]);
    p.delete(p.first);
    assert!(p.store.get(column).is_some());
    assert_eq!(
        column::elevations(&p.store.view(), column),
        Some((0.0, 3.0))
    );
}

#[test]
fn deleting_the_base_level_deletes_the_column() {
    let mut p = project();
    let column = p.column([0.0, 0.0]);
    p.delete(p.ground);
    assert!(p.store.get(column).is_none());
}

#[test]
fn a_column_on_two_grids_follows_them() {
    let mut p = project();
    let a = p.grid("A", [2.0, -1.0], [2.0, 10.0]);
    let one = p.grid("1", [-1.0, 3.0], [10.0, 3.0]);
    let crossing = grid::intersection_near(&p.store.view(), [2.0005, 3.0], column::TOLERANCE)
        .expect("within 1 mm");
    assert_eq!(crossing.point, [2.0, 3.0]);

    let placement = Placement::new(p.rectangular, p.ground, crossing.point)
        .with_top(Top::Level {
            level: p.first,
            offset: 0.0,
        })
        .on_grids(crossing.grids);
    let column = p.insert(placement).unwrap();
    let mut grids = column::grids(&p.store.view(), column).unwrap();
    grids.sort();
    assert_eq!(grids, [a, one]);

    // Moving grid A moves the column with it, in the same transaction.
    let mut tx = p.store.transaction();
    tx.set_property(a, grid::START_X, 5.0).unwrap();
    tx.set_property(a, grid::END_X, 5.0).unwrap();
    tx.commit().unwrap();
    assert!(close(p.location(column), [5.0, 3.0]));
    p.set(one, grid::START_Y, 4.0).unwrap();
    p.set(one, grid::END_Y, 4.0).unwrap();
    assert!(close(p.location(column), [5.0, 4.0]));

    // Its offsets from the intersection move it off it, and it still
    // follows.
    p.set(column, column::GRID_OFFSET_X, 0.25).unwrap();
    assert!(close(p.location(column), [5.25, 4.0]));
    p.set(a, grid::START_X, 6.0).unwrap();
    p.set(a, grid::END_X, 6.0).unwrap();
    assert!(close(p.location(column), [6.25, 4.0]));

    // Deleting a grid leaves it where it was.
    p.delete(one);
    assert!(p.store.get(column).is_some());
    assert!(close(p.location(column), [6.25, 4.0]));
    p.set(a, grid::START_X, 7.0).unwrap();
    assert!(close(p.location(column), [6.25, 4.0]));
}

#[test]
fn a_point_away_from_every_intersection_snaps_to_none() {
    let mut p = project();
    p.grid("A", [0.0, 0.0], [0.0, 10.0]);
    p.grid("1", [-1.0, 3.0], [10.0, 3.0]);
    let view = p.store.view();
    assert!(grid::intersection_near(&view, [0.01, 3.0], column::TOLERANCE).is_none());
}

#[test]
fn columns_need_a_type_a_location_and_a_height() {
    let mut p = project();
    let unconnected = |height| Top::Unconnected { height };
    let flat = Placement::new(p.rectangular, p.ground, [0.0, 0.0]).with_top(unconnected(0.0));
    assert!(rule_broken(p.insert(flat), "ColumnHeight"));
    let below = Placement::new(p.rectangular, p.first, [0.0, 0.0]).with_top(Top::Level {
        level: p.ground,
        offset: 0.0,
    });
    assert!(rule_broken(p.insert(below), "ColumnHeight"));

    let not_a_column_type = Placement::new(p.h25, p.ground, [0.0, 0.0]);
    assert!(matches!(
        p.insert(not_a_column_type),
        Err(StoreError::InvalidType { .. })
    ));

    let mut tx = p.store.transaction();
    let bare = Element::new(class(column::COLUMN), p.model).with_property(column::HEIGHT, 3.0);
    let bare = tx.insert(bare).unwrap();
    tx.relate(Relationship::new(
        class(level::ATTACHED_TO_LEVEL),
        bare,
        p.ground,
    ))
    .unwrap();
    let result = tx.commit();
    assert!(rule_broken(result.clone(), "ColumnLocation"));
    assert!(rule_broken(result, "ColumnType"));
}

#[test]
fn a_column_has_at_most_one_top_level() {
    let mut p = project();
    let column = p.column([0.0, 0.0]);
    let mut tx = p.store.transaction();
    let second = Element::new(class(level::LEVEL), p.model)
        .with_property(level::NAME, "Segundo piso")
        .with_property(level::ELEVATION, 6.0);
    let second = tx.insert(second).unwrap();
    tx.relate(Relationship::new(class(column::TOP_LEVEL), column, second))
        .unwrap();
    assert!(rule_broken(tx.commit(), "ColumnConstraints"));
}

/// A kernel for extrusions only: they never reach it.
struct NoKernel;

impl Kernel for NoKernel {
    fn evaluate(&self, _: &Csg) -> Result<Mesh, GeometryError> {
        unreachable!("a column is one extrusion")
    }
}

#[test]
fn a_column_is_its_section_extruded_from_base_to_top() {
    let mut p = project();
    let rectangular = p.column([4.0, 2.0]);
    let placement =
        Placement::new(p.circular, p.ground, [0.0, 0.0]).with_top(Top::Unconnected { height: 2.0 });
    let circular = p.insert(placement).unwrap();
    let mut cache = GeometryCache::new(Arc::new(NoKernel), hito_structural::generators());
    cache.rebuild(&p.store);

    let mesh = cache.mesh(rectangular).expect("a mesh");
    assert!((mesh.volume() - 0.3 * 0.4 * 3.0).abs() < 1e-9);
    let (min, max) = mesh.bounds().unwrap();
    let expected = ([3.85, 1.8, 0.0], [4.15, 2.2, 3.0]);
    for i in 0..3 {
        assert!((min[i] - expected.0[i]).abs() < 1e-9, "{min:?}");
        assert!((max[i] - expected.1[i]).abs() < 1e-9, "{max:?}");
    }
    let round = cache.mesh(circular).expect("a mesh");
    let exact = std::f64::consts::PI * 0.2 * 0.2 * 2.0;
    assert!((round.volume() - exact).abs() < 0.01 * exact);

    // Turned a quarter, the width runs along y.
    let changes = {
        let mut tx = p.store.transaction();
        tx.set_property(rectangular, column::ROTATION, FRAC_PI_2)
            .unwrap();
        tx.commit().unwrap()
    };
    cache.update(&p.store, &changes);
    let (min, max) = cache.mesh(rectangular).unwrap().bounds().unwrap();
    assert!((max[0] - min[0] - 0.4).abs() < 1e-9);
    assert!((max[1] - min[1] - 0.3).abs() < 1e-9);
}
