//! Structural grids: labelled lines that lay out columns in plan (#34).
//!
//! A grid is a spatial location element: a straight line in plan, from a
//! start point to an end point, with a label such as `A` or `3`. It's IFC's
//! `IfcGridAxis`. Grids run through every level, so they have no elevation.
//! Coordinates are in metres from the project's origin.
//!
//! The placement tool labels new grids automatically with [`next_label`]
//! (A, B, C… or 1, 2, 3…), and users can rename them. Where two grids cross
//! is a snap point for placing elements ([`intersections`]).
//!
//! ```
//! use hito_core::{Element, ElementStore, Model, ModelKind, SchemaRegistry};
//! use hito_structural::{class, grid};
//!
//! # let mut registry = SchemaRegistry::new();
//! # hito_structural::register(&mut registry)?;
//! # let mut store = ElementStore::new(registry);
//! # let mut tx = store.transaction();
//! # let model = tx.insert_model(Model::new(ModelKind::Physical))?;
//! let a = grid::GridLine::new([0.0, 0.0], [0.0, 10.0]).element(model, "A");
//! let one = grid::GridLine::new([-1.0, 2.0], [8.0, 2.0]).element(model, "1");
//! tx.insert(a)?;
//! tx.insert(one)?;
//! tx.commit()?;
//!
//! let crossings = grid::intersections(&store.view());
//! assert_eq!(crossings[0].point, [0.0, 2.0]);
//! assert_eq!(grid::next_label("A"), Some("B".into()));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use hito_core::{
    ClassDef, ClassId, DataType, Dimension, Element, ElementId, ModelId, Rule, StoreView, Value,
};

use crate::class;

/// A grid line.
pub const GRID: &str = "Grid";

/// The grid's label, unique among grids, such as `A` or `3`. It's data, not
/// interface text, so it isn't translated.
pub const NAME: &str = "Name";
/// Where the line starts, in plan.
pub const START_X: &str = "StartX";
pub const START_Y: &str = "StartY";
/// Where the line ends, in plan. The label bubble is drawn at this end.
pub const END_X: &str = "EndX";
pub const END_Y: &str = "EndY";

const LENGTH: DataType = DataType::Quantity(Dimension::LENGTH);

/// How close, relative to the lines' lengths, two lines must come to count
/// as crossing, or as parallel. It absorbs rounding, not drawing errors.
const TOLERANCE: f64 = 1e-9;

/// The grid class.
pub(crate) fn classes() -> [ClassDef; 1] {
    let grid = ClassDef::new(GRID, ClassId::SPATIAL_LOCATION_ELEMENT)
        .property(NAME, DataType::Text)
        .property(START_X, LENGTH)
        .property(START_Y, LENGTH)
        .property(END_X, LENGTH)
        .property(END_Y, LENGTH);
    [grid]
}

/// The grid rules:
///
/// - `GridName`: every grid has a label, and no two grids share one, as in
///   Revit.
/// - `GridLine`: every grid has both ends, and they're different points.
pub(crate) fn rules() -> Vec<Rule> {
    let name = Rule::new("GridName", class(GRID), |store, grid| {
        let name = match grid.properties().get(NAME) {
            Some(Value::Text(name)) if !name.trim().is_empty() => name,
            _ => return Err("the grid has no label".into()),
        };
        match find(store, name).filter(|&other| other != grid.id()) {
            Some(other) => Err(format!("grid {other} is also labelled `{name}`")),
            None => Ok(()),
        }
    });

    let line = Rule::new("GridLine", class(GRID), |store, grid| {
        match GridLine::of(store, grid.id()) {
            None => Err("the grid is missing an end point".into()),
            Some(line) if line.length() == 0.0 => Err("the grid's ends are the same point".into()),
            Some(_) => Ok(()),
        }
    });

    vec![name, line]
}

/// A grid's line in plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridLine {
    pub start: [f64; 2],
    pub end: [f64; 2],
}

impl GridLine {
    pub fn new(start: [f64; 2], end: [f64; 2]) -> Self {
        Self { start, end }
    }

    /// The line of grid `id`, if it has both ends.
    pub fn of(store: &StoreView<'_>, id: ElementId) -> Option<Self> {
        let get = |name| match store.property(id, name) {
            Some(Value::Real(x)) => Some(*x),
            _ => None,
        };
        Some(Self {
            start: [get(START_X)?, get(START_Y)?],
            end: [get(END_X)?, get(END_Y)?],
        })
    }

    /// A new grid element along this line in `model`, labelled `name`.
    pub fn element(&self, model: ModelId, name: &str) -> Element {
        Element::new(class(GRID), model)
            .with_property(NAME, name)
            .with_property(START_X, self.start[0])
            .with_property(START_Y, self.start[1])
            .with_property(END_X, self.end[0])
            .with_property(END_Y, self.end[1])
    }

    pub fn length(&self) -> f64 {
        let [dx, dy] = self.direction();
        dx.hypot(dy)
    }

    /// Where this line crosses `other`, if it does. Ends that touch count;
    /// parallel and overlapping lines don't cross.
    pub fn intersection(&self, other: &GridLine) -> Option<[f64; 2]> {
        let (r, s) = (self.direction(), other.direction());
        let denominator = cross(r, s);
        if denominator.abs() <= TOLERANCE * self.length() * other.length() {
            return None;
        }
        let q = [
            other.start[0] - self.start[0],
            other.start[1] - self.start[1],
        ];
        let t = cross(q, s) / denominator;
        let u = cross(q, r) / denominator;
        let within = |x: f64| (-TOLERANCE..=1.0 + TOLERANCE).contains(&x);
        (within(t) && within(u)).then(|| [self.start[0] + t * r[0], self.start[1] + t * r[1]])
    }

    fn direction(&self) -> [f64; 2] {
        [self.end[0] - self.start[0], self.end[1] - self.start[1]]
    }
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

/// Where two grids cross: a snap point for placing elements.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridIntersection {
    pub grids: [ElementId; 2],
    pub point: [f64; 2],
}

/// Every point where two grids cross, each pair once.
pub fn intersections(store: &StoreView<'_>) -> Vec<GridIntersection> {
    let registry = store.registry();
    let mut grids: Vec<_> = store
        .elements()
        .filter(|e| registry.is_a(e.class(), &class(GRID)))
        .filter_map(|e| Some((e.id(), GridLine::of(store, e.id())?)))
        .collect();
    grids.sort_by_key(|(id, _)| *id);
    let mut crossings = Vec::new();
    for (i, (a, line_a)) in grids.iter().enumerate() {
        for (b, line_b) in &grids[i + 1..] {
            if let Some(point) = line_a.intersection(line_b) {
                crossings.push(GridIntersection {
                    grids: [*a, *b],
                    point,
                });
            }
        }
    }
    crossings
}

/// The grid labelled `name`, if any.
pub fn find(store: &StoreView<'_>, name: &str) -> Option<ElementId> {
    let registry = store.registry();
    let name = Value::Text(name.to_owned());
    store
        .elements()
        .find(|e| registry.is_a(e.class(), &class(GRID)) && e.properties().get(NAME) == Some(&name))
        .map(Element::id)
}

/// The label after `label`, as the placement tool numbers grids: the last
/// run of digits counts up (`1` → `2`, `9` → `10`, `A09` → `A10`), or else
/// the last run of letters does, as columns in a spreadsheet (`A` → `B`,
/// `Z` → `AA`, `b` → `c`). Anything before the run is kept. `None` if the
/// label ends in neither, such as `A'`.
pub fn next_label(label: &str) -> Option<String> {
    let last = label.chars().last()?;
    let in_run = |c: char| match last.is_ascii_digit() {
        true => c.is_ascii_digit(),
        false => c.is_ascii_alphabetic() && c.is_ascii_uppercase() == last.is_ascii_uppercase(),
    };
    if !in_run(last) {
        return None;
    }
    let start = label
        .char_indices()
        .rev()
        .take_while(|&(_, c)| in_run(c))
        .last()
        .map_or(0, |(i, _)| i);
    let (prefix, run) = label.split_at(start);

    // Count up from the right, carrying: 9 → 0 and Z → A, then a new digit
    // or letter in front if everything carried.
    let (first, last_char) = match last.is_ascii_digit() {
        true => (b'0', b'9'),
        false if last.is_ascii_uppercase() => (b'A', b'Z'),
        false => (b'a', b'z'),
    };
    let mut bytes = run.as_bytes().to_vec();
    let mut carried = true;
    for byte in bytes.iter_mut().rev() {
        if *byte == last_char {
            *byte = first;
        } else {
            *byte += 1;
            carried = false;
            break;
        }
    }
    if carried {
        // 99 → 100, but Z → AA.
        let new = if first == b'0' { b'1' } else { first };
        bytes.insert(0, new);
    }
    let run = String::from_utf8(bytes).expect("ASCII");
    Some(format!("{prefix}{run}"))
}

/// The first label after `label` that no grid has, for a new grid placed
/// after the one labelled `label`.
pub fn next_free_label(store: &StoreView<'_>, label: &str) -> Option<String> {
    let mut next = next_label(label)?;
    while find(store, &next).is_some() {
        next = next_label(&next)?;
    }
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_count_up_like_revit() {
        let next = |l: &str| next_label(l);
        assert_eq!(next("A").as_deref(), Some("B"));
        assert_eq!(next("H").as_deref(), Some("I"));
        assert_eq!(next("Z").as_deref(), Some("AA"));
        assert_eq!(next("AZ").as_deref(), Some("BA"));
        assert_eq!(next("ZZ").as_deref(), Some("AAA"));
        assert_eq!(next("b").as_deref(), Some("c"));
        assert_eq!(next("1").as_deref(), Some("2"));
        assert_eq!(next("9").as_deref(), Some("10"));
        assert_eq!(next("99").as_deref(), Some("100"));
        assert_eq!(next("09").as_deref(), Some("10"));
        assert_eq!(next("A1").as_deref(), Some("A2"));
        assert_eq!(next("C-09").as_deref(), Some("C-10"));
        assert_eq!(next("1a").as_deref(), Some("1b"));
        // Only the run in the last character's case counts.
        assert_eq!(next("Ab").as_deref(), Some("Ac"));
        assert_eq!(next("aZ").as_deref(), Some("aAA"));
        assert_eq!(next("A'"), None);
        assert_eq!(next("Ñ"), None);
        assert_eq!(next(""), None);
    }

    #[test]
    fn crossing_lines_meet_at_one_point() {
        let a = GridLine::new([0.0, 0.0], [0.0, 10.0]);
        let one = GridLine::new([-2.0, 4.0], [6.0, 4.0]);
        assert_eq!(a.intersection(&one), Some([0.0, 4.0]));
        assert_eq!(one.intersection(&a), Some([0.0, 4.0]));

        // Diagonal grids.
        let d1 = GridLine::new([0.0, 0.0], [4.0, 4.0]);
        let d2 = GridLine::new([0.0, 4.0], [4.0, 0.0]);
        assert_eq!(d1.intersection(&d2), Some([2.0, 2.0]));
    }

    #[test]
    fn touching_ends_cross_but_parallel_and_short_lines_dont() {
        let a = GridLine::new([0.0, 0.0], [0.0, 10.0]);
        let ends_on_a = GridLine::new([0.0, 10.0], [5.0, 10.0]);
        assert_eq!(a.intersection(&ends_on_a), Some([0.0, 10.0]));

        let b = GridLine::new([6.0, 0.0], [6.0, 10.0]);
        assert_eq!(a.intersection(&b), None);
        let overlapping = GridLine::new([0.0, 5.0], [0.0, 15.0]);
        assert_eq!(a.intersection(&overlapping), None);

        let short = GridLine::new([1.0, 5.0], [5.0, 5.0]);
        assert_eq!(a.intersection(&short), None);
    }
}
