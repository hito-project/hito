//! Walls and wall types (#193), through the public API only. The drawing
//! tool comes later; this is what it builds on.

mod common;

use common::{Project, broken_rules, close, project, slab_class, total, unconnected};
use hito_architectural::wall::{
    self, BASE_ATTACHED_TO, BOTTOM_ELEVATION, END_X, HEIGHT, LocationLine, TOP_ATTACHED_TO,
    TOP_ELEVATION, TOP_LEVEL, TOP_OFFSET, Top,
};
use hito_architectural::wall_type::{self, Function, Layer};
use hito_architectural::{class, footprint};
use hito_core::{Element, ElementId, Relationship, StoreError, Value};
use hito_structural::level::{self, BOTTOM_ELEVATION as SLAB_BOTTOM, TOP_ELEVATION as SLAB_TOP};

#[test]
fn a_wall_type_is_its_layers_from_the_exterior_in() {
    let p = project();
    let view = p.store.view();
    let layers = wall_type::layers(&view, p.brick);
    let functions: Vec<_> = layers.iter().map(|l| l.function).collect();
    assert_eq!(
        functions,
        [Function::Finish1, Function::Structure, Function::Finish2]
    );
    let materials: Vec<_> = layers.iter().map(|l| l.material).collect();
    assert_eq!(
        materials,
        [Some(p.render), Some(p.hollow_brick), Some(p.plaster)]
    );
    assert!(close(wall_type::width(&view, p.brick), 0.22));
    assert_eq!(wall_type::core(&layers), 1..2);

    // Without a structural layer, it's all core.
    let concrete = wall_type::layers(&view, p.concrete);
    assert_eq!(wall_type::core(&concrete), 0..1);
}

#[test]
fn wall_type_and_layer_rules() {
    let mut p = project();
    let mut tx = p.store.transaction();
    let model = p.model;
    let thin = [Layer::new(Function::Structure, 0.1)];
    wall_type::insert(&mut tx, model, "Hormigón 20", &thin).unwrap();
    assert_eq!(broken_rules(tx.commit()), ["WallTypeName"]);

    let mut tx = p.store.transaction();
    let negative = [Layer::new(Function::Structure, -0.1)];
    wall_type::insert(&mut tx, model, "Negativo", &negative).unwrap();
    assert_eq!(broken_rules(tx.commit()), ["WallLayer"]);

    // Two layers at the same position.
    let layer = wall_type::layers(&p.store.view(), p.brick)[0].id;
    let mut tx = p.store.transaction();
    tx.set_property(layer, wall_type::POSITION, 2).unwrap();
    assert_eq!(broken_rules(tx.commit()), ["WallLayer"]);

    assert_eq!(
        broken_rules(p.set(layer, wall_type::FUNCTION, "Glass")),
        ["WallLayer"]
    );
}

#[test]
fn deleting_a_wall_type_deletes_its_layers_and_walls() {
    let mut p = project();
    let wall = p.wall(p.concrete, [0.0, 0.0], [4.0, 0.0]);
    let layer = wall_type::layers(&p.store.view(), p.concrete)[0].id;
    let mut tx = p.store.transaction();
    let mut deleted = tx.delete(p.concrete).unwrap();
    deleted.sort();
    let mut expected = vec![p.concrete, layer, wall];
    expected.sort();
    assert_eq!(deleted, expected);
    tx.commit().unwrap();
}

#[test]
fn an_unconnected_wall_stands_on_its_level_plus_offset() {
    let mut p = project();
    let placement = p
        .placement(p.brick, [0.0, 0.0], [5.0, 0.0])
        .with_base_offset(-0.5)
        .with_top(unconnected(2.6));
    let wall = p.place(&placement).unwrap();
    assert_eq!(p.real(wall, BOTTOM_ELEVATION), -0.5);
    assert!(close(p.real(wall, TOP_ELEVATION), 2.1));
    assert_eq!(p.real(wall, HEIGHT), 2.6);

    p.set(wall, HEIGHT, 3.0).unwrap();
    assert!(close(p.real(wall, TOP_ELEVATION), 2.5));
}

#[test]
fn a_wall_up_to_a_level_follows_both_levels() {
    let mut p = project();
    let placement = p
        .placement(p.brick, [0.0, 0.0], [5.0, 0.0])
        .with_top(Top::Level {
            level: p.first,
            offset: -0.1,
        });
    let wall = p.place(&placement).unwrap();
    assert!(close(p.real(wall, HEIGHT), 2.9));

    // Raising the top level stretches the wall; raising the base level
    // shortens it. Each is one transaction.
    p.set(p.first, level::ELEVATION, 3.2).unwrap();
    assert!(close(p.real(wall, TOP_ELEVATION), 3.1));
    assert!(close(p.real(wall, HEIGHT), 3.1));
    p.set(p.ground, level::ELEVATION, 0.4).unwrap();
    assert!(close(p.real(wall, BOTTOM_ELEVATION), 0.4));
    assert!(close(p.real(wall, HEIGHT), 2.7));

    // The height is derived while the top is on a level.
    p.set(wall, HEIGHT, 10.0).unwrap();
    assert!(close(p.real(wall, HEIGHT), 2.7));
    p.set(wall, TOP_OFFSET, 0.0).unwrap();
    assert!(close(p.real(wall, HEIGHT), 2.8));
}

#[test]
fn deleting_the_top_level_leaves_the_wall_unconnected_at_its_height() {
    let mut p = project();
    let placement = p
        .placement(p.brick, [0.0, 0.0], [5.0, 0.0])
        .with_top(Top::Level {
            level: p.first,
            offset: 0.0,
        });
    let wall = p.place(&placement).unwrap();
    let mut tx = p.store.transaction();
    tx.delete(p.first).unwrap();
    tx.commit().unwrap();
    assert!(p.store.get(wall).is_some());
    assert_eq!(p.real(wall, HEIGHT), 3.0);
    assert_eq!(p.real(wall, TOP_ELEVATION), 3.0);
}

#[test]
fn deleting_the_base_level_deletes_the_wall() {
    let mut p = project();
    let wall = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    let mut tx = p.store.transaction();
    tx.delete(p.ground).unwrap();
    tx.commit().unwrap();
    assert!(p.store.get(wall).is_none());
}

/// A slab between `bottom` and `top`, on the ground floor.
fn slab(p: &mut Project, bottom: f64, top: f64) -> ElementId {
    let slab = Element::new(slab_class(), p.model)
        .with_property(SLAB_BOTTOM, bottom)
        .with_property(SLAB_TOP, top);
    let mut tx = p.store.transaction();
    let id = tx.insert(slab).unwrap();
    let attached = hito_structural::class(level::ATTACHED_TO_LEVEL);
    tx.relate(Relationship::new(attached, id, p.ground))
        .unwrap();
    tx.commit().unwrap();
    id
}

fn attach(p: &mut Project, wall: ElementId, how: &str, slab: ElementId) {
    let mut tx = p.store.transaction();
    tx.relate(Relationship::new(class(how), wall, slab))
        .unwrap();
    tx.commit().unwrap();
}

#[test]
fn an_attached_top_and_base_follow_their_slabs() {
    let mut p = project();
    let placement = p
        .placement(p.brick, [0.0, 0.0], [5.0, 0.0])
        .with_top(Top::Level {
            level: p.first,
            offset: 0.0,
        });
    let wall = p.place(&placement).unwrap();
    let above = slab(&mut p, 2.8, 3.0);
    let below = slab(&mut p, -0.1, 0.1);

    attach(&mut p, wall, TOP_ATTACHED_TO, above);
    assert!(close(p.real(wall, TOP_ELEVATION), 2.8));
    attach(&mut p, wall, BASE_ATTACHED_TO, below);
    assert!(close(p.real(wall, BOTTOM_ELEVATION), 0.1));
    assert!(close(p.real(wall, HEIGHT), 2.7));

    // The slab gets thicker: the wall follows in the same transaction.
    p.set(above, SLAB_BOTTOM, 2.75).unwrap();
    assert!(close(p.real(wall, TOP_ELEVATION), 2.75));
    assert!(close(p.real(wall, HEIGHT), 2.65));

    // Deleting the slab above leaves the top at its level again.
    let mut tx = p.store.transaction();
    tx.delete(above).unwrap();
    tx.commit().unwrap();
    assert!(close(p.real(wall, TOP_ELEVATION), 3.0));
}

#[test]
fn a_wall_top_must_be_above_its_base() {
    let mut p = project();
    let low = p
        .placement(p.brick, [0.0, 0.0], [5.0, 0.0])
        .with_top(Top::Level {
            level: p.first,
            offset: 0.0,
        });
    let wall = p.place(&low).unwrap();
    // Moving the top level under the base is refused, as in Revit.
    assert_eq!(
        broken_rules(p.set(p.first, level::ELEVATION, -1.0)),
        ["WallHeight"]
    );
    assert_eq!(broken_rules(p.set(wall, TOP_OFFSET, -3.0)), ["WallHeight"]);
    let flat = p
        .placement(p.brick, [0.0, 0.0], [5.0, 0.0])
        .with_top(unconnected(0.0));
    assert_eq!(broken_rules(p.place(&flat)), ["WallHeight"]);
}

#[test]
fn a_wall_needs_a_line_a_type_and_known_constraints() {
    let mut p = project();
    let short = p.placement(p.brick, [1.0, 1.0], [1.0, 1.0005]);
    assert_eq!(broken_rules(p.place(&short)), ["WallLine"]);

    let wall = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    assert_eq!(
        broken_rules(p.set(wall, wall::LOCATION_LINE, "Middle")),
        ["WallConstraints"]
    );
    let mut tx = p.store.transaction();
    tx.relate(Relationship::new(class(TOP_LEVEL), wall, p.first))
        .unwrap();
    tx.relate(Relationship::new(class(TOP_LEVEL), wall, p.ground))
        .unwrap();
    assert_eq!(broken_rules(tx.commit()), ["WallConstraints"]);

    let mut tx = p.store.transaction();
    let untyped = Element::new(class(wall::WALL), p.model)
        .with_property(wall::START_X, 0.0)
        .with_property(wall::START_Y, 0.0)
        .with_property(END_X, 1.0)
        .with_property(wall::END_Y, 0.0)
        .with_property(HEIGHT, 3.0);
    let id = tx.insert(untyped).unwrap();
    let attached = hito_structural::class(level::ATTACHED_TO_LEVEL);
    tx.relate(Relationship::new(attached, id, p.ground))
        .unwrap();
    assert_eq!(broken_rules(tx.commit()), ["WallType"]);
}

/// The y extent of a wall along x from its single slice.
fn faces(p: &Project, wall: ElementId) -> (f64, f64) {
    let pieces = p.pieces(&[wall]);
    let ys = pieces.iter().flatten().map(|q| q[1]);
    let low = ys.clone().fold(f64::INFINITY, f64::min);
    let high = ys.fold(f64::NEG_INFINITY, f64::max);
    (low, high)
}

#[test]
fn layers_lie_across_the_location_line_as_it_says() {
    let mut p = project();
    let cases = [
        (LocationLine::WallCenterline, (-0.11, 0.11)),
        (LocationLine::CoreCenterline, (-0.11, 0.11)),
        (LocationLine::FinishFaceExterior, (-0.22, 0.0)),
        (LocationLine::FinishFaceInterior, (0.0, 0.22)),
        (LocationLine::CoreFaceExterior, (-0.2, 0.02)),
        (LocationLine::CoreFaceInterior, (-0.02, 0.2)),
    ];
    for (i, (location, (low, high))) in cases.into_iter().enumerate() {
        // Far apart, so they don't join.
        let y = 10.0 * i as f64;
        let placement = p
            .placement(p.brick, [0.0, y], [5.0, y])
            .with_location_line(location);
        let wall = p.place(&placement).unwrap();
        let (l, h) = faces(&p, wall);
        assert!(
            close(l - y, low) && close(h - y, high),
            "{location:?}: {l}, {h}"
        );
    }
}

#[test]
fn the_exterior_is_on_the_left_until_flipped() {
    let mut p = project();
    // Drawn along +x, the exterior render is on the +y side.
    let wall = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    let render = &p.layer(wall, 0)[0];
    assert!(render.iter().all(|q| q[1] >= 0.09 - 1e-12));

    let mut tx = p.store.transaction();
    wall::flip(&mut tx, wall).unwrap();
    tx.commit().unwrap();
    let render = &p.layer(wall, 0)[0];
    assert!(render.iter().all(|q| q[1] <= -0.09 + 1e-12));
    assert_eq!(
        p.store.property(wall, wall::FLIPPED),
        Some(&Value::Boolean(true))
    );
}

#[test]
fn a_structural_wall_carries_its_core_material() {
    let mut p = project();
    let placement = p
        .placement(p.concrete, [0.0, 0.0], [5.0, 0.0])
        .structural(true);
    let wall = p.place(&placement).unwrap();
    assert_eq!(
        p.store.property(wall, wall::STRUCTURAL),
        Some(&Value::Boolean(true))
    );
    assert_eq!(
        wall::structural_material(&p.store.view(), wall),
        Some(p.h25)
    );
    let brick = p.wall(p.brick, [0.0, 5.0], [5.0, 5.0]);
    assert_eq!(
        wall::structural_material(&p.store.view(), brick),
        Some(p.hollow_brick)
    );
}

/// The warnings the commit of a structural wall of `wall_type`, along `y`,
/// gives.
fn structural_warnings(p: &mut Project, wall_type: ElementId, y: f64) -> Vec<String> {
    let placement = p.placement(wall_type, [0.0, y], [5.0, y]).structural(true);
    let mut tx = p.store.transaction();
    wall::insert(&mut tx, p.model, &placement).unwrap();
    let changes = tx.commit().expect("warnings don't refuse the wall");
    changes.warnings().iter().map(|w| w.rule.clone()).collect()
}

#[test]
fn a_structural_wall_is_expected_to_be_concrete() {
    let mut p = project();
    let (concrete, brick) = (p.concrete, p.brick);
    assert!(structural_warnings(&mut p, concrete, 0.0).is_empty());
    // Hollow brick isn't concrete.
    assert_eq!(
        structural_warnings(&mut p, brick, 3.0),
        ["StructuralWallConcrete"]
    );
    // Nor is a layer without a material.
    let mut tx = p.store.transaction();
    let bare = wall_type::insert(
        &mut tx,
        p.model,
        "Sin material",
        &[Layer::new(Function::Structure, 0.2)],
    )
    .unwrap();
    tx.commit().unwrap();
    assert_eq!(
        structural_warnings(&mut p, bare, 6.0),
        ["StructuralWallConcrete"]
    );

    // A wall that isn't structural can be of anything.
    let placement = p.placement(brick, [0.0, 9.0], [5.0, 9.0]);
    let mut tx = p.store.transaction();
    wall::insert(&mut tx, p.model, &placement).unwrap();
    assert!(tx.commit().unwrap().warnings().is_empty());
}

#[test]
fn layer_materials_are_materials() {
    let mut p = project();
    let layer = wall_type::layers(&p.store.view(), p.concrete)[0].id;
    let mut tx = p.store.transaction();
    // A wall type isn't a material.
    let not_a_material = Relationship::new(class(wall_type::LAYER_MATERIAL), layer, p.brick);
    assert!(matches!(
        tx.relate(not_a_material),
        Err(StoreError::InvalidRelationship(_))
    ));
}

#[test]
fn a_lone_wall_is_a_box_of_its_layers() {
    let mut p = project();
    let wall = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    let footprint = p.footprint(wall);
    assert_eq!(footprint.slices.len(), 1);
    let slice = &footprint.slices[0];
    assert_eq!((slice.bottom, slice.top), (0.0, 3.0));
    let areas: Vec<f64> = slice.layers.iter().map(|l| total(&l.polygons)).collect();
    for (area, thickness) in areas.iter().zip([0.02, 0.18, 0.02]) {
        assert!(close(*area, 5.0 * thickness), "{area}");
    }
    assert!(footprint::footprint(&p.store.view(), p.ground).is_none());
}
