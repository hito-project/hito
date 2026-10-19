//! Doors and windows (#194), through the public API only: their types,
//! hosting, the opening each cuts, the flips and the plan symbol. The
//! placement tool comes later; this is what it builds on.

mod common;

use std::f64::consts::FRAC_PI_2;

use common::{Project, broken_rules, close, project};
use hito_architectural::opening::{self, Placement};
use hito_architectural::opening_type::{self, Definition, Kind};
use hito_architectural::{class, wall};
use hito_core::{Element, ElementId, StoreError, Value};

/// A door type, 80 × 205 cm, and a window type, 150 × 110 cm at 90 cm.
fn types(p: &mut Project) -> (ElementId, ElementId) {
    let mut tx = p.store.transaction();
    let door = Definition::door("Puerta 80 × 205", 0.8, 2.05);
    let window = Definition::window("Ventana 150 × 110", 1.5, 1.1, 0.9);
    let door = opening_type::insert(&mut tx, p.model, &door).unwrap();
    let window = opening_type::insert(&mut tx, p.model, &window).unwrap();
    tx.commit().unwrap();
    (door, window)
}

fn place(p: &mut Project, placement: &Placement) -> Result<ElementId, StoreError> {
    let mut tx = p.store.transaction();
    let id = opening::insert(&mut tx, p.model, placement)?;
    tx.commit()?;
    Ok(id)
}

fn close_point(a: [f64; 2], b: [f64; 2]) -> bool {
    close(a[0], b[0]) && close(a[1], b[1])
}

/// The rules a committed change set warns about.
fn warnings(p: &mut Project, change: impl FnOnce(&mut hito_core::Transaction<'_>)) -> Vec<String> {
    let mut tx = p.store.transaction();
    change(&mut tx);
    let changes = tx.commit().expect("warnings don't block the commit");
    changes.warnings().iter().map(|v| v.rule.clone()).collect()
}

#[test]
fn types_have_fixed_sizes() {
    let mut p = project();
    let (door, window) = types(&mut p);
    let view = p.store.view();
    assert_eq!(opening_type::kind(&view, door), Some(Kind::Door));
    assert_eq!(opening_type::kind(&view, window), Some(Kind::Window));
    let size = opening_type::size(&view, window).unwrap();
    assert_eq!((size.width, size.height, size.sill_height), (1.5, 1.1, 0.9));
    assert_eq!((size.frame_width, size.frame_depth), (0.05, 0.1));
    assert_eq!(opening_type::types(&view, Kind::Door), [door]);
    assert_eq!(opening_type::size(&view, p.brick), None, "not a door type");
}

#[test]
fn type_rules() {
    let mut p = project();
    types(&mut p);
    let insert = |p: &mut Project, d: Definition| {
        let mut tx = p.store.transaction();
        opening_type::insert(&mut tx, p.model, &d).unwrap();
        tx.commit().map(drop)
    };
    // Names are unique among doors, but a window may share a door's.
    let same = Definition::door("Puerta 80 × 205", 0.9, 2.05);
    assert_eq!(broken_rules(insert(&mut p, same)), ["OpeningTypeName"]);
    let window = Definition::window("Puerta 80 × 205", 0.9, 2.05, 0.0);
    insert(&mut p, window).unwrap();
    assert_eq!(
        broken_rules(insert(&mut p, Definition::door("", 0.8, 2.0))),
        ["OpeningTypeName"]
    );

    for bad in [
        Definition::door("Sin ancho", 0.0, 2.0),
        Definition::door("Alto negativo", 0.8, -2.0),
        Definition::window("Antepecho negativo", 1.0, 1.0, -0.1),
        Definition::door("Marco sin profundidad", 0.8, 2.0).with_frame(0.05, 0.0),
        Definition::door("Marco enorme", 0.8, 2.0).with_frame(0.4, 0.1),
    ] {
        let name = bad.name.clone();
        assert_eq!(
            broken_rules(insert(&mut p, bad)),
            ["OpeningTypeSize"],
            "{name}"
        );
    }
    // No frame is fine.
    insert(
        &mut p,
        Definition::door("Vano", 0.8, 2.0).with_frame(0.0, 0.1),
    )
    .unwrap();
}

#[test]
fn a_door_cuts_its_size_out_of_its_wall() {
    let mut p = project();
    let (door, _) = types(&mut p);
    let wall = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    let d = place(&mut p, &Placement::new(door, wall, 1.0)).unwrap();
    let view = p.store.view();
    assert_eq!(opening::host(&view, d), Some(wall));
    assert_eq!(opening::hosted(&view, wall), [d]);
    assert!(view.get(d).unwrap().class() == &class(opening::DOOR));

    let cut = opening::cut(&view, d).unwrap();
    assert!(close(cut.from, 0.6) && close(cut.to, 1.4));
    assert!(close(cut.bottom, 0.0) && close(cut.top, 2.05));
    // In the middle of the wall's thickness, which is centred on the line.
    let o = opening::opening(&view, d).unwrap();
    assert!(close_point(o.centre, [1.0, 0.0]));
    assert!(close(o.wall_width, 0.22));
}

#[test]
fn a_window_stands_on_its_sill_above_the_walls_level() {
    let mut p = project();
    let (_, window) = types(&mut p);
    let placement = p
        .placement(p.concrete, [0.0, 0.0], [0.0, 6.0])
        .with_base_offset(0.5);
    let ground_wall = p.place(&placement).unwrap();
    let w = place(&mut p, &Placement::new(window, ground_wall, 3.0)).unwrap();
    let cut = opening::cut(&p.store.view(), w).unwrap();
    // From the level, not from the wall's offset base, as in Revit.
    assert!(close(cut.bottom, 0.9) && close(cut.top, 2.0));

    // Its own sill height overrides the type's.
    p.set(w, opening::SILL_HEIGHT, 1.2).unwrap();
    let cut = opening::cut(&p.store.view(), w).unwrap();
    assert!(close(cut.bottom, 1.2) && close(cut.top, 2.3));

    // On the first floor, it's 3 m higher.
    let upper = wall::Placement::new(p.concrete, p.first, [10.0, 0.0], [10.0, 6.0]);
    let upper = p.place(&upper).unwrap();
    let w = place(&mut p, &Placement::new(window, upper, 3.0)).unwrap();
    let cut = opening::cut(&p.store.view(), w).unwrap();
    assert!(close(cut.bottom, 3.9) && close(cut.top, 5.0));
}

#[test]
fn doors_move_with_their_wall_and_follow_their_type() {
    let mut p = project();
    let (door, _) = types(&mut p);
    let wall = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    let d = place(&mut p, &Placement::new(door, wall, 2.0)).unwrap();

    // Moving and turning the wall takes the door with it.
    let mut tx = p.store.transaction();
    for (name, value) in [
        (wall::START_X, 10.0),
        (wall::START_Y, 10.0),
        (wall::END_X, 10.0),
        (wall::END_Y, 15.0),
    ] {
        tx.set_property(wall, name, value).unwrap();
    }
    tx.commit().unwrap();
    let o = opening::opening(&p.store.view(), d).unwrap();
    assert!(close_point(o.centre, [10.0, 12.0]));
    assert!(close_point(o.along, [0.0, 1.0]));

    // Moving the door along the wall.
    let mut tx = p.store.transaction();
    opening::move_to(&mut tx, d, 3.5).unwrap();
    tx.commit().unwrap();
    let cut = opening::cut(&p.store.view(), d).unwrap();
    assert!(close(cut.from, 3.1) && close(cut.to, 3.9));

    // Changing the type changes every door of it.
    p.set(door, opening_type::WIDTH, 0.9).unwrap();
    let cut = opening::cut(&p.store.view(), d).unwrap();
    assert!(close(cut.from, 3.05) && close(cut.to, 3.95));
}

#[test]
fn deleting_a_wall_deletes_its_doors_and_windows() {
    let mut p = project();
    let (door, window) = types(&mut p);
    let wall = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    let d = place(&mut p, &Placement::new(door, wall, 1.0)).unwrap();
    let w = place(&mut p, &Placement::new(window, wall, 3.5)).unwrap();
    let preview = p.store.view().preview_delete(wall).unwrap();
    assert!(preview.contains(&d) && preview.contains(&w));
    let mut tx = p.store.transaction();
    tx.delete(wall).unwrap();
    tx.commit().unwrap();
    assert!(p.store.get(d).is_none() && p.store.get(w).is_none());
    // The types stay.
    assert!(p.store.get(door).is_some());
}

#[test]
fn a_door_needs_one_wall_a_type_and_a_position() {
    let mut p = project();
    let (door, _) = types(&mut p);
    let mut tx = p.store.transaction();
    let loose = Element::new(class(opening::DOOR), p.model)
        .with_type(door)
        .with_property(opening::DISTANCE_ALONG, 1.0);
    tx.insert(loose).unwrap();
    assert_eq!(broken_rules(tx.commit()), ["OpeningHost"]);

    // A door type is refused for a window, and a wall type for both.
    let wall = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    let mut tx = p.store.transaction();
    let window = Element::new(class(opening::WINDOW), p.model).with_type(door);
    assert!(matches!(
        tx.insert(window),
        Err(StoreError::InvalidType { .. })
    ));
    let wrong = opening::insert(&mut tx, p.model, &Placement::new(p.brick, wall, 1.0));
    assert!(matches!(wrong, Err(StoreError::InvalidType { .. })));
}

#[test]
fn a_door_that_wouldnt_fit_its_wall_is_refused() {
    let mut p = project();
    let (door, window) = types(&mut p);
    let wall = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    let d = place(&mut p, &Placement::new(door, wall, 4.0)).unwrap();

    // Shortening the wall past the door is refused, as in Revit, and
    // leaves the wall and the door as they were.
    assert_eq!(
        broken_rules(p.set(wall, wall::END_X, 4.2)),
        ["OpeningInWall"]
    );
    assert!(close(p.real(wall, wall::END_X), 5.0));
    assert!(p.store.get(d).is_some());
    // Shortening it up to the door's edge is fine.
    p.set(wall, wall::END_X, 4.4).unwrap();

    // So is a door placed past the end, or a window above the wall's top.
    let past = Placement::new(door, wall, 4.3);
    assert_eq!(broken_rules(place(&mut p, &past)), ["OpeningInWall"]);
    let high = Placement::new(window, wall, 1.5).with_sill_height(2.5);
    assert_eq!(broken_rules(place(&mut p, &high)), ["OpeningInWall"]);
    // Raising the door's sill until it pokes out of the top, too.
    assert_eq!(
        broken_rules(p.set(d, opening::SILL_HEIGHT, 1.0)),
        ["OpeningInWall"]
    );
}

#[test]
fn overlapping_doors_are_a_warning() {
    let mut p = project();
    let (door, _) = types(&mut p);
    let wall = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    place(&mut p, &Placement::new(door, wall, 1.0)).unwrap();
    let model = p.model;
    let rules = warnings(&mut p, |tx| {
        opening::insert(tx, model, &Placement::new(door, wall, 1.5)).unwrap();
    });
    assert_eq!(rules, ["OpeningOverlap"]);
    // Side by side is fine.
    let rules = warnings(&mut p, |tx| {
        opening::insert(tx, model, &Placement::new(door, wall, 3.0)).unwrap();
    });
    assert!(rules.is_empty(), "{rules:?}");
}

#[test]
fn flips_swap_the_swing_and_the_hinges() {
    let mut p = project();
    let (door, _) = types(&mut p);
    // Along x: the exterior face is on the left, +y.
    let wall = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    let d = place(&mut p, &Placement::new(door, wall, 2.0)).unwrap();

    let symbol = opening::plan_symbol(&p.store.view(), d).unwrap();
    let clear = 0.8 - 2.0 * 0.05;
    let [arc] = symbol.arcs[..] else {
        panic!("one swing")
    };
    // Hinges on the start's side, at the frame's interior edge, swinging
    // into the room (-y) and closing towards +x.
    assert!(close_point(arc.centre, [2.0 - 0.35, -0.05]));
    assert!(close(arc.radius, clear));
    assert!(close(arc.start, -FRAC_PI_2) && close(arc.sweep, FRAC_PI_2));
    let [[hinge, open]] = symbol.lines[..] else {
        panic!("one leaf")
    };
    assert!(close_point(hinge, arc.centre));
    assert!(close_point(open, [1.65, -0.05 - clear]));
    assert_eq!(symbol.outlines.len(), 2, "two jambs");
    assert_eq!(symbol.cut.len(), 4);

    // Flipping the facing swings it out; flipping the hand moves the
    // hinges to the end's side.
    let mut tx = p.store.transaction();
    opening::flip_facing(&mut tx, d).unwrap();
    opening::flip_hand(&mut tx, d).unwrap();
    tx.commit().unwrap();
    let symbol = opening::plan_symbol(&p.store.view(), d).unwrap();
    let arc = symbol.arcs[0];
    assert!(close_point(arc.centre, [2.35, 0.05]));
    assert!(close(arc.start, FRAC_PI_2) && close(arc.sweep, FRAC_PI_2));
    assert_eq!(
        p.store.property(d, opening::FACING_FLIPPED),
        Some(&Value::Boolean(true))
    );

    // Flipping the wall swaps its exterior side, and the door's facing
    // follows it.
    let mut tx = p.store.transaction();
    wall::flip(&mut tx, wall).unwrap();
    opening::flip_facing(&mut tx, d).unwrap();
    tx.commit().unwrap();
    let o = opening::opening(&p.store.view(), d).unwrap();
    assert!(close_point(o.facing, [0.0, -1.0]));
}

#[test]
fn a_window_is_drawn_with_the_wall_faces_and_its_glass() {
    let mut p = project();
    let (_, window) = types(&mut p);
    let wall = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    let w = place(&mut p, &Placement::new(window, wall, 2.5)).unwrap();
    let symbol = opening::plan_symbol(&p.store.view(), w).unwrap();
    assert!(symbol.arcs.is_empty());
    let ys: Vec<f64> = symbol.lines.iter().map(|l| l[0][1]).collect();
    assert!(close(ys[0], 0.11) && close(ys[1], -0.11) && close(ys[2], 0.0));
    let glass = symbol.lines[2];
    assert!(close_point(glass[0], [1.8, 0.0]) && close_point(glass[1], [3.2, 0.0]));
}

#[test]
fn a_point_finds_its_wall_and_its_side() {
    let mut p = project();
    let south = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    let east = p.wall(p.brick, [5.0, 0.0], [5.0, 4.0]);
    let upper = wall::Placement::new(p.brick, p.first, [0.0, 0.0], [5.0, 0.0]);
    let upper = p.place(&upper).unwrap();
    let view = p.store.view();

    assert_eq!(
        opening::wall_at(&view, Some(p.ground), [2.0, 0.05]),
        Some(south)
    );
    assert_eq!(
        opening::wall_at(&view, Some(p.first), [2.0, 0.05]),
        Some(upper)
    );
    assert_eq!(
        opening::wall_at(&view, Some(p.ground), [5.1, 2.0]),
        Some(east)
    );
    assert_eq!(opening::wall_at(&view, Some(p.ground), [2.0, 1.0]), None);

    let on = opening::on_wall(&view, south, [2.0, -0.05]).unwrap();
    assert!(close(on.distance_along, 2.0));
    assert!(on.interior_side, "the interior is on the right, -y");
    let on = opening::on_wall(&view, south, [3.0, 0.05]).unwrap();
    assert!(!on.interior_side);
}
