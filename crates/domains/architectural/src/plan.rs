//! Plan geometry: the few 2D operations wall joins need.
//!
//! Every layer of a joined wall is the band between two of its layer lines,
//! cut by straight lines at its ends and around other walls. Each cut keeps
//! one side of a line, so every piece stays a convex polygon and a
//! half-plane clip is all the polygon arithmetic needed. Points are in
//! metres, in plan.

/// A point or vector in plan.
pub type Point = [f64; 2];

/// How close two directions must be, as the sine of the angle between them,
/// to count as parallel: about 0.6°.
pub(crate) const PARALLEL: f64 = 0.01;

pub(crate) fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}

pub(crate) fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}

pub(crate) fn scale(a: Point, s: f64) -> Point {
    [a[0] * s, a[1] * s]
}

pub(crate) fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// The z component of the cross product: positive when `b` turns
/// anticlockwise from `a`.
pub(crate) fn cross(a: Point, b: Point) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

pub(crate) fn length(a: Point) -> f64 {
    a[0].hypot(a[1])
}

pub(crate) fn distance(a: Point, b: Point) -> f64 {
    length(sub(a, b))
}

/// `a` turned a quarter turn anticlockwise: the normal on its left.
pub(crate) fn left(a: Point) -> Point {
    [-a[1], a[0]]
}

/// A straight line through `point` along the unit vector `direction`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Line {
    pub point: Point,
    pub direction: Point,
}

impl Line {
    /// Where this line meets `other`, or `None` if they're parallel.
    pub fn intersection(&self, other: &Line) -> Option<Point> {
        let denominator = cross(self.direction, other.direction);
        if denominator.abs() < PARALLEL {
            return None;
        }
        let t = cross(sub(other.point, self.point), other.direction) / denominator;
        Some(add(self.point, scale(self.direction, t)))
    }
}

/// The points on one side of a line: those `x` with
/// `dot(x - point, normal) >= 0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HalfPlane {
    pub point: Point,
    pub normal: Point,
}

impl HalfPlane {
    /// The side of the line through `a` and `b` that `inside` is on.
    pub fn through(a: Point, b: Point, inside: Point) -> Self {
        let normal = left(sub(b, a));
        let normal = match dot(sub(inside, a), normal) < 0.0 {
            true => scale(normal, -1.0),
            false => normal,
        };
        Self { point: a, normal }
    }

    fn distance(&self, x: Point) -> f64 {
        dot(sub(x, self.point), self.normal)
    }
}

/// The part of a convex polygon on the inner side of `plane`
/// (Sutherland–Hodgman). Empty if nothing is left.
pub(crate) fn clip(polygon: &[Point], plane: &HalfPlane) -> Vec<Point> {
    let mut out = Vec::with_capacity(polygon.len() + 1);
    for (i, &a) in polygon.iter().enumerate() {
        let b = polygon[(i + 1) % polygon.len()];
        let (da, db) = (plane.distance(a), plane.distance(b));
        if da >= 0.0 {
            out.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            let t = da / (da - db);
            out.push(add(a, scale(sub(b, a), t)));
        }
    }
    if area(&out) <= MIN_AREA {
        Vec::new()
    } else {
        out
    }
}

/// Pieces smaller than this, in m², are rounding left-overs and dropped:
/// a square of 0.01 mm.
pub(crate) const MIN_AREA: f64 = 1e-10;

/// A polygon's area, by the shoelace formula. Positive whichever the
/// winding.
pub fn area(polygon: &[Point]) -> f64 {
    let n = polygon.len();
    if n < 3 {
        return 0.0;
    }
    let origin = polygon[0];
    let twice: f64 = (1..n - 1)
        .map(|i| cross(sub(polygon[i], origin), sub(polygon[i + 1], origin)))
        .sum();
    (twice / 2.0).abs()
}

/// The area two convex polygons share.
pub fn overlap(a: &[Point], b: &[Point]) -> f64 {
    let mut shared = a.to_vec();
    for (i, &p) in b.iter().enumerate() {
        let q = b[(i + 1) % b.len()];
        // Anticlockwise or not, the polygon's inside is on the side of the
        // centroid.
        let inside = centroid(b);
        shared = clip(&shared, &HalfPlane::through(p, q, inside));
        if shared.is_empty() {
            return 0.0;
        }
    }
    area(&shared)
}

fn centroid(polygon: &[Point]) -> Point {
    let sum = polygon.iter().fold([0.0, 0.0], |s, &p| add(s, p));
    scale(sum, 1.0 / polygon.len() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE: [Point; 4] = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];

    #[test]
    fn clipping_keeps_one_side() {
        let right = HalfPlane {
            point: [1.0, 0.0],
            normal: [1.0, 0.0],
        };
        let half = clip(&SQUARE, &right);
        assert_eq!(area(&half), 2.0);
        let left = HalfPlane {
            point: [1.0, 0.0],
            normal: [-1.0, 0.0],
        };
        assert_eq!(area(&clip(&SQUARE, &left)), 2.0);

        let diagonal = HalfPlane::through([0.0, 0.0], [2.0, 2.0], [2.0, 0.0]);
        assert_eq!(area(&clip(&SQUARE, &diagonal)), 2.0);

        let outside = HalfPlane {
            point: [3.0, 0.0],
            normal: [1.0, 0.0],
        };
        assert!(clip(&SQUARE, &outside).is_empty());
    }

    #[test]
    fn lines_meet_unless_parallel() {
        let x = Line {
            point: [0.0, 1.0],
            direction: [1.0, 0.0],
        };
        let y = Line {
            point: [3.0, -5.0],
            direction: [0.0, 1.0],
        };
        assert_eq!(x.intersection(&y), Some([3.0, 1.0]));
        let parallel = Line {
            point: [0.0, 4.0],
            direction: [-1.0, 0.0],
        };
        assert_eq!(x.intersection(&parallel), None);
    }

    #[test]
    fn overlaps_of_convex_polygons() {
        let shifted = SQUARE.map(|p| add(p, [1.0, 1.0]));
        assert_eq!(overlap(&SQUARE, &shifted), 1.0);
        let touching = SQUARE.map(|p| add(p, [2.0, 0.0]));
        assert_eq!(overlap(&SQUARE, &touching), 0.0);
    }
}
