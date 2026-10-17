//! The kernel port: what a boolean kernel adapter implements.

use crate::{GeometryError, Mesh};

/// A boolean program over closed meshes: what a [`Kernel`] evaluates. The
/// geometry port builds it from a [`Solid`](crate::Solid), so kernels never
/// see profiles or parameters.
#[derive(Debug, Clone, PartialEq)]
pub enum Csg {
    Mesh(Mesh),
    /// Everything inside any of the parts.
    Union(Vec<Csg>),
    /// The base, minus everything inside any of the voids.
    Difference {
        base: Box<Csg>,
        voids: Vec<Csg>,
    },
}

/// A solid-boolean kernel, such as the Manifold adapter (ADR 0014).
///
/// The cache calls it from several threads at once, each with its own
/// program.
pub trait Kernel: Send + Sync {
    /// The closed mesh of a boolean program. An empty mesh means nothing is
    /// left, such as a solid cut away entirely.
    fn evaluate(&self, csg: &Csg) -> Result<Mesh, GeometryError>;
}
