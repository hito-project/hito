//! Beams: horizontal members drawn on a level, between columns or points
//! (#38).
//!
//! A beam is a level-based element (see [`level`](crate::level)) drawn as a
//! straight line in plan, from [`START_X`], [`START_Y`] to [`END_X`],
//! [`END_Y`]. As Revit draws beams by default, the line is the beam's
//! **top**, at its level raised by the level offset: its
//! [`BASE_ELEVATION`](crate::level::BASE_ELEVATION). The section hangs below
//! it, centred across it.
//!
//! Its type is a [`BEAM_TYPE`], a [section type](crate::section): a
//! rectangle, `Width` across and `Depth` deep, and a material.
//!
//! Each end can **frame into a column**, through [`START_COLUMN`] and
//! [`END_COLUMN`]. The column drives the beam: that end is recomputed as the
//! column's centre, so the beam follows the column when it moves, and so
//! when the column follows its grids. The beam's body stops at the column's
//! face, whatever the column's shape and rotation, so the two don't overlap.
//! Deleting the column leaves the beam where it was.
//!
//! ```
//! use hito_core::{Element, ElementStore, Model, ModelKind, SchemaRegistry};
//! use hito_structural::beam::{self, BEAM_TYPE, End};
//! use hito_structural::column::{self, COLUMN_TYPE, Placement};
//! use hito_structural::section::{self, Section};
//! use hito_structural::{class, level};
//!
//! # let mut registry = SchemaRegistry::new();
//! # hito_structural::register(&mut registry)?;
//! # let mut store = ElementStore::new(registry);
//! let mut tx = store.transaction();
//! let model = tx.insert_model(Model::new(ModelKind::Physical))?;
//! let ground = Element::new(class(level::LEVEL), model)
//!     .with_property(level::NAME, "Planta baja")
//!     .with_property(level::ELEVATION, 0.0);
//! let ground = tx.insert(ground)?;
//! let square = Section::Rectangle { width: 0.3, depth: 0.3 };
//! let c30 = section::insert(&mut tx, model, COLUMN_TYPE, "C 30 x 30", square, None)?;
//! let left = column::insert(&mut tx, model, &Placement::new(c30, ground, [0.0, 0.0]))?;
//! let right = column::insert(&mut tx, model, &Placement::new(c30, ground, [5.0, 0.0]))?;
//! let v20 = Section::Rectangle { width: 0.2, depth: 0.5 };
//! let v20 = section::insert(&mut tx, model, BEAM_TYPE, "V 20 x 50", v20, None)?;
//! let placement = beam::Placement::new(v20, ground, End::Column(left), End::Column(right));
//! let beam = beam::insert(&mut tx, model, &placement)?;
//! tx.commit()?;
//! assert_eq!(beam::line(&store.view(), beam), Some(([0.0, 0.0], [5.0, 0.0])));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use hito_core::{
    ClassDef, DataType, Dimension, Element, ElementId, ModelId, Properties, Recompute,
    Relationship, RelationshipDef, Rule, StoreError, StoreView, Transaction,
};

use crate::class;
use crate::column::{self, COLUMN};
use crate::level::{ATTACHED_TO_LEVEL, ELEVATION, LEVEL_BASED_ELEMENT, LEVEL_OFFSET};
use crate::read::Read;
use crate::section::{self, SECTION_TYPE, Section, Shape};

/// A beam.
pub const BEAM: &str = "Beam";
/// A beam type: a rectangular [section type](crate::section).
pub const BEAM_TYPE: &str = "BeamType";
/// From a beam to the column its start frames into, which drives it.
pub const START_COLUMN: &str = "StartFramesInto";
/// From a beam to the column its end frames into, which drives it.
pub const END_COLUMN: &str = "EndFramesInto";

/// The beam's line in plan, along its top.
pub const START_X: &str = "StartX";
pub const START_Y: &str = "StartY";
pub const END_X: &str = "EndX";
pub const END_Y: &str = "EndY";

/// How close two points must be to count as one: 1 mm. A beam must be
/// longer, and so must what's left of it between the columns' faces.
pub const TOLERANCE: f64 = 0.001;

const LENGTH: DataType = DataType::Quantity(Dimension::LENGTH);

/// Where one end of a beam is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum End {
    /// At this point in plan.
    Point([f64; 2]),
    /// Framing into this column, at its centre.
    Column(ElementId),
}

/// A beam to add with [`insert`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub beam_type: ElementId,
    pub level: ElementId,
    /// How far the beam's top is above its level. Negative is below.
    pub offset: f64,
    pub start: End,
    pub end: End,
}

impl Placement {
    /// A beam of `beam_type` from `start` to `end`, its top at `level`.
    pub fn new(beam_type: ElementId, level: ElementId, start: End, end: End) -> Self {
        Self {
            beam_type,
            level,
            offset: 0.0,
            start,
            end,
        }
    }

    pub fn with_offset(mut self, offset: f64) -> Self {
        self.offset = offset;
        self
    }
}

/// Adds a beam to `model`. Returns the beam.
pub fn insert(
    tx: &mut Transaction<'_>,
    model: ModelId,
    placement: &Placement,
) -> Result<ElementId, StoreError> {
    let p = placement;
    let view = tx.view();
    let point = |end: End| match end {
        End::Point(point) => Some(point),
        End::Column(column) => column::location(&view, column),
    };
    // A missing column leaves the end unset, which the rules report.
    let mut element = Element::new(class(BEAM), model)
        .with_type(p.beam_type)
        .with_property(LEVEL_OFFSET, p.offset);
    for (end, [x_name, y_name]) in [(p.start, [START_X, START_Y]), (p.end, [END_X, END_Y])] {
        if let Some([x, y]) = point(end) {
            element = element.with_property(x_name, x).with_property(y_name, y);
        }
    }
    let beam = tx.insert(element)?;
    tx.relate(Relationship::new(class(ATTACHED_TO_LEVEL), beam, p.level))?;
    for (end, name) in [(p.start, START_COLUMN), (p.end, END_COLUMN)] {
        if let End::Column(column) = end {
            tx.relate(Relationship::new(class(name), beam, column))?;
        }
    }
    Ok(beam)
}

/// The beam's line in plan, from its start to its end.
pub fn line(store: &StoreView<'_>, beam: ElementId) -> Option<([f64; 2], [f64; 2])> {
    read_line(store, beam)
}

/// The columns the beam's start and end frame into.
pub fn columns(store: &StoreView<'_>, beam: ElementId) -> [Option<ElementId>; 2] {
    [START_COLUMN, END_COLUMN].map(|name| store.targets(beam, &class(name)).first().copied())
}

/// The column a beam end placed at `point`, with its top at `elevation`,
/// frames into: one whose section contains the point and that reaches the
/// beam, from below it up to it or past it. The oldest, if several do.
pub fn column_at(store: &StoreView<'_>, point: [f64; 2], elevation: f64) -> Option<ElementId> {
    column::columns(store).into_iter().find(|&c| {
        let reaches = column::read_elevations(store, c)
            .is_some_and(|(bottom, top)| bottom < elevation && elevation <= top + TOLERANCE);
        let inside = column::read_section(store, c)
            .zip(column::to_section(store, c, point))
            .is_some_and(|(section, local)| section.contains(local));
        reaches && inside
    })
}

/// The beam classes.
pub(crate) fn classes() -> [ClassDef; 2] {
    let beam_type = ClassDef::new(BEAM_TYPE, class(SECTION_TYPE));
    let beam = ClassDef::new(BEAM, class(LEVEL_BASED_ELEMENT))
        .typed_by(class(BEAM_TYPE))
        .property(START_X, LENGTH)
        .property(START_Y, LENGTH)
        .property(END_X, LENGTH)
        .property(END_Y, LENGTH);
    [beam_type, beam]
}

pub(crate) fn relationships() -> [RelationshipDef; 2] {
    let frames = |name| RelationshipDef::new(name, class(BEAM), class(COLUMN)).driving();
    [frames(START_COLUMN), frames(END_COLUMN)]
}

/// A beam's ends follow the columns they frame into.
pub(crate) fn recomputes() -> Vec<Recompute> {
    let follow = Recompute::new(class(BEAM), |store, beam| {
        let mut values = Properties::new();
        let ends = [
            (START_COLUMN, [START_X, START_Y]),
            (END_COLUMN, [END_X, END_Y]),
        ];
        for (relationship, [x_name, y_name]) in ends {
            let column = store
                .targets(beam.id(), &class(relationship))
                .first()
                .copied();
            if let Some([x, y]) = column.and_then(|c| column::read_location(store, c)) {
                values.insert(x_name.into(), x.into());
                values.insert(y_name.into(), y.into());
            }
        }
        values
    });
    vec![follow]
}

/// The beam rules:
///
/// - `BeamType`: the beam has a beam type, and beam types are rectangular.
/// - `BeamLine`: the beam has both ends, more than 1 mm apart.
/// - `BeamEnds`: each end frames into at most one column, and not both
///   into the same one.
pub(crate) fn rules() -> Vec<Rule> {
    // The core already refuses a type of another class.
    let beam_type = Rule::new("BeamType", class(BEAM), |_, beam| match beam.type_id() {
        Some(_) => Ok(()),
        None => Err("the beam has no type".into()),
    });

    let rectangular = Rule::new(
        "BeamSection",
        class(BEAM_TYPE),
        |store, beam_type| match store
            .text(beam_type.id(), section::SHAPE)
            .and_then(Shape::from_name)
        {
            Some(Shape::Circular) => Err("a beam's section must be rectangular".into()),
            _ => Ok(()),
        },
    );

    let line = Rule::new("BeamLine", class(BEAM), |store, beam| {
        match read_line(store, beam.id()) {
            None => Err("the beam is missing an end point".into()),
            Some((start, end)) if distance(start, end) <= TOLERANCE => {
                Err("the beam is shorter than 1 mm".into())
            }
            Some(_) => Ok(()),
        }
    });

    let ends = Rule::new("BeamEnds", class(BEAM), |store, beam| {
        let mut columns = Vec::new();
        for name in [START_COLUMN, END_COLUMN] {
            let targets = store.targets(beam.id(), &class(name));
            if targets.len() > 1 {
                return Err(format!("the beam has {} `{name}` columns", targets.len()));
            }
            columns.extend(targets);
        }
        match columns[..] {
            [a, b] if a == b => Err(format!("both ends frame into column {a}")),
            _ => Ok(()),
        }
    });

    vec![beam_type, rectangular, line, ends]
}

pub(crate) fn read_line<'a>(
    store: &impl Read<'a>,
    beam: ElementId,
) -> Option<([f64; 2], [f64; 2])> {
    let get = |name| store.real(beam, name);
    Some(([get(START_X)?, get(START_Y)?], [get(END_X)?, get(END_Y)?]))
}

/// The beam's top, read from its level and offset rather than the
/// recomputed base elevation, so it's right before a transaction commits.
pub(crate) fn read_top<'a>(store: &impl Read<'a>, beam: ElementId) -> Option<f64> {
    let level = *store.targets(beam, &class(ATTACHED_TO_LEVEL)).first()?;
    let elevation = store.real(level, ELEVATION).unwrap_or(0.0);
    Some(elevation + store.real(beam, LEVEL_OFFSET).unwrap_or(0.0))
}

pub(crate) fn read_section<'a>(store: &impl Read<'a>, beam: ElementId) -> Option<Section> {
    section::read_section(store, store.type_of(beam)?)
}

/// The beam's body in plan, from start to end: its line, with each end
/// that frames into a column moved back to the column's face.
pub(crate) fn read_body_line<'a>(
    store: &impl Read<'a>,
    beam: ElementId,
) -> Option<([f64; 2], [f64; 2])> {
    let (start, end) = read_line(store, beam)?;
    let length = distance(start, end);
    if length <= TOLERANCE {
        return None;
    }
    let along = [(end[0] - start[0]) / length, (end[1] - start[1]) / length];
    // How far the column at an end reaches along the beam, towards the
    // other end.
    let reach = |name, direction: [f64; 2]| {
        let column = *store.targets(beam, &class(name)).first()?;
        let section = column::read_section(store, column)?;
        let (sin, cos) = column::read_rotation(store, column).sin_cos();
        let [dx, dy] = direction;
        Some(section.reach([dx * cos + dy * sin, -dx * sin + dy * cos]))
    };
    let from_start = reach(START_COLUMN, along).unwrap_or(0.0);
    let from_end = reach(END_COLUMN, [-along[0], -along[1]]).unwrap_or(0.0);
    if length - from_start - from_end <= TOLERANCE {
        return None;
    }
    let at = |t: f64| [start[0] + along[0] * t, start[1] + along[1] * t];
    Some((at(from_start), at(length - from_end)))
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}
