//! Doors and windows (#194) through the geometry cache and Manifold: the
//! wall loses exactly the volume of its openings, the doors and windows
//! mesh closed, and the cut follows them when they move or their type
//! changes.

use std::sync::Arc;

use hito_architectural::opening::{self, GLASS_THICKNESS, LEAF_THICKNESS, Placement};
use hito_architectural::opening_type::{self, Definition};
use hito_architectural::wall::{self, Placement as WallPlacement};
use hito_architectural::wall_type::{self, Function, Layer};
use hito_core::{Element, ElementId, ElementStore, Model, ModelKind, SchemaRegistry};
use hito_geometry::GeometryCache;
use hito_manifold::Manifold;
use hito_structural::level;

struct Fixture {
    store: ElementStore,
    cache: GeometryCache,
    wall: ElementId,
    door_type: ElementId,
    door: ElementId,
    window: ElementId,
}

/// A 5 m concrete wall, 20 cm thick and 3 m high, far from the origin,
/// with an 80 × 205 door at 1 m and a 150 × 110 window at 3.5 m, sill
/// 90 cm.
fn fixture() -> Fixture {
    let mut registry = SchemaRegistry::new();
    hito_architectural::register(&mut registry).unwrap();
    let mut store = ElementStore::new(registry);
    let mut tx = store.transaction();
    let model = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
    let ground = Element::new(hito_structural::class(level::LEVEL), model)
        .with_property(level::NAME, "Planta baja")
        .with_property(level::ELEVATION, 0.0);
    let ground = tx.insert(ground).unwrap();
    let concrete = [Layer::new(Function::Structure, 0.2)];
    let concrete = wall_type::insert(&mut tx, model, "Hormigón 20", &concrete).unwrap();
    let o = [5_500_000.0, 6_100_000.0];
    let start = [o[0], o[1]];
    let end = [o[0] + 3.0, o[1] + 4.0];
    let placement = WallPlacement::new(concrete, ground, start, end);
    let wall = wall::insert(&mut tx, model, &placement).unwrap();
    let door = Definition::door("Puerta 80 × 205", 0.8, 2.05);
    let door_type = opening_type::insert(&mut tx, model, &door).unwrap();
    let window = Definition::window("Ventana 150 × 110", 1.5, 1.1, 0.9);
    let window_type = opening_type::insert(&mut tx, model, &window).unwrap();
    let door = opening::insert(&mut tx, model, &Placement::new(door_type, wall, 1.0)).unwrap();
    let window = opening::insert(&mut tx, model, &Placement::new(window_type, wall, 3.5)).unwrap();
    tx.commit().unwrap();
    let mut cache = GeometryCache::new(Arc::new(Manifold), hito_architectural::generators());
    cache.rebuild(&store);
    Fixture {
        store,
        cache,
        wall,
        door_type,
        door,
        window,
    }
}

fn volume(f: &Fixture, id: ElementId) -> f64 {
    let mesh = f.cache.mesh(id).expect("it has geometry");
    assert!(mesh.is_closed());
    mesh.volume()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn a_wall_loses_the_volume_of_its_openings() {
    let f = fixture();
    let expected = 5.0 * 0.2 * 3.0 - 0.8 * 2.05 * 0.2 - 1.5 * 1.1 * 0.2;
    let got = volume(&f, f.wall);
    assert!(close(got, expected), "{got} vs {expected}");
}

#[test]
fn doors_and_windows_are_their_frame_and_leaf_or_glass() {
    let f = fixture();
    let (fw, fd, gap) = (0.05, 0.1, 0.002);

    let (w, h) = (0.8, 2.05);
    let frame = (w * h - (w - 2.0 * fw) * (h - fw)) * fd;
    let leaf = (w - 2.0 * fw - 2.0 * gap) * (h - fw - 2.0 * gap) * LEAF_THICKNESS;
    let got = volume(&f, f.door);
    assert!(close(got, frame + leaf), "{got} vs {}", frame + leaf);

    let (w, h) = (1.5, 1.1);
    let frame = (w * h - (w - 2.0 * fw) * (h - 2.0 * fw)) * fd;
    let glass = (w - 2.0 * fw - 2.0 * gap) * (h - 2.0 * fw - 2.0 * gap) * GLASS_THICKNESS;
    let got = volume(&f, f.window);
    assert!(close(got, frame + glass), "{got} vs {}", frame + glass);

    // The door stands in the wall, from the floor to 2.05 m.
    let (lo, hi) = f.cache.mesh(f.door).unwrap().bounds().unwrap();
    assert!(close(lo[2], 0.0) && close(hi[2], 2.05));
}

#[test]
fn the_cut_follows_the_door_and_its_type() {
    let mut f = fixture();
    let before = volume(&f, f.wall);

    // Moving the door regenerates the wall, whose volume stays the same.
    let mut tx = f.store.transaction();
    opening::move_to(&mut tx, f.door, 2.0).unwrap();
    let changes = tx.commit().unwrap();
    let done = f.cache.update(&f.store, &changes);
    assert!(done.updated.contains(&f.wall) && done.updated.contains(&f.door));
    assert!(close(volume(&f, f.wall), before));
    let (lo, _) = f.cache.mesh(f.door).unwrap().bounds().unwrap();
    // 2 m along a 3-4-5 wall, less half the door's 80 cm, and half its
    // 10 cm frame across the wall.
    assert!(close(lo[0] - 5_500_000.0, 1.6 * 0.6 - 0.05 * 0.8));

    // A wider type cuts more.
    let mut tx = f.store.transaction();
    tx.set_property(f.door_type, opening_type::WIDTH, 0.9)
        .unwrap();
    let changes = tx.commit().unwrap();
    let done = f.cache.update(&f.store, &changes);
    assert!(done.updated.contains(&f.wall));
    assert!(close(volume(&f, f.wall), before - 0.1 * 2.05 * 0.2));

    // Deleting the door fills the wall again.
    let mut tx = f.store.transaction();
    tx.delete(f.door).unwrap();
    let changes = tx.commit().unwrap();
    let done = f.cache.update(&f.store, &changes);
    assert!(done.removed.contains(&f.door));
    let expected = 5.0 * 0.2 * 3.0 - 1.5 * 1.1 * 0.2;
    assert!(close(volume(&f, f.wall), expected));
}
