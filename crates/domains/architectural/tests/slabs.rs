//! Walls attached to the structural domain's slabs (#39), as Revit's
//! "Attach Top/Base".

use hito_architectural::wall::{
    self, BASE_ATTACHED_TO, HEIGHT, Placement, TOP_ATTACHED_TO, TOP_ELEVATION, Top,
};
use hito_architectural::wall_type::{self, Function, Layer};
use hito_core::{Element, ElementStore, Model, ModelKind, Relationship, SchemaRegistry, Value};
use hito_structural::level::{self, ELEVATION, LEVEL};
use hito_structural::slab;

fn real(store: &ElementStore, id: hito_core::ElementId, name: &str) -> f64 {
    match store.property(id, name) {
        Some(Value::Real(x)) => *x,
        other => panic!("{name} is {other:?}"),
    }
}

#[test]
fn walls_stand_on_and_stop_under_slabs_and_follow_them() {
    let mut registry = SchemaRegistry::new();
    hito_architectural::register(&mut registry).unwrap();
    let mut store = ElementStore::new(registry);
    let mut tx = store.transaction();
    let model = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
    let level = |name: &str, elevation: f64| {
        Element::new(hito_structural::class(LEVEL), model)
            .with_property(level::NAME, name)
            .with_property(ELEVATION, elevation)
    };
    let ground = tx.insert(level("Planta baja", 0.0)).unwrap();
    let first = tx.insert(level("Primer piso", 3.0)).unwrap();
    let losa = slab::insert_type(&mut tx, model, "Losa 12", 0.12, None).unwrap();
    let room = [[-1.0, -1.0], [6.0, -1.0], [6.0, 1.0], [-1.0, 1.0]];
    let platea = slab::insert(&mut tx, model, &slab::Placement::new(losa, ground, &room)).unwrap();
    let floor = slab::insert(&mut tx, model, &slab::Placement::new(losa, first, &room)).unwrap();
    let brick = wall_type::insert(
        &mut tx,
        model,
        "Ladrillo 18",
        &[Layer::new(Function::Structure, 0.18)],
    )
    .unwrap();
    let placement = Placement::new(brick, ground, [0.0, 0.0], [5.0, 0.0]).with_top(Top::Level {
        level: first,
        offset: 0.0,
    });
    let wall = wall::insert(&mut tx, model, &placement).unwrap();
    let attach = |how| Relationship::new(hito_architectural::class(how), wall, floor);
    tx.relate(attach(TOP_ATTACHED_TO)).unwrap();
    let on = Relationship::new(hito_architectural::class(BASE_ATTACHED_TO), wall, platea);
    tx.relate(on).unwrap();
    tx.commit().unwrap();

    let view = store.view();
    assert_eq!(wall::elevations(&view, wall), Some((0.0, 2.88)));
    assert!((real(&store, wall, HEIGHT) - 2.88).abs() < 1e-9);

    // A thicker slab type: the wall stops under the new bottom.
    let mut tx = store.transaction();
    tx.set_property(losa, slab::THICKNESS, 0.15).unwrap();
    tx.commit().unwrap();
    assert!((real(&store, wall, TOP_ELEVATION) - 2.85).abs() < 1e-9);
    assert!((real(&store, wall, wall::BOTTOM_ELEVATION) - 0.0).abs() < 1e-9);

    // Moving the upper level moves its slab, and the wall with it.
    let mut tx = store.transaction();
    tx.set_property(first, ELEVATION, 3.2).unwrap();
    tx.commit().unwrap();
    assert!((real(&store, wall, TOP_ELEVATION) - 3.05).abs() < 1e-9);
}
