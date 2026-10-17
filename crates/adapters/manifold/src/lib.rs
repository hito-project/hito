//! The solid-boolean kernel: [Manifold](https://github.com/elalish/manifold)
//! behind the geometry port
//! ([ADR 0014](https://github.com/hito-project/hito/blob/main/docs/decisions/0014-geometry-kernel.md)).
//!
//! Manifold is a C++ library, used through the `manifold-csg` bindings. It
//! only sees closed meshes and the unions and differences between them,
//! never profiles or parameters, so it can be replaced by another
//! [`Kernel`] without changes above the port.
//!
//! ```
//! use hito_geometry::{CHORD_TOLERANCE, Frame, Profile, Solid};
//! use hito_manifold::Manifold;
//!
//! let slab = Solid::Extrusion {
//!     profile: Profile::Rectangle { width: 6.0, height: 4.0 },
//!     frame: Frame::WORLD,
//!     depth: 0.15,
//! };
//! let shaft = Solid::Extrusion {
//!     profile: Profile::Rectangle { width: 1.0, height: 1.0 },
//!     frame: Frame::at([0.0, 0.0, -0.1]),
//!     depth: 0.35,
//! };
//! let slab = Solid::Difference { base: Box::new(slab), voids: vec![shaft] };
//! let mesh = slab.mesh(&Manifold, CHORD_TOLERANCE)?;
//! assert!((mesh.volume() - 23.0 * 0.15).abs() < 1e-9);
//! # Ok::<(), hito_geometry::GeometryError>(())
//! ```

use hito_geometry::{Csg, GeometryError, Kernel, Mesh};
use manifold_csg::Manifold as Solid;

/// The Manifold kernel. It holds nothing, so one value serves every thread.
#[derive(Debug, Clone, Copy, Default)]
pub struct Manifold;

impl Kernel for Manifold {
    fn evaluate(&self, csg: &Csg) -> Result<Mesh, GeometryError> {
        let solid = solid(csg)?;
        let (positions, stride, indices) = solid.to_mesh_f64();
        Ok(Mesh {
            positions: positions
                .chunks_exact(stride)
                .map(|p| [p[0], p[1], p[2]])
                .collect(),
            triangles: indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| t.map(|i| i as u32))
                .collect(),
        })
    }
}

/// The program as one Manifold solid.
fn solid(csg: &Csg) -> Result<Solid, GeometryError> {
    let solid = match csg {
        Csg::Mesh(mesh) => {
            let positions: Vec<f64> = mesh.positions.iter().flatten().copied().collect();
            let indices: Vec<u64> = mesh.triangles.iter().flatten().map(|&i| i.into()).collect();
            Solid::from_mesh_f64(&positions, 3, &indices).map_err(kernel_error)?
        }
        Csg::Union(parts) => Solid::batch_union(&all(parts)?),
        Csg::Difference { base, voids } => {
            let mut parts = vec![solid(base)?];
            parts.extend(all(voids)?);
            Solid::batch_difference(&parts)
        }
    };
    solid.status().map_err(kernel_error)?;
    Ok(solid)
}

fn all(programs: &[Csg]) -> Result<Vec<Solid>, GeometryError> {
    programs.iter().map(solid).collect()
}

fn kernel_error(error: impl std::fmt::Display) -> GeometryError {
    GeometryError::Kernel(error.to_string())
}
