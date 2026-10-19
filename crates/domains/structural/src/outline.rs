//! Outlines: closed polygons in plan, such as a slab's boundary or an
//! opening in it (#39).
//!
//! An outline is its corners in order, either winding, without repeating
//! the first corner at the end. A valid outline is a simple polygon: at
//! least three corners, every corner more than [`TOLERANCE`] from every
//! edge it isn't on, so no edge crosses or touches another, and an area.
//! Points are in metres, in plan.
//!
//! ```
//! use hito_structural::outline::{self, Problem};
//!
//! let square = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
//! let hole = [[1.0, 1.0], [2.0, 1.0], [2.0, 2.0], [1.0, 2.0]];
//! assert_eq!(outline::check(&square), Ok(()));
//! assert!(outline::inside(&hole, &square));
//! let bow_tie = [[0.0, 0.0], [4.0, 4.0], [4.0, 0.0], [0.0, 4.0]];
//! assert_eq!(outline::check(&bow_tie), Err(Problem::Crossing));
//! ```
//!
//! Slab outlines have tens of corners, so the checks compare every edge
//! with every other, which is simple and fast enough. The `geo` crate's
//! `Validation` does the same checks, but would bring a dozen crates,
//! geodesy among them, for these few functions.

/// A point in plan.
pub type Point = [f64; 2];

/// How close two points, or a point and an edge, must be to count as
/// touching: 1 mm, as wall ends.
pub const TOLERANCE: f64 = 0.001;

/// Why an outline isn't valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// It has fewer than three corners.
    TooFewCorners,
    /// A coordinate isn't a finite number.
    NotFinite,
    /// Two corners are the same point, or a corner lies on another edge.
    Touching,
    /// Two edges cross.
    Crossing,
    /// It encloses no area.
    NoArea,
}

impl Problem {
    /// What's wrong, for a rule's message.
    pub fn message(self) -> &'static str {
        match self {
            Self::TooFewCorners => "the outline has fewer than 3 corners",
            Self::NotFinite => "the outline has a coordinate that isn't a number",
            Self::Touching => "the outline touches itself",
            Self::Crossing => "the outline crosses itself",
            Self::NoArea => "the outline encloses no area",
        }
    }
}

/// Whether `outline` is a simple polygon.
pub fn check(outline: &[Point]) -> Result<(), Problem> {
    if outline.len() < 3 {
        return Err(Problem::TooFewCorners);
    }
    check_edges(outline, true)?;
    if area(outline) <= TOLERANCE * TOLERANCE {
        return Err(Problem::NoArea);
    }
    Ok(())
}

/// Whether the corners drawn so far, not yet closed, can still become a
/// valid outline: no edge crosses or touches another. A tool checks each
/// new corner with it, so a mistake is refused as it's made.
pub fn check_open(corners: &[Point]) -> Result<(), Problem> {
    check_edges(corners, false)
}

/// Whether no edge crosses or touches another, the closing edge from the
/// last corner to the first included if `closed`.
fn check_edges(corners: &[Point], closed: bool) -> Result<(), Problem> {
    if !corners.iter().flatten().all(|c| c.is_finite()) {
        return Err(Problem::NotFinite);
    }
    let n = corners.len();
    let edges = if closed { n } else { n.saturating_sub(1) };
    for i in 0..edges {
        let edge = (corners[i], corners[(i + 1) % n]);
        for j in i + 1..edges {
            let other = (corners[j], corners[(j + 1) % n]);
            let adjacent = j == i + 1 || (closed && i == 0 && j == n - 1);
            if adjacent {
                // They share a corner, so only their far ends can touch the
                // other edge: an edge that folds back on the last one.
                let (shared, far, other_far) = if j == i + 1 {
                    (edge.1, edge.0, other.1)
                } else {
                    (edge.0, edge.1, other.0)
                };
                if distance(shared, far) <= TOLERANCE
                    || point_segment(far, (shared, other_far)) <= TOLERANCE
                    || point_segment(other_far, (shared, far)) <= TOLERANCE
                {
                    return Err(Problem::Touching);
                }
            } else if crosses(edge, other) {
                return Err(Problem::Crossing);
            } else if segment_segment(edge, other) <= TOLERANCE {
                return Err(Problem::Touching);
            }
        }
    }
    // A single edge must have length too.
    if n == 2 && distance(corners[0], corners[1]) <= TOLERANCE {
        return Err(Problem::Touching);
    }
    Ok(())
}

/// The area `outline` encloses, by the shoelace formula.
pub fn area(outline: &[Point]) -> f64 {
    signed_area(outline).abs()
}

/// The area, positive when the corners go anticlockwise.
pub fn signed_area(outline: &[Point]) -> f64 {
    let n = outline.len();
    let twice: f64 = (0..n)
        .map(|i| {
            let ([x0, y0], [x1, y1]) = (outline[i], outline[(i + 1) % n]);
            x0 * y1 - x1 * y0
        })
        .sum();
    twice / 2.0
}

/// Whether `point` is inside `outline`, more than [`TOLERANCE`] from its
/// edges.
pub fn contains(outline: &[Point], point: Point) -> bool {
    let n = outline.len();
    if edges(outline).any(|e| point_segment(point, e) <= TOLERANCE) {
        return false;
    }
    // Crossings of a ray from the point towards +x.
    let mut inside = false;
    for i in 0..n {
        let ([x0, y0], [x1, y1]) = (outline[i], outline[(i + 1) % n]);
        if (y0 > point[1]) != (y1 > point[1]) {
            let x = x0 + (point[1] - y0) / (y1 - y0) * (x1 - x0);
            if x > point[0] {
                inside = !inside;
            }
        }
    }
    inside
}

/// Whether `inner` lies inside `outer` without touching it: an opening
/// in a slab.
pub fn inside(inner: &[Point], outer: &[Point]) -> bool {
    !inner.is_empty() && apart(inner, outer) && contains(outer, inner[0])
}

/// Whether `a` and `b` neither overlap nor touch: two openings in a slab.
pub fn disjoint(a: &[Point], b: &[Point]) -> bool {
    match (a.first(), b.first()) {
        (Some(&first_a), Some(&first_b)) => {
            apart(a, b) && !contains(b, first_a) && !contains(a, first_b)
        }
        _ => true,
    }
}

/// Whether no edge of `a` comes within [`TOLERANCE`] of an edge of `b`, so
/// one is inside the other or they're apart.
fn apart(a: &[Point], b: &[Point]) -> bool {
    edges(a).all(|e| edges(b).all(|f| segment_segment(e, f) > TOLERANCE))
}

type Segment = (Point, Point);

fn edges(outline: &[Point]) -> impl Iterator<Item = Segment> + '_ {
    let n = outline.len();
    (0..n).map(move |i| (outline[i], outline[(i + 1) % n]))
}

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: Point, b: Point) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn distance(a: Point, b: Point) -> f64 {
    let d = sub(a, b);
    d[0].hypot(d[1])
}

/// How far `point` is from the segment.
fn point_segment(point: Point, (a, b): Segment) -> f64 {
    let ab = sub(b, a);
    let length2 = dot(ab, ab);
    if length2 == 0.0 {
        return distance(point, a);
    }
    let t = (dot(sub(point, a), ab) / length2).clamp(0.0, 1.0);
    distance(point, [a[0] + ab[0] * t, a[1] + ab[1] * t])
}

/// Whether the segments cross at a point inside both.
fn crosses((a, b): Segment, (c, d): Segment) -> bool {
    let side = |p: Point, q: Point, r: Point| cross(sub(q, p), sub(r, p));
    let (d1, d2) = (side(c, d, a), side(c, d, b));
    let (d3, d4) = (side(a, b, c), side(a, b, d));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

/// How far apart the segments are: zero if they cross.
fn segment_segment(e: Segment, f: Segment) -> f64 {
    if crosses(e, f) {
        return 0.0;
    }
    [
        point_segment(e.0, f),
        point_segment(e.1, f),
        point_segment(f.0, e),
        point_segment(f.1, e),
    ]
    .into_iter()
    .fold(f64::INFINITY, f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE: [Point; 4] = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];

    #[test]
    fn simple_polygons_pass_either_winding() {
        assert_eq!(check(&SQUARE), Ok(()));
        let mut clockwise = SQUARE;
        clockwise.reverse();
        assert_eq!(check(&clockwise), Ok(()));
        // An L, not convex.
        let l = [
            [0.0, 0.0],
            [6.0, 0.0],
            [6.0, 2.0],
            [2.0, 2.0],
            [2.0, 5.0],
            [0.0, 5.0],
        ];
        assert_eq!(check(&l), Ok(()));
        assert_eq!(area(&l), 12.0 + 6.0);
    }

    #[test]
    fn broken_outlines_say_why() {
        assert_eq!(
            check(&[[0.0, 0.0], [1.0, 0.0]]),
            Err(Problem::TooFewCorners)
        );
        let nan = [[0.0, 0.0], [1.0, f64::NAN], [1.0, 1.0]];
        assert_eq!(check(&nan), Err(Problem::NotFinite));
        let repeated = [[0.0, 0.0], [4.0, 0.0], [4.0, 0.0], [0.0, 4.0]];
        assert_eq!(check(&repeated), Err(Problem::Touching));
        let bow_tie = [[0.0, 0.0], [4.0, 4.0], [4.0, 0.0], [0.0, 4.0]];
        assert_eq!(check(&bow_tie), Err(Problem::Crossing));
        // A corner on a far edge.
        let pinched = [
            [0.0, 0.0],
            [4.0, 0.0],
            [4.0, 4.0],
            [2.0, 0.0005],
            [0.0, 4.0],
        ];
        assert_eq!(check(&pinched), Err(Problem::Touching));
        // An edge that folds back on the last one.
        let folded = [[0.0, 0.0], [4.0, 0.0], [2.0, 0.0], [2.0, 3.0]];
        assert_eq!(check(&folded), Err(Problem::Touching));
        let flat = [[0.0, 0.0], [2.0, 0.0], [4.0, 0.0]];
        assert!(check(&flat).is_err());
    }

    #[test]
    fn an_open_chain_is_checked_without_its_closing_edge() {
        // Closing it would cross, but drawn so far it's fine.
        let z = [[0.0, 0.0], [4.0, 0.0], [0.0, 4.0], [4.0, 4.0]];
        assert_eq!(check_open(&z), Ok(()));
        assert_eq!(check(&z), Err(Problem::Crossing));
        let crossing = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [2.0, -1.0]];
        assert_eq!(check_open(&crossing), Err(Problem::Crossing));
        assert_eq!(
            check_open(&[[1.0, 1.0], [1.0, 1.0]]),
            Err(Problem::Touching)
        );
        assert_eq!(check_open(&[[1.0, 1.0]]), Ok(()));
    }

    #[test]
    fn openings_go_strictly_inside() {
        let hole = [[1.0, 1.0], [2.0, 1.0], [2.0, 2.0], [1.0, 2.0]];
        assert!(inside(&hole, &SQUARE));
        assert!(!inside(&SQUARE, &hole));
        let on_edge = [[0.0, 1.0], [1.0, 1.0], [1.0, 2.0], [0.0, 2.0]];
        assert!(!inside(&on_edge, &SQUARE));
        let across = [[3.0, 1.0], [5.0, 1.0], [5.0, 2.0], [3.0, 2.0]];
        assert!(!inside(&across, &SQUARE));
        let outside = [[5.0, 1.0], [6.0, 1.0], [6.0, 2.0]];
        assert!(!inside(&outside, &SQUARE));
    }

    #[test]
    fn openings_may_not_overlap_or_nest() {
        let a = [[1.0, 1.0], [2.0, 1.0], [2.0, 2.0], [1.0, 2.0]];
        let b = [[2.5, 1.0], [3.0, 1.0], [3.0, 2.0], [2.5, 2.0]];
        assert!(disjoint(&a, &b));
        let touching = [[2.0, 1.0], [3.0, 1.0], [3.0, 2.0], [2.0, 2.0]];
        assert!(!disjoint(&a, &touching));
        let nested = [[1.2, 1.2], [1.8, 1.2], [1.8, 1.8]];
        assert!(!disjoint(&a, &nested));
        assert!(!disjoint(&nested, &a));
    }

    #[test]
    fn points_on_an_edge_are_not_inside() {
        assert!(contains(&SQUARE, [2.0, 2.0]));
        assert!(!contains(&SQUARE, [4.0, 2.0]));
        assert!(!contains(&SQUARE, [5.0, 2.0]));
        assert!(!contains(&SQUARE, [0.0, 0.0]));
    }
}
