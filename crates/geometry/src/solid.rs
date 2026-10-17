//! Solids: an element's geometry as parameters.

use earcut::Earcut;

use crate::frame::length;
use crate::{Csg, Frame, GeometryError, Kernel, Mesh, Profile};

/// A solid described by its parameters: what a domain's
/// [`Generator`](crate::Generator) returns. IFC export writes these
/// parameters (an extrusion as `IfcExtrudedAreaSolid`, a difference as
/// openings), never the mesh.
///
/// ```
/// use hito_geometry::{Frame, Profile, Solid};
///
/// // A 5 m wall, 20 cm thick and 3 m high, with a door.
/// let wall = Solid::Extrusion {
///     profile: Profile::Rectangle { width: 5.0, height: 0.2 },
///     frame: Frame::at([2.5, 0.1, 0.0]),
///     depth: 3.0,
/// };
/// let door = Solid::Extrusion {
///     profile: Profile::Rectangle { width: 1.0, height: 0.3 },
///     frame: Frame::at([2.5, 0.1, 0.0]),
///     depth: 2.1,
/// };
/// let wall = Solid::Difference { base: Box::new(wall), voids: vec![door] };
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum Solid {
    /// A profile in the frame's x-y plane, extruded along its z axis.
    Extrusion {
        profile: Profile,
        frame: Frame,
        depth: f64,
    },
    /// A profile swept along the straight line from `start` to `end`, upright
    /// as [`Frame::along`] places it, with its origin on the line. A beam.
    Sweep {
        profile: Profile,
        start: [f64; 3],
        end: [f64; 3],
    },
    /// Everything inside any of the parts.
    Union(Vec<Solid>),
    /// The base, minus everything inside any of the voids: openings, cuts.
    Difference { base: Box<Solid>, voids: Vec<Solid> },
}

impl Solid {
    /// The solid's closed mesh. Extrusions and sweeps are meshed here; the
    /// kernel only runs if there's a union or a difference. Curves stay
    /// within `tolerance` of the true curve.
    pub fn mesh(&self, kernel: &dyn Kernel, tolerance: f64) -> Result<Mesh, GeometryError> {
        match self.csg(tolerance)? {
            Csg::Mesh(mesh) => Ok(mesh),
            csg => kernel.evaluate(&csg),
        }
    }

    /// The boolean program for the kernel, with every extrusion and sweep
    /// meshed.
    pub fn csg(&self, tolerance: f64) -> Result<Csg, GeometryError> {
        let all = |solids: &[Solid]| -> Result<Vec<Csg>, GeometryError> {
            solids.iter().map(|s| s.csg(tolerance)).collect()
        };
        Ok(match self {
            Self::Extrusion {
                profile,
                frame,
                depth,
            } => Csg::Mesh(prism(&profile.loops(tolerance)?, frame, *depth)?),
            Self::Sweep {
                profile,
                start,
                end,
            } => {
                let frame = Frame::along(*start, *end)?;
                let depth = length(crate::frame::sub(*end, *start));
                Csg::Mesh(prism(&profile.loops(tolerance)?, &frame, depth)?)
            }
            Self::Union(parts) => Csg::Union(all(parts)?),
            Self::Difference { base, voids } => Csg::Difference {
                base: Box::new(base.csg(tolerance)?),
                voids: all(voids)?,
            },
        })
    }
}

/// The closed mesh of `loops` (the outline anticlockwise, then the holes
/// clockwise) extruded `depth` along the frame's z axis.
fn prism(loops: &[Vec<[f64; 2]>], frame: &Frame, depth: f64) -> Result<Mesh, GeometryError> {
    if !(depth > 0.0 && depth.is_finite()) {
        return Err(GeometryError::InvalidLength(depth));
    }
    let points: Vec<[f64; 2]> = loops.iter().flatten().copied().collect();
    let n = points.len() as u32;
    let mut holes = Vec::new();
    let mut start = 0;
    for l in loops {
        if start > 0 {
            holes.push(start);
        }
        start += l.len() as u32;
    }

    let mut cap = Vec::new();
    Earcut::new().earcut(points.iter().copied(), &holes, &mut cap);
    if cap.is_empty() {
        return Err(GeometryError::InvalidProfile(
            "the profile couldn't be triangulated".into(),
        ));
    }

    // Bottom corners are 0..n, top corners n..2n.
    let bottom = points.iter().map(|&[x, y]| frame.to_world([x, y, 0.0]));
    let top = points.iter().map(|&[x, y]| frame.to_world([x, y, depth]));
    let positions = bottom.chain(top).collect();

    let mut triangles = Vec::with_capacity(cap.len() / 3 * 2 + points.len() * 2);
    for &[a, b, c] in cap.as_chunks::<3>().0 {
        let [pa, pb, pc] = [a, b, c].map(|i| points[i as usize]);
        let anticlockwise =
            (pb[0] - pa[0]) * (pc[1] - pa[1]) - (pb[1] - pa[1]) * (pc[0] - pa[0]) > 0.0;
        let [a, b, c] = if anticlockwise { [a, b, c] } else { [a, c, b] };
        triangles.push([n + a, n + b, n + c]);
        triangles.push([c, b, a]);
    }
    // Sides: the loops' winding makes every quad face outwards.
    let mut first = 0;
    for l in loops {
        let len = l.len() as u32;
        for i in 0..len {
            let (a, b) = (first + i, first + (i + 1) % len);
            triangles.push([a, b, n + b]);
            triangles.push([a, n + b, n + a]);
        }
        first += len;
    }
    Ok(Mesh {
        positions,
        triangles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CHORD_TOLERANCE;

    /// A kernel for solids without booleans.
    struct NoKernel;

    impl Kernel for NoKernel {
        fn evaluate(&self, _: &Csg) -> Result<Mesh, GeometryError> {
            Err(GeometryError::Kernel("no kernel in this test".into()))
        }
    }

    fn mesh(solid: &Solid) -> Mesh {
        solid.mesh(&NoKernel, CHORD_TOLERANCE).unwrap()
    }

    #[test]
    fn a_column_is_its_area_times_its_height() {
        let column = Solid::Extrusion {
            profile: Profile::Rectangle {
                width: 0.3,
                height: 0.4,
            },
            frame: Frame::at([5_500_000.0, 6_100_000.0, 3.0]),
            depth: 2.8,
        };
        let mesh = mesh(&column);
        assert!(mesh.is_closed());
        // Far from the origin, corners round to about 1e-9 m.
        assert!((mesh.volume() - 0.3 * 0.4 * 2.8).abs() < 1e-8);
        let (lo, hi) = mesh.bounds().unwrap();
        assert_eq!(lo[2], 3.0);
        assert!((hi[2] - 5.8).abs() < 1e-12);
    }

    #[test]
    fn a_hollow_profile_keeps_its_hole() {
        let tube = Solid::Extrusion {
            profile: Profile::Polygon {
                outline: vec![[0.0, 0.0], [0.2, 0.0], [0.2, 0.2], [0.0, 0.2]],
                holes: vec![vec![[0.02, 0.02], [0.18, 0.02], [0.18, 0.18], [0.02, 0.18]]],
            },
            frame: Frame::WORLD,
            depth: 6.0,
        };
        let mesh = mesh(&tube);
        assert!(mesh.is_closed());
        assert!((mesh.volume() - (0.04 - 0.0256) * 6.0).abs() < 1e-12);
    }

    #[test]
    fn a_non_convex_profile_meshes_closed() {
        // An I section, 30 cm deep.
        let i_section = Profile::polygon([
            [-0.075, -0.15],
            [0.075, -0.15],
            [0.075, -0.14],
            [0.005, -0.14],
            [0.005, 0.14],
            [0.075, 0.14],
            [0.075, 0.15],
            [-0.075, 0.15],
            [-0.075, 0.14],
            [-0.005, 0.14],
            [-0.005, -0.14],
            [-0.075, -0.14],
        ]);
        let area = i_section.area();
        let beam = Solid::Sweep {
            profile: i_section,
            start: [0.0, 0.0, 3.0],
            end: [6.0, 8.0, 3.0],
        };
        let mesh = mesh(&beam);
        assert!(mesh.is_closed());
        assert!((mesh.volume() - area * 10.0).abs() < 1e-12);
    }

    #[test]
    fn a_circular_column_is_close_to_pi_r_squared_h() {
        let column = Solid::Sweep {
            profile: Profile::Circle { radius: 0.2 },
            start: [0.0; 3],
            end: [0.0, 0.0, 3.0],
        };
        let mesh = mesh(&column);
        assert!(mesh.is_closed());
        let exact = std::f64::consts::PI * 0.04 * 3.0;
        // Faceted: under the true volume, by under 1%.
        assert!(mesh.volume() < exact && mesh.volume() > exact * 0.99);
    }

    #[test]
    fn booleans_go_to_the_kernel() {
        let block = Solid::Extrusion {
            profile: Profile::Rectangle {
                width: 1.0,
                height: 1.0,
            },
            frame: Frame::WORLD,
            depth: 1.0,
        };
        let union = Solid::Union(vec![block.clone(), block]);
        let error = union.mesh(&NoKernel, CHORD_TOLERANCE).unwrap_err();
        assert!(matches!(error, GeometryError::Kernel(_)));
        let Csg::Union(parts) = union.csg(CHORD_TOLERANCE).unwrap() else {
            panic!("a union's program is a union");
        };
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn a_zero_depth_or_length_is_refused() {
        let flat = Solid::Extrusion {
            profile: Profile::Circle { radius: 1.0 },
            frame: Frame::WORLD,
            depth: 0.0,
        };
        let point = Solid::Sweep {
            profile: Profile::Circle { radius: 1.0 },
            start: [1.0; 3],
            end: [1.0; 3],
        };
        let flat = flat.mesh(&NoKernel, CHORD_TOLERANCE).unwrap_err();
        assert_eq!(flat, GeometryError::InvalidLength(0.0));
        let point = point.mesh(&NoKernel, CHORD_TOLERANCE).unwrap_err();
        assert_eq!(point, GeometryError::InvalidFrame);
    }
}
