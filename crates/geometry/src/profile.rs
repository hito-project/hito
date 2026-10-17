//! Profiles: the 2D shapes that are extruded and swept.

use std::f64::consts::PI;

use crate::GeometryError;

/// A closed 2D shape in its own x-y plane, as IFC's `IfcProfileDef`.
///
/// A profile keeps its parameters, so its [area](Profile::area) is exact: a
/// circle's is πr², not its polygon's. Curves are only faceted when it's
/// meshed.
#[derive(Debug, Clone, PartialEq)]
pub enum Profile {
    /// A rectangle centred on the origin, `width` along x and `height`
    /// along y.
    Rectangle { width: f64, height: f64 },
    /// A circle centred on the origin.
    Circle { radius: f64 },
    /// A polygon given by its corners, with polygonal holes. Either winding
    /// works. Holes must lie inside the outline and not overlap.
    Polygon {
        outline: Vec<[f64; 2]>,
        holes: Vec<Vec<[f64; 2]>>,
    },
}

impl Profile {
    /// A polygon without holes.
    pub fn polygon(outline: impl Into<Vec<[f64; 2]>>) -> Self {
        Self::Polygon {
            outline: outline.into(),
            holes: Vec::new(),
        }
    }

    /// The exact area, net of holes.
    pub fn area(&self) -> f64 {
        match self {
            Self::Rectangle { width, height } => width * height,
            Self::Circle { radius } => PI * radius * radius,
            Self::Polygon { outline, holes } => {
                let holes: f64 = holes.iter().map(|h| signed_area(h).abs()).sum();
                signed_area(outline).abs() - holes
            }
        }
    }

    /// The profile as polygons: the outline anticlockwise first, then each
    /// hole clockwise. Curves get as many segments as keep them within
    /// `tolerance` of the true curve.
    pub(crate) fn loops(&self, tolerance: f64) -> Result<Vec<Vec<[f64; 2]>>, GeometryError> {
        let invalid = |reason: &str| Err(GeometryError::InvalidProfile(reason.into()));
        match self {
            Self::Rectangle { width, height } => {
                if !(*width > 0.0 && *height > 0.0 && width.is_finite() && height.is_finite()) {
                    return invalid("a rectangle's sides must be positive");
                }
                let (x, y) = (width / 2.0, height / 2.0);
                Ok(vec![vec![[-x, -y], [x, -y], [x, y], [-x, y]]])
            }
            Self::Circle { radius } => {
                if !(*radius > 0.0 && radius.is_finite()) {
                    return invalid("a circle's radius must be positive");
                }
                let n = segments(*radius, tolerance);
                let point = |i: usize| {
                    let angle = 2.0 * PI * i as f64 / n as f64;
                    [radius * angle.cos(), radius * angle.sin()]
                };
                Ok(vec![(0..n).map(point).collect()])
            }
            Self::Polygon { outline, holes } => {
                let mut loops = vec![oriented(outline, true)?];
                for hole in holes {
                    loops.push(oriented(hole, false)?);
                }
                Ok(loops)
            }
        }
    }
}

/// How many segments keep a circle of `radius` within `tolerance` of the
/// true circle: the sagitta r(1 − cos(π/n)) is at most the tolerance.
fn segments(radius: f64, tolerance: f64) -> usize {
    let ratio = (1.0 - tolerance / radius).clamp(-1.0, 1.0);
    let n = (PI / ratio.acos()).ceil();
    if n.is_finite() {
        (n as usize).clamp(8, 256)
    } else {
        256
    }
}

/// A polygon's area by the shoelace formula, positive when anticlockwise.
fn signed_area(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    let twice: f64 = (0..n)
        .map(|i| {
            let ([x0, y0], [x1, y1]) = (points[i], points[(i + 1) % n]);
            x0 * y1 - x1 * y0
        })
        .sum();
    twice / 2.0
}

/// A polygon with the winding asked for, after checking it isn't
/// degenerate.
fn oriented(points: &[[f64; 2]], anticlockwise: bool) -> Result<Vec<[f64; 2]>, GeometryError> {
    let invalid = |reason: &str| Err(GeometryError::InvalidProfile(reason.into()));
    if points.len() < 3 {
        return invalid("a polygon needs at least 3 points");
    }
    if !points.iter().flatten().all(|c| c.is_finite()) {
        return invalid("a polygon's coordinates must be finite");
    }
    let area = signed_area(points);
    if area.abs() < 1e-12 {
        return invalid("a polygon must enclose an area");
    }
    let mut points = points.to_vec();
    if (area > 0.0) != anticlockwise {
        points.reverse();
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn areas_are_exact() {
        let circle = Profile::Circle { radius: 0.2 };
        assert_eq!(circle.area(), PI * 0.04);
        let hollow = Profile::Polygon {
            outline: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            holes: vec![vec![[0.25, 0.25], [0.25, 0.75], [0.75, 0.75], [0.75, 0.25]]],
        };
        assert_eq!(hollow.area(), 0.75);
    }

    #[test]
    fn a_20_cm_circle_gets_32_segments() {
        assert_eq!(segments(0.2, crate::CHORD_TOLERANCE), 32);
        assert_eq!(segments(1e-6, crate::CHORD_TOLERANCE), 8);
        assert_eq!(segments(1000.0, crate::CHORD_TOLERANCE), 256);
    }

    #[test]
    fn loops_are_wound_outline_anticlockwise_and_holes_clockwise() {
        let clockwise = vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
        let profile = Profile::Polygon {
            outline: clockwise.clone(),
            holes: vec![clockwise.iter().map(|[x, y]| [x / 2.0, y / 2.0]).collect()],
        };
        let loops = profile.loops(crate::CHORD_TOLERANCE).unwrap();
        assert!(signed_area(&loops[0]) > 0.0);
        assert!(signed_area(&loops[1]) < 0.0);
    }

    #[test]
    fn degenerate_profiles_are_refused() {
        let flat = Profile::polygon([[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]]);
        let empty = Profile::Rectangle {
            width: 0.0,
            height: 1.0,
        };
        let nan = Profile::Circle { radius: f64::NAN };
        for profile in [flat, empty, nan] {
            let error = profile.loops(crate::CHORD_TOLERANCE).unwrap_err();
            assert!(matches!(error, GeometryError::InvalidProfile(_)));
        }
    }
}
