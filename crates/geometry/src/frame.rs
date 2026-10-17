//! Placement frames: where a profile sits in the world.

use crate::GeometryError;

/// A right-handed, orthonormal coordinate frame: an origin and three axes,
/// in world coordinates. It's IFC's `IfcAxis2Placement3D`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    origin: [f64; 3],
    x: [f64; 3],
    y: [f64; 3],
    z: [f64; 3],
}

impl Frame {
    /// The world frame.
    pub const WORLD: Frame = Frame {
        origin: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
        z: [0.0, 0.0, 1.0],
    };

    /// The world axes, moved to `origin`.
    pub fn at(origin: [f64; 3]) -> Self {
        Self {
            origin,
            ..Self::WORLD
        }
    }

    /// A frame at `origin` with its z axis along `z`. The x axis is `x`
    /// made perpendicular to z, so it only needs to point the right way.
    pub fn new(origin: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Result<Self, GeometryError> {
        let z = normalize(z).ok_or(GeometryError::InvalidFrame)?;
        let x = normalize(sub(x, scale(z, dot(x, z)))).ok_or(GeometryError::InvalidFrame)?;
        if !origin.iter().all(|c| c.is_finite()) {
            return Err(GeometryError::InvalidFrame);
        }
        Ok(Self {
            origin,
            x,
            y: cross(z, x),
            z,
        })
    }

    /// The frame a profile is swept in along the line from `start` to `end`:
    /// z along the line, y as close to world up as it can be, so a beam's
    /// profile stands upright. On a vertical line, x is world x.
    pub fn along(start: [f64; 3], end: [f64; 3]) -> Result<Self, GeometryError> {
        let z = normalize(sub(end, start)).ok_or(GeometryError::InvalidFrame)?;
        let x = normalize(cross([0.0, 0.0, 1.0], z)).unwrap_or([1.0, 0.0, 0.0]);
        Self::new(start, z, x)
    }

    pub fn origin(&self) -> [f64; 3] {
        self.origin
    }

    pub fn x_axis(&self) -> [f64; 3] {
        self.x
    }

    pub fn y_axis(&self) -> [f64; 3] {
        self.y
    }

    pub fn z_axis(&self) -> [f64; 3] {
        self.z
    }

    /// A point given in this frame's coordinates, in world coordinates.
    pub fn to_world(&self, [x, y, z]: [f64; 3]) -> [f64; 3] {
        let along = add(add(scale(self.x, x), scale(self.y, y)), scale(self.z, z));
        add(self.origin, along)
    }
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn length(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// `a` scaled to length 1, or `None` if it's too short or not finite.
fn normalize(a: [f64; 3]) -> Option<[f64; 3]> {
    let length = length(a);
    (length.is_finite() && length > 1e-12).then(|| scale(a, 1.0 / length))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        length(sub(a, b)) < 1e-12
    }

    #[test]
    fn a_frame_is_orthonormal_whatever_x_it_gets() {
        let frame = Frame::new([1.0, 2.0, 3.0], [0.0, 0.0, 2.0], [1.0, 1.0, 5.0]).unwrap();
        assert!(close(frame.z_axis(), [0.0, 0.0, 1.0]));
        assert!(close(frame.x_axis(), [0.5f64.sqrt(), 0.5f64.sqrt(), 0.0]));
        assert!(close(cross(frame.x_axis(), frame.y_axis()), frame.z_axis()));
        assert!(close(frame.to_world([0.0, 0.0, 1.0]), [1.0, 2.0, 4.0]));
    }

    #[test]
    fn a_beam_profile_stands_upright() {
        let frame = Frame::along([0.0; 3], [5.0, 0.0, 0.0]).unwrap();
        assert!(close(frame.y_axis(), [0.0, 0.0, 1.0]));
        assert!(close(frame.z_axis(), [1.0, 0.0, 0.0]));
    }

    #[test]
    fn a_vertical_sweep_keeps_world_x() {
        let frame = Frame::along([0.0; 3], [0.0, 0.0, 3.0]).unwrap();
        assert!(close(frame.x_axis(), [1.0, 0.0, 0.0]));
        assert!(close(frame.y_axis(), [0.0, 1.0, 0.0]));
    }

    #[test]
    fn parallel_or_zero_axes_are_refused() {
        let parallel = Frame::new([0.0; 3], [0.0, 0.0, 1.0], [0.0, 0.0, 3.0]);
        assert_eq!(parallel, Err(GeometryError::InvalidFrame));
        let zero = Frame::along([1.0; 3], [1.0; 3]);
        assert_eq!(zero, Err(GeometryError::InvalidFrame));
    }
}
