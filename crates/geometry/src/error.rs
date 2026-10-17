//! Why geometry couldn't be generated.

use std::fmt;

/// Why a solid couldn't be meshed. The element then has no geometry, and
/// the [`GeometryCache`](crate::GeometryCache) keeps the error for the UI to
/// show.
#[derive(Debug, Clone, PartialEq)]
pub enum GeometryError {
    /// A profile is degenerate: too few points, a size that isn't positive
    /// or a coordinate that isn't finite.
    InvalidProfile(String),
    /// A frame's axes are zero, parallel or not finite.
    InvalidFrame,
    /// An extrusion's depth or a sweep's length isn't positive.
    InvalidLength(f64),
    /// The kernel refused a boolean operation.
    Kernel(String),
}

impl fmt::Display for GeometryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfile(reason) => write!(f, "invalid profile: {reason}"),
            Self::InvalidFrame => write!(f, "invalid placement axes"),
            Self::InvalidLength(length) => write!(f, "length {length} m isn't positive"),
            Self::Kernel(reason) => write!(f, "geometry kernel: {reason}"),
        }
    }
}

impl std::error::Error for GeometryError {}
