//! Finding elements on the screen: under the cursor, for a click, and in a
//! box, for window and crossing selection. It works on the meshes the
//! scene already holds, on the CPU, in f64.

use glam::{DVec2, DVec3};
use hito_core::ElementId;
use hito_geometry::Mesh;

use super::camera::{Bounds, Camera};
use super::scene::Scene;

/// Which elements a selection box takes, as in Revit and AutoCAD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxKind {
    /// Dragged left to right: only elements wholly inside the box.
    Window,
    /// Dragged right to left: elements inside the box or crossing its edge.
    Crossing,
}

impl BoxKind {
    /// The kind a drag from `start` to `end` draws.
    pub fn of_drag(start: DVec2, end: DVec2) -> Self {
        if end.x >= start.x {
            BoxKind::Window
        } else {
            BoxKind::Crossing
        }
    }
}

/// The elements under a screen position, nearest first. Tab cycles
/// through them.
pub fn under(scene: &Scene, camera: &Camera, screen: DVec2, size: DVec2) -> Vec<ElementId> {
    let (start, direction) = camera.ray(screen, size);
    let mut hits: Vec<(f64, ElementId)> = scene
        .meshes()
        .filter_map(|(id, mesh)| {
            let (lo, hi) = mesh.bounds()?;
            ray_hits_box(start, direction, Bounds::new(lo, hi))?;
            let distance = nearest_hit(mesh, start, direction)?;
            Some((distance, id))
        })
        .collect();
    hits.sort_by(|a, b| a.0.total_cmp(&b.0));
    hits.into_iter().map(|(_, id)| id).collect()
}

/// The elements a box between two screen corners takes.
pub fn in_box(
    scene: &Scene,
    camera: &Camera,
    corners: [DVec2; 2],
    kind: BoxKind,
    size: DVec2,
) -> Vec<ElementId> {
    let (lo, hi) = (corners[0].min(corners[1]), corners[0].max(corners[1]));
    let inside = |p: DVec2| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y;
    scene
        .meshes()
        .filter(|(_, mesh)| {
            let projected: Vec<Option<DVec2>> = mesh
                .positions
                .iter()
                .map(|&p| camera.project(p.into(), size))
                .collect();
            match kind {
                BoxKind::Window => projected.iter().all(|p| p.is_some_and(inside)),
                BoxKind::Crossing => mesh.triangles.iter().any(|t| {
                    let corners = t.map(|i| projected[i as usize]);
                    match corners {
                        [Some(a), Some(b), Some(c)] => triangle_meets_box([a, b, c], lo, hi),
                        _ => false,
                    }
                }),
            }
        })
        .map(|(id, _)| id)
        .collect()
}

/// Whether a ray meets a box, by the slab method.
fn ray_hits_box(start: DVec3, direction: DVec3, bounds: Bounds) -> Option<()> {
    let inverse = direction.recip();
    let a = (bounds.min - start) * inverse;
    let b = (bounds.max - start) * inverse;
    let near = a.min(b).max_element();
    let far = a.max(b).min_element();
    (far >= near.max(0.0)).then_some(())
}

/// How far outside a triangle a ray may pass and still hit it, in its
/// own coordinates, so a ray along the edge between two triangles hits
/// one of them.
const EDGE: f64 = 1e-9;

/// How far along the ray it first meets the mesh, by Möller and
/// Trumbore's test on each triangle.
fn nearest_hit(mesh: &Mesh, start: DVec3, direction: DVec3) -> Option<f64> {
    let at = |i: u32| DVec3::from(mesh.positions[i as usize]);
    mesh.triangles
        .iter()
        .filter_map(|&[a, b, c]| {
            let (a, b, c) = (at(a), at(b), at(c));
            let (ab, ac) = (b - a, c - a);
            let p = direction.cross(ac);
            let det = ab.dot(p);
            if det.abs() < 1e-12 * ab.length_squared().max(1e-24) {
                return None;
            }
            let s = start - a;
            let u = s.dot(p) / det;
            let q = s.cross(ab);
            let v = direction.dot(q) / det;
            let t = ac.dot(q) / det;
            (u >= -EDGE && v >= -EDGE && u + v <= 1.0 + EDGE && t >= 0.0).then_some(t)
        })
        .min_by(f64::total_cmp)
}

/// Whether a screen triangle touches an axis-aligned box.
fn triangle_meets_box(t: [DVec2; 3], lo: DVec2, hi: DVec2) -> bool {
    let inside = |p: DVec2| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y;
    if t.iter().any(|&p| inside(p)) {
        return true;
    }
    let corners = [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)];
    let side = |a: DVec2, b: DVec2, p: DVec2| (b - a).perp_dot(p - a);
    let in_triangle = |p: DVec2| {
        let s = [
            side(t[0], t[1], p),
            side(t[1], t[2], p),
            side(t[2], t[0], p),
        ];
        s.iter().all(|&x| x >= 0.0) || s.iter().all(|&x| x <= 0.0)
    };
    if corners.iter().any(|&c| in_triangle(c)) {
        return true;
    }
    let crosses = |a: DVec2, b: DVec2, c: DVec2, d: DVec2| {
        let (d1, d2) = (side(c, d, a), side(c, d, b));
        let (d3, d4) = (side(a, b, c), side(a, b, d));
        d1 * d2 < 0.0 && d3 * d4 < 0.0
    };
    (0..3).any(|i| {
        let (a, b) = (t[i], t[(i + 1) % 3]);
        (0..4).any(|j| crosses(a, b, corners[j], corners[(j + 1) % 4]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewport::camera::Projection;
    use crate::viewport::scene::tests::column;

    const SIZE: DVec2 = DVec2::new(800.0, 600.0);

    /// Two columns one behind the other seen from the front, and one to
    /// the side.
    fn scene() -> (Scene, [ElementId; 3], Camera) {
        let mut scene = Scene::default();
        let ids = [(); 3].map(|_| ElementId::generate());
        scene.insert(ids[0], column(0.0, 0.4));
        let behind = super::super::scene::tests::mesh(hito_geometry::Solid::Extrusion {
            profile: hito_geometry::Profile::Rectangle {
                width: 0.4,
                height: 0.4,
            },
            frame: hito_geometry::Frame::at([0.0, 3.0, 0.0]),
            depth: 3.0,
        });
        scene.insert(ids[1], behind);
        scene.insert(ids[2], column(6.0, 0.4));
        let mut camera = Camera::default();
        camera.look_from(-DVec3::Y);
        camera.fit(scene.bounds().unwrap(), SIZE);
        (scene, ids, camera)
    }

    #[test]
    fn under_the_cursor_nearest_first() {
        let (scene, ids, mut camera) = scene();
        for projection in [Projection::Orthographic, Projection::Perspective] {
            camera.set_projection(projection);
            let at = camera.project(DVec3::new(0.0, 0.0, 1.5), SIZE).unwrap();
            let hits = under(&scene, &camera, at, SIZE);
            // In perspective the column behind is off to the side.
            let expected: &[ElementId] = match projection {
                Projection::Orthographic => &[ids[0], ids[1]],
                Projection::Perspective => &[ids[0]],
            };
            assert_eq!(hits, expected);
            let empty = camera.project(DVec3::new(3.0, 0.0, 1.5), SIZE).unwrap();
            assert!(under(&scene, &camera, empty, SIZE).is_empty());
        }
    }

    #[test]
    fn window_takes_only_whole_elements_and_crossing_takes_touched_ones() {
        let (scene, ids, camera) = scene();
        let p = |x, z| camera.project(DVec3::new(x, 0.0, z), SIZE).unwrap();
        // Round the first column's middle only.
        let corners = [p(-1.0, 1.0), p(1.0, 2.0)];
        assert!(in_box(&scene, &camera, corners, BoxKind::Window, SIZE).is_empty());
        let mut crossing = in_box(&scene, &camera, corners, BoxKind::Crossing, SIZE);
        crossing.sort();
        let mut expected = vec![ids[0], ids[1]];
        expected.sort();
        assert_eq!(crossing, expected);
        // Round all of both front columns, but not the far one.
        let corners = [p(-1.0, -0.5), p(1.0, 3.5)];
        let mut window = in_box(&scene, &camera, corners, BoxKind::Window, SIZE);
        window.sort();
        assert_eq!(window, expected);
    }

    #[test]
    fn a_crossing_box_inside_one_face_takes_it() {
        let (scene, ids, camera) = scene();
        let p = |x, z| camera.project(DVec3::new(x, 0.0, z), SIZE).unwrap();
        let corners = [p(-0.05, 1.4), p(0.05, 1.6)];
        assert!(
            in_box(&scene, &camera, corners, BoxKind::Crossing, SIZE).contains(&ids[0]),
            "the box's corners are inside a triangle"
        );
    }

    #[test]
    fn drag_direction_picks_the_kind() {
        let (a, b) = (DVec2::new(10.0, 10.0), DVec2::new(50.0, 5.0));
        assert_eq!(BoxKind::of_drag(a, b), BoxKind::Window);
        assert_eq!(BoxKind::of_drag(b, a), BoxKind::Crossing);
    }
}
