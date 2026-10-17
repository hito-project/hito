//! Triangle meshes: what solids are evaluated into.

use std::collections::HashMap;

/// An indexed triangle mesh in world coordinates. Each triangle lists its
/// corners anticlockwise seen from outside, so its normal points out.
///
/// A mesh is derived data, rebuilt from the parameters. A closed mesh shares
/// its corners between triangles, as kernels need.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mesh {
    pub positions: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }

    /// The enclosed volume, by the divergence theorem. Exact for planar
    /// solids, and only meaningful for a [closed](Mesh::is_closed) mesh.
    ///
    /// Positions are taken relative to the first one, so a solid far from
    /// the origin keeps its precision.
    pub fn volume(&self) -> f64 {
        let Some(&origin) = self.positions.first() else {
            return 0.0;
        };
        let at = |i: u32| {
            let p = self.positions[i as usize];
            [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]]
        };
        let six_times: f64 = self
            .triangles
            .iter()
            .map(|&[a, b, c]| {
                let (a, b, c) = (at(a), at(b), at(c));
                a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0])
            })
            .sum();
        six_times / 6.0
    }

    /// Whether the mesh is watertight and consistently oriented: every edge
    /// is used once in each direction.
    pub fn is_closed(&self) -> bool {
        let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
        for &[a, b, c] in &self.triangles {
            for edge in [(a, b), (b, c), (c, a)] {
                *edges.entry(edge).or_default() += 1;
            }
        }
        let paired = |&(from, to): &(u32, u32)| edges.get(&(to, from)) == Some(&1);
        !edges.is_empty() && edges.iter().all(|(edge, &n)| n == 1 && paired(edge))
    }

    /// The smallest and largest corner, or `None` if the mesh is empty.
    pub fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        let first = *self.positions.first()?;
        Some(self.positions.iter().fold((first, first), |(lo, hi), p| {
            (
                [lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])],
                [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])],
            )
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit tetrahedron, outward facing, moved to `at`.
    fn tetrahedron(at: [f64; 3]) -> Mesh {
        let corners = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        Mesh {
            positions: corners
                .map(|p| [p[0] + at[0], p[1] + at[1], p[2] + at[2]])
                .to_vec(),
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }
    }

    #[test]
    fn a_tetrahedron_has_a_sixth_of_a_cube() {
        let mesh = tetrahedron([0.0; 3]);
        assert!(mesh.is_closed());
        assert!((mesh.volume() - 1.0 / 6.0).abs() < 1e-15);
    }

    #[test]
    fn volume_keeps_its_precision_far_from_the_origin() {
        let mesh = tetrahedron([5_500_000.0, 6_100_000.0, 0.0]);
        assert!((mesh.volume() - 1.0 / 6.0).abs() < 1e-9);
    }

    #[test]
    fn an_open_or_flipped_mesh_isnt_closed() {
        let mut open = tetrahedron([0.0; 3]);
        open.triangles.pop();
        assert!(!open.is_closed());
        let mut flipped = tetrahedron([0.0; 3]);
        flipped.triangles[0] = [0, 1, 2];
        assert!(!flipped.is_closed());
        assert!(!Mesh::default().is_closed());
    }

    #[test]
    fn bounds_span_every_corner() {
        let mesh = tetrahedron([2.0, 0.0, 0.0]);
        assert_eq!(mesh.bounds(), Some(([2.0, 0.0, 0.0], [3.0, 1.0, 1.0])));
        assert_eq!(Mesh::default().bounds(), None);
    }
}
