//! The Manifold kernel behind the geometry port, on the cases from the
//! geometry kernel spike (#2) and through the geometry cache (#28).

use std::sync::Arc;

use hito_core::{
    ClassDef, ClassId, DataType, Dimension, Element, ElementStore, Model, ModelKind, Relationship,
    RelationshipDef, Schema, SchemaRegistry, SchemaVersion,
};
use hito_geometry::{CHORD_TOLERANCE, Frame, Generator, GeometryCache, Mesh, Profile, Solid};
use hito_manifold::Manifold;

/// A box from `min` to `max`.
fn cuboid(min: [f64; 3], max: [f64; 3]) -> Solid {
    let [width, height, depth] = [0, 1, 2].map(|i| max[i] - min[i]);
    Solid::Extrusion {
        profile: Profile::Rectangle { width, height },
        frame: Frame::at([min[0] + width / 2.0, min[1] + height / 2.0, min[2]]),
        depth,
    }
}

fn mesh(solid: &Solid) -> Mesh {
    let mesh = solid.mesh(&Manifold, CHORD_TOLERANCE).unwrap();
    assert!(mesh.is_closed());
    mesh
}

/// A 5 m wall, 20 cm thick and 3 m high, at `origin`, minus an opening
/// that overshoots both faces by 5 cm, as IFC openings do.
fn wall(origin: [f64; 3], x: f64, width: f64, sill: f64, height: f64) -> Solid {
    let [ox, oy, oz] = origin;
    let body = cuboid(origin, [ox + 5.0, oy + 0.2, oz + 3.0]);
    let opening = cuboid(
        [ox + x, oy - 0.05, oz + sill],
        [ox + x + width, oy + 0.25, oz + sill + height],
    );
    Solid::Difference {
        base: Box::new(body),
        voids: vec![opening],
    }
}

#[test]
fn a_wall_minus_a_window() {
    let volume = mesh(&wall([0.0; 3], 1.5, 1.2, 0.9, 1.2)).volume();
    assert!((volume - (5.0 * 0.2 * 3.0 - 1.2 * 1.2 * 0.2)).abs() < 1e-9);
}

#[test]
fn a_door_flush_with_the_wall_base() {
    // The door shares the wall's bottom face: the coplanar case that broke
    // the Rust B-rep kernels in the spike.
    let volume = mesh(&wall([0.0; 3], 2.0, 1.0, 0.0, 2.1)).volume();
    assert!((volume - (5.0 * 0.2 * 3.0 - 1.0 * 2.1 * 0.2)).abs() < 1e-9);
}

#[test]
fn a_wall_far_from_the_origin() {
    let origin = [5_500_000.0, 6_100_000.0, 0.0];
    let volume = mesh(&wall(origin, 1.5, 1.2, 0.9, 1.2)).volume();
    assert!((volume - (5.0 * 0.2 * 3.0 - 1.2 * 1.2 * 0.2)).abs() < 1e-6);
}

#[test]
fn a_plate_with_200_bolt_holes() {
    let (thickness, radius) = (0.02, 0.011);
    let hole = |x: f64, y: f64| Solid::Extrusion {
        profile: Profile::Circle { radius },
        frame: Frame::at([x, y, -thickness]),
        depth: 3.0 * thickness,
    };
    let holes: Vec<_> = (0..20)
        .flat_map(|i| (0..10).map(move |j| (i, j)))
        .map(|(i, j)| hole(0.05 + i as f64 * 0.1, 0.05 + j as f64 * 0.1))
        .collect();
    // A faceted hole's area, from its own mesh.
    let hole_area = mesh(&holes[0]).volume() / (3.0 * thickness);
    let plate = Solid::Difference {
        base: Box::new(cuboid([0.0; 3], [2.0, 1.0, thickness])),
        voids: holes,
    };
    let exact = (2.0 * 1.0 - 200.0 * hole_area) * thickness;
    assert!((mesh(&plate).volume() - exact).abs() < 1e-9);
}

#[test]
fn a_union_counts_the_overlap_once() {
    let a = cuboid([0.0; 3], [2.0, 1.0, 1.0]);
    let b = cuboid([1.0, 0.0, 0.0], [3.0, 1.0, 1.0]);
    let volume = mesh(&Solid::Union(vec![a, b])).volume();
    assert!((volume - 3.0).abs() < 1e-9);
}

#[test]
fn a_solid_cut_away_entirely_is_empty() {
    let small = cuboid([0.0; 3], [1.0; 3]);
    let big = cuboid([-1.0; 3], [2.0; 3]);
    let nothing = Solid::Difference {
        base: Box::new(small),
        voids: vec![big],
    };
    let mesh = nothing.mesh(&Manifold, CHORD_TOLERANCE).unwrap();
    assert!(mesh.is_empty());
}

/// Walls with one opening each, through the geometry cache: the opening is
/// an element related to its wall, and the wall's geometry follows it.
#[test]
fn walls_follow_their_openings() {
    let class = |name: &str| ClassId::new("TestWalls", name);
    let length = DataType::Quantity(Dimension::LENGTH);
    let schema = Schema::new("TestWalls", SchemaVersion::new(1, 0, 0))
        .class(ClassDef::new("Wall", ClassId::GEOMETRIC_ELEMENT_3D).property("X", length))
        .class(ClassDef::new("Opening", ClassId::GEOMETRIC_ELEMENT_3D).property("Width", length))
        .relationship(RelationshipDef::new(
            "Voids",
            class("Opening"),
            class("Wall"),
        ));
    let mut registry = SchemaRegistry::new();
    registry.register(schema).unwrap();
    let mut store = ElementStore::new(registry);

    // A wall with a door.
    let generator = Generator::new(class("Wall"), |inputs, element| {
        let x = inputs.real(element.id(), "X")?;
        let opening = inputs.relationships_to(element.id()).next()?.source();
        let width = inputs.real(opening, "Width")?;
        Some(wall([x, 0.0, 0.0], 1.0, width, 0.0, 2.1))
    });
    let mut cache = GeometryCache::new(Arc::new(Manifold), [generator]);

    let mut tx = store.transaction();
    let model = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
    let mut walls = Vec::new();
    let mut openings = Vec::new();
    for i in 0..200 {
        let wall = Element::new(class("Wall"), model).with_property("X", i as f64 * 6.0);
        let wall = tx.insert(wall).unwrap();
        let opening = Element::new(class("Opening"), model).with_property("Width", 1.0);
        let opening = tx.insert(opening).unwrap();
        tx.relate(Relationship::new(class("Voids"), opening, wall))
            .unwrap();
        walls.push(wall);
        openings.push(opening);
    }
    let changes = tx.commit().unwrap();
    let done = cache.update(&store, &changes);
    assert_eq!(done.updated.len(), 200);
    let door = |width: f64| 5.0 * 0.2 * 3.0 - width * 2.1 * 0.2;
    assert!((cache.mesh(walls[7]).unwrap().volume() - door(1.0)).abs() < 1e-9);

    let mut tx = store.transaction();
    tx.set_property(openings[7], "Width", 1.5).unwrap();
    let changes = tx.commit().unwrap();
    let done = cache.update(&store, &changes);
    assert_eq!(done.updated, [walls[7]]);
    assert!((cache.mesh(walls[7]).unwrap().volume() - door(1.5)).abs() < 1e-9);
    assert!((cache.mesh(walls[8]).unwrap().volume() - door(1.0)).abs() < 1e-9);
}
