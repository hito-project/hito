//! What a plan view draws of an element, from its mesh: where a horizontal
//! plane cuts it, and its edges seen from above. Plain geometry in f64,
//! with no egui in it.
//!
//! Cutting is done here, on the meshes the scene already holds, so any
//! element with geometry shows in plan without code of its own. Manifold
//! can slice a solid too, but the geometry port only takes boolean
//! programs, and the meshes would go back to the kernel every time the cut
//! changes; a triangle at a time against a plane is a few lines.

use glam::{DVec2, DVec3};
use hito_geometry::Mesh;

use super::scene;

/// A line in plan, from one point to another.
pub type Segment = [DVec2; 2];

/// Where the horizontal plane at height `z` cuts the mesh: one segment for
/// each triangle it crosses. A closed mesh's segments join up into closed
/// outlines. Faces lying in the plane are left out, as the faces just
/// above or below it draw the same outline.
pub fn section(mesh: &Mesh, z: f64) -> Vec<Segment> {
    let at = |i: u32| DVec3::from(mesh.positions[i as usize]);
    let (lo, hi) = match mesh.bounds() {
        Some((lo, hi)) => (lo[2], hi[2]),
        None => return Vec::new(),
    };
    if z < lo || z > hi {
        return Vec::new();
    }
    let mut segments = Vec::new();
    for &[a, b, c] in &mesh.triangles {
        let corners = [at(a), at(b), at(c)];
        // A corner on the plane counts as above it, so a triangle touching
        // the plane at one corner gives nothing, and one with an edge in
        // it gives nothing either: the triangle beside it gives that edge.
        let above = corners.map(|p| p.z >= z);
        let mut points = Vec::with_capacity(2);
        for i in 0..3 {
            let (p, q) = (corners[i], corners[(i + 1) % 3]);
            if above[i] != above[(i + 1) % 3] {
                let t = (z - p.z) / (q.z - p.z);
                points.push((p + (q - p) * t).truncate());
            }
        }
        if let [p, q] = points[..]
            && p.distance_squared(q) > 0.0
        {
            segments.push([p, q]);
        }
    }
    segments
}

/// The mesh's edges seen from straight above: where it ends or where its
/// faces meet at a crease, as the 3D views draw them. Vertical edges are
/// points from above, so they're left out.
pub fn projection(mesh: &Mesh) -> Vec<Segment> {
    let at = |i: u32| DVec3::from(mesh.positions[i as usize]).truncate();
    let normals = scene::normals(mesh);
    scene::creases(mesh, &normals)
        .into_iter()
        .map(|(a, b)| [at(a), at(b)])
        .filter(|[p, q]| p.distance_squared(*q) > 1e-18)
        .collect()
}

/// How far a point is from a segment.
pub fn distance(point: DVec2, [a, b]: Segment) -> f64 {
    let ab = b - a;
    let t = ((point - a).dot(ab) / ab.length_squared().max(f64::MIN_POSITIVE)).clamp(0.0, 1.0);
    point.distance(a + ab * t)
}

/// Whether a segment meets the box from `lo` to `hi`, by clipping it to
/// the box (Liang and Barsky).
pub fn meets_box([a, b]: Segment, lo: DVec2, hi: DVec2) -> bool {
    let d = b - a;
    let (mut start, mut end) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-d.x, a.x - lo.x),
        (d.x, hi.x - a.x),
        (-d.y, a.y - lo.y),
        (d.y, hi.y - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                start = start.max(t);
            } else {
                end = end.min(t);
            }
        }
    }
    start <= end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewport::scene::tests::column;

    fn length(segments: &[Segment]) -> f64 {
        segments.iter().map(|[a, b]| a.distance(*b)).sum()
    }

    #[test]
    fn a_column_cut_halfway_up_is_its_square() {
        // 40 cm square, centred on (2, 0), from 0 to 3 m.
        let mesh = column(2.0, 0.4);
        let cut = section(&mesh, 1.2);
        assert!((length(&cut) - 1.6).abs() < 1e-9, "{cut:?}");
        for [a, b] in &cut {
            for p in [a, b] {
                let on_side =
                    ((p.x - 2.0).abs() - 0.2).abs() < 1e-9 || (p.y.abs() - 0.2).abs() < 1e-9;
                assert!(on_side, "{p}");
            }
        }
        assert!(section(&mesh, 3.5).is_empty(), "above it");
        assert!(section(&mesh, -0.1).is_empty(), "below it");
    }

    #[test]
    fn a_cut_through_its_top_face_draws_it_once() {
        let mesh = column(0.0, 0.4);
        let cut = section(&mesh, 3.0);
        assert!((length(&cut) - 1.6).abs() < 1e-9, "{cut:?}");
    }

    #[test]
    fn seen_from_above_a_column_is_its_square() {
        let edges = projection(&column(0.0, 0.4));
        // The top and bottom squares, one over the other.
        assert_eq!(edges.len(), 8);
        assert!((length(&edges) - 3.2).abs() < 1e-9);
    }

    #[test]
    fn segments_against_points_and_boxes() {
        let s = [DVec2::new(0.0, 0.0), DVec2::new(4.0, 0.0)];
        assert!((distance(DVec2::new(2.0, 1.0), s) - 1.0).abs() < 1e-12);
        assert!((distance(DVec2::new(-3.0, 4.0), s) - 5.0).abs() < 1e-12);
        let (lo, hi) = (DVec2::new(1.0, -1.0), DVec2::new(2.0, 1.0));
        assert!(meets_box(s, lo, hi), "crosses it");
        assert!(!meets_box(s, DVec2::new(5.0, -1.0), DVec2::new(6.0, 1.0)));
        let inside = [DVec2::new(1.2, 0.0), DVec2::new(1.8, 0.5)];
        assert!(meets_box(inside, lo, hi));
    }
}
