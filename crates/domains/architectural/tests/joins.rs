//! Walls join on their own, layer by layer (#193): corners, T-junctions,
//! crossings and junctions of more walls, through the public API only.
//!
//! Every test checks the two things that make a join clean: no two layer
//! outlines overlap, and together they cover exactly the walls' union, so
//! there's no gap either.

mod common;

use common::{Project, assert_disjoint, close, project, total, unconnected};
use hito_architectural::class;
use hito_architectural::footprint::End;
use hito_architectural::join::{self, CROSSES};
use hito_architectural::plan::Point;
use hito_architectural::wall::{self, START_Y};
use hito_core::ElementId;

/// The points of `polygons` on the line x + y = `c`.
fn on_line(polygons: &[Vec<Point>], c: f64) -> Vec<Point> {
    let mut points: Vec<Point> = polygons
        .iter()
        .flatten()
        .filter(|q| (q[0] + q[1] - c).abs() < 1e-9)
        .map(|q| q.map(|x| (x * 1e9).round() / 1e9))
        .collect();
    points.sort_by(|a, b| a.partial_cmp(b).unwrap());
    points.dedup();
    points
}

#[test]
fn a_corner_joins_layer_by_layer_on_the_bisector() {
    let mut p = project();
    let [a, b] = p.chain(p.brick, &[[0.0, 0.0], [5.0, 0.0], [5.0, 4.0]])[..] else {
        unreachable!()
    };
    let view = p.store.view();
    assert_eq!(join::joined_at(&view, a, End::End), [b]);
    assert_eq!(join::joined_at(&view, b, End::Start), [a]);
    assert!(join::joined_at(&view, a, End::Start).is_empty());

    let pieces = p.pieces(&[a, b]);
    assert_disjoint(&pieces);
    // The L of two 22 cm walls, measured to their outer faces.
    let union = 5.11 * 0.22 + 0.22 * 4.11 - 0.22 * 0.22;
    assert!(close(total(&pieces), union), "{}", total(&pieces));

    // Render meets render, brick meets brick, plaster meets plaster, on the
    // line x + y = 5 through the corner.
    for i in 0..3 {
        let mine = on_line(&p.layer(a, i), 5.0);
        let theirs = on_line(&p.layer(b, i), 5.0);
        assert_eq!(mine.len(), 2, "layer {i}");
        assert_eq!(mine, theirs, "layer {i}");
    }
}

#[test]
fn a_corner_of_two_types_is_clean() {
    let mut p = project();
    let a = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    let b = p.wall(p.concrete, [5.0, 0.0], [5.0, 4.0]);
    let pieces = p.pieces(&[a, b]);
    assert_disjoint(&pieces);
    let union = 5.1 * 0.22 + 0.2 * 4.11 - 0.2 * 0.22;
    assert!(close(total(&pieces), union), "{}", total(&pieces));
}

#[test]
fn walls_in_line_meet_square() {
    let mut p = project();
    let walls = p.chain(p.brick, &[[0.0, 0.0], [3.0, 0.0], [6.0, 0.0]]);
    let pieces = p.pieces(&walls);
    assert_disjoint(&pieces);
    assert!(close(total(&pieces), 6.0 * 0.22));
    assert!(close(total(&p.pieces(&walls[..1])), 3.0 * 0.22));
}

#[test]
fn a_closed_chain_makes_a_clean_ring() {
    let mut p = project();
    let corners = [[0.0, 0.0], [6.0, 0.0], [6.0, 4.0], [0.0, 4.0], [0.0, 0.0]];
    let walls = p.chain(p.brick, &corners);
    // The last wall's end joins the first wall's start.
    let view = p.store.view();
    assert_eq!(join::joined_at(&view, walls[3], End::End), [walls[0]]);
    assert_eq!(join::joined_at(&view, walls[0], End::Start), [walls[3]]);

    let pieces = p.pieces(&walls);
    assert_disjoint(&pieces);
    let ring = 6.22 * 4.22 - 5.78 * 3.78;
    assert!(close(total(&pieces), ring), "{}", total(&pieces));
}

/// A 10 m wall along x, and a 4 m wall from its middle up, on its exterior
/// side.
fn tee(p: &mut Project) -> (ElementId, ElementId) {
    let a = p.wall(p.brick, [0.0, 0.0], [10.0, 0.0]);
    let b = p.wall(p.brick, [5.0, 0.0], [5.0, 4.0]);
    (a, b)
}

#[test]
fn at_a_t_core_meets_core_and_finish_meets_finish() {
    let mut p = project();
    let (a, b) = tee(&mut p);
    let view = p.store.view();
    assert_eq!(join::joined_at(&view, b, End::Start), [a]);
    assert!(join::joined_at(&view, a, End::Start).is_empty());
    assert!(join::joined_at(&view, a, End::End).is_empty());

    let pieces = p.pieces(&[a, b]);
    assert_disjoint(&pieces);
    let union = 10.0 * 0.22 + 0.22 * (4.0 - 0.11);
    assert!(close(total(&pieces), union), "{}", total(&pieces));

    // The through wall's render is cut where the brick passes.
    let render = p.layer(a, 0);
    assert_eq!(render.len(), 2);
    assert!(close(total(&render), 0.02 * (10.0 - 0.18)));
    assert!(close(total(&p.layer(a, 2)), 0.02 * 10.0));
    // The brick runs on to the other wall's brick; the finishes stop at
    // its face.
    assert!(close(total(&p.layer(b, 1)), 0.18 * (4.0 - 0.09)));
    assert!(close(total(&p.layer(b, 0)), 0.02 * (4.0 - 0.11)));
    assert!(close(total(&p.layer(b, 2)), 0.02 * (4.0 - 0.11)));
}

#[test]
fn moving_a_wall_away_undoes_its_join() {
    let mut p = project();
    let (a, b) = tee(&mut p);
    let mut tx = p.store.transaction();
    tx.set_property(b, START_Y, 1.0).unwrap();
    join::update(&mut tx, &[b]).unwrap();
    tx.commit().unwrap();
    assert!(join::joined_at(&p.store.view(), b, End::Start).is_empty());
    assert_eq!(p.layer(a, 0).len(), 1);

    // And back: the join comes back.
    let mut tx = p.store.transaction();
    tx.set_property(b, START_Y, 0.0).unwrap();
    join::update(&mut tx, &[b]).unwrap();
    tx.commit().unwrap();
    assert_eq!(join::joined_at(&p.store.view(), b, End::Start), [a]);
    assert_eq!(p.layer(a, 0).len(), 2);
}

#[test]
fn moving_the_through_wall_updates_the_t() {
    let mut p = project();
    let (a, b) = tee(&mut p);
    // Slide the through wall along itself: the T stays.
    let mut tx = p.store.transaction();
    tx.set_property(a, wall::START_X, -2.0).unwrap();
    join::update(&mut tx, &[a]).unwrap();
    tx.commit().unwrap();
    assert_eq!(join::joined_at(&p.store.view(), b, End::Start), [a]);
    let pieces = p.pieces(&[a, b]);
    assert_disjoint(&pieces);
    assert!(close(total(&pieces), 12.0 * 0.22 + 0.22 * 3.89));
}

#[test]
fn a_join_applies_only_where_both_walls_stand() {
    let mut p = project();
    let a = p.wall(p.brick, [0.0, 0.0], [10.0, 0.0]);
    let low = p
        .placement(p.brick, [5.0, 0.0], [5.0, 4.0])
        .with_top(unconnected(1.2));
    p.place(&low).unwrap();

    let footprint = p.footprint(a);
    assert_eq!(footprint.slices.len(), 2);
    let [below, above] = &footprint.slices[..] else {
        unreachable!()
    };
    assert_eq!((below.bottom, below.top), (0.0, 1.2));
    assert_eq!((above.bottom, above.top), (1.2, 3.0));
    assert_eq!(below.layers[0].polygons.len(), 2);
    assert_eq!(above.layers[0].polygons.len(), 1);
}

#[test]
fn walls_on_different_floors_dont_join() {
    let mut p = project();
    let a = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    let upstairs =
        hito_architectural::wall::Placement::new(p.brick, p.first, [5.0, 0.0], [5.0, 4.0]);
    let b = p.place(&upstairs).unwrap();
    let view = p.store.view();
    assert!(join::joined_at(&view, a, End::End).is_empty());
    assert!(join::joined_at(&view, b, End::Start).is_empty());
}

#[test]
fn distant_walls_dont_join() {
    let mut p = project();
    let a = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    // Its start is 30 cm from the first wall's line: outside its 11 cm.
    let b = p.wall(p.brick, [2.0, 0.3], [2.0, 4.0]);
    let view = p.store.view();
    assert!(join::joined_at(&view, b, End::Start).is_empty());
    assert_eq!(view.relationships_to(a).count(), 0);
}

fn crosses(p: &Project, a: ElementId, b: ElementId) -> bool {
    let view = p.store.view();
    let crosses = class(CROSSES);
    view.relationships_from(a)
        .chain(view.relationships_from(b))
        .any(|r| r.class() == &crosses)
}

#[test]
fn at_a_crossing_the_thicker_core_goes_through() {
    let mut p = project();
    let a = p.wall(p.concrete, [0.0, 0.0], [10.0, 0.0]);
    let b = p.wall(p.brick, [5.0, -3.0], [5.0, 3.0]);
    assert!(crosses(&p, a, b));

    let pieces = p.pieces(&[a, b]);
    assert_disjoint(&pieces);
    assert!(close(total(&pieces), 10.0 * 0.2 + 0.22 * 5.8));
    // The concrete is whole; every brick wall layer is in two.
    assert_eq!(p.layer(a, 0).len(), 1);
    for i in 0..3 {
        assert_eq!(p.layer(b, i).len(), 2, "layer {i}");
    }
    assert!(close(total(&p.layer(b, 1)), 0.18 * 5.8));
}

#[test]
fn a_crossing_of_one_type_is_clean() {
    let mut p = project();
    let a = p.wall(p.brick, [0.0, 0.0], [10.0, 0.0]);
    let b = p.wall(p.brick, [5.0, -3.0], [5.0, 3.0]);
    let pieces = p.pieces(&[a, b]);
    assert_disjoint(&pieces);
    assert!(close(total(&pieces), 10.0 * 0.22 + 0.22 * 5.78));
    // One wall's finishes are cut where the other's brick passes, on both
    // sides; the other's are cut by the whole wall.
    let a_render = p.layer(a, 0).len();
    let b_render = p.layer(b, 0).len();
    assert_eq!(a_render + b_render, 4);
}

#[test]
fn three_walls_meeting_make_a_t() {
    let mut p = project();
    let a = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    let b = p.wall(p.brick, [5.0, 0.0], [10.0, 0.0]);
    let c = p.wall(p.brick, [5.0, 0.0], [5.0, 4.0]);
    let pieces = p.pieces(&[a, b, c]);
    assert_disjoint(&pieces);
    assert!(close(total(&pieces), 10.0 * 0.22 + 0.22 * 3.89));
    // The walls in line go through; the third abuts them.
    assert!(close(total(&p.layer(a, 0)), 0.02 * 4.91));
    assert!(close(total(&p.layer(b, 0)), 0.02 * 4.91));
    assert!(close(total(&p.layer(c, 1)), 0.18 * 3.91));
}

#[test]
fn four_walls_meeting_make_a_cross() {
    let mut p = project();
    let a = p.wall(p.brick, [0.0, 0.0], [5.0, 0.0]);
    let b = p.wall(p.brick, [5.0, 0.0], [10.0, 0.0]);
    let c = p.wall(p.brick, [5.0, 0.0], [5.0, 4.0]);
    let d = p.wall(p.brick, [5.0, 0.0], [5.0, -4.0]);
    let pieces = p.pieces(&[a, b, c, d]);
    assert_disjoint(&pieces);
    assert!(close(total(&pieces), 10.0 * 0.22 + 0.22 * (8.0 - 0.22)));
}

#[test]
fn deleting_a_wall_removes_its_joins() {
    let mut p = project();
    let (a, b) = tee(&mut p);
    let mut tx = p.store.transaction();
    tx.delete(b).unwrap();
    tx.commit().unwrap();
    assert_eq!(p.store.view().relationships_to(a).count(), 0);
    assert_eq!(p.layer(a, 0).len(), 1);
}
