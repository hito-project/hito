//! The HITO geometry port
//! ([ADR 0014](https://github.com/hito-project/hito/blob/main/docs/decisions/0014-geometry-kernel.md)).
//!
//! An element's geometry is stored as parameters, never as a mesh. A domain
//! describes it as a [`Solid`]: [`Profile`]s extruded or swept along straight
//! paths, combined by union and difference. This crate turns solids into
//! [`Mesh`]es and keeps them in a [`GeometryCache`], one per element:
//!
//! - Extrusions and sweeps are meshed here, in Rust.
//! - Unions and differences go to a [`Kernel`], an adapter behind this port.
//!   Kernels only see meshes and boolean programs ([`Csg`]), so the kernel
//!   can be replaced without changing anything above it.
//! - A domain adds a [`Generator`] per class: a function from an element's
//!   parameters to its solid. The cache reruns it when the element, or
//!   anything the generator read, changes.
//! - Elements are generated in parallel
//!   ([principle 5](https://github.com/hito-project/hito/blob/main/docs/vision/principles.md)).
//!
//! Lengths are in metres, in world coordinates, as `f64`. Meshes are a cache:
//! quantities and exports come from the parameters.

mod cache;
mod error;
mod frame;
mod generator;
mod kernel;
mod mesh;
mod profile;
mod solid;

pub use cache::{GeometryCache, Regenerated};
pub use error::GeometryError;
pub use frame::Frame;
pub use generator::{Generator, Inputs};
pub use kernel::{Csg, Kernel};
pub use mesh::Mesh;
pub use profile::Profile;
pub use solid::Solid;

/// How far a faceted curve may stray from the true curve: 1 mm. A circle of
/// 20 cm radius gets 32 segments.
pub const CHORD_TOLERANCE: f64 = 0.001;
