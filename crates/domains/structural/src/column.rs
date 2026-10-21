//! Columns: vertical members from a base level to a top level (#37).
//!
//! A column is a level-based element (see [`level`](crate::level)): it
//! stands on its base level, raised by the level offset (Revit's "Base
//! Offset"). Its top is one of:
//!
//! - **a top level**, through [`TOP_LEVEL`], raised by [`TOP_OFFSET`]
//!   (Revit's "Top Level" and "Top Offset")
//! - **unconnected**: [`HEIGHT`] above its base, as Revit's "Height" option
//!   when placing
//!
//! Both levels drive it, so it follows them in the transaction that moves
//! them, and its [`TOP_ELEVATION`] and, with a top level, its [`HEIGHT`]
//! are recomputed.
//!
//! Its type is a [`COLUMN_TYPE`], a [section type](crate::section): a
//! rectangle or a circle, and a material. In plan the column is centred on
//! its location, [`X`] and [`Y`], and turned by [`ROTATION`] anticlockwise.
//!
//! A column placed on a grid intersection is **on two grids**, through
//! [`ON_GRID`] (element model, [§7 grid
//! placement](https://github.com/hito-project/hito/blob/main/docs/architecture/element-model.md#7-placement-sr-4-one-placement-type-with-several-kinds)).
//! The grids drive it: its location is recomputed as their intersection
//! plus [`GRID_OFFSET_X`] and [`GRID_OFFSET_Y`], so it follows them when
//! they move, as Revit's "Moves With Grids". If the grids stop crossing, or
//! one is deleted, the column stays where it was.
//!
//! ```
//! use hito_core::{Element, ElementStore, Model, ModelKind, SchemaRegistry, Value};
//! use hito_structural::column::{self, COLUMN_TYPE, Placement, Top};
//! use hito_structural::section::{self, Section};
//! use hito_structural::{class, level};
//!
//! # let mut registry = SchemaRegistry::new();
//! # hito_structural::register(&mut registry)?;
//! # let mut store = ElementStore::new(registry);
//! let mut tx = store.transaction();
//! let model = tx.insert_model(Model::new(ModelKind::Physical))?;
//! let level = |name, elevation| {
//!     Element::new(class(level::LEVEL), model)
//!         .with_property(level::NAME, name)
//!         .with_property(level::ELEVATION, elevation)
//! };
//! let ground = tx.insert(level("Planta baja", 0.0))?;
//! let first = tx.insert(level("Primer piso", 3.0))?;
//! let square = Section::Rectangle { width: 0.3, depth: 0.3 };
//! let c30 = section::insert(&mut tx, model, COLUMN_TYPE, "C 30 x 30", square, None)?;
//! let placement = Placement::new(c30, ground, [4.0, 0.0])
//!     .with_top(Top::Level { level: first, offset: 0.0 });
//! let column = column::insert(&mut tx, model, &placement)?;
//! tx.commit()?;
//! assert_eq!(store.property(column, column::HEIGHT), Some(&Value::Real(3.0)));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use hito_core::{
    ClassDef, DataType, Dimension, Element, ElementId, ModelId, Properties, Recompute,
    Relationship, RelationshipDef, Rule, StoreError, StoreView, Transaction,
};

use crate::class;
use crate::grid::{self, GRID, GridLine};
use crate::level::{ATTACHED_TO_LEVEL, ELEVATION, LEVEL, LEVEL_BASED_ELEMENT, LEVEL_OFFSET};
use crate::read::Read;
use crate::section::{self, SECTION_TYPE, Section};

/// A column.
pub const COLUMN: &str = "Column";
/// A column type: a [section type](crate::section).
pub const COLUMN_TYPE: &str = "ColumnType";
/// From a column to its top level, which drives it. Deleting the level
/// leaves the column unconnected, at the height it had.
pub const TOP_LEVEL: &str = "TopLevel";
/// From a column to each of the two grids it's placed on, which drive its
/// location. Deleting a grid leaves the column where it is.
pub const ON_GRID: &str = "OnGrid";

/// The column's location in plan: its centre.
pub const X: &str = "X";
pub const Y: &str = "Y";
/// How far the column is turned anticlockwise in plan, in radians. Its
/// section's width runs along the turned x axis.
pub const ROTATION: &str = "Rotation";
/// How far a column on grids is from their intersection, along x and y.
pub const GRID_OFFSET_X: &str = "GridOffsetX";
pub const GRID_OFFSET_Y: &str = "GridOffsetY";
/// How far the top is above the top level. Negative is below.
pub const TOP_OFFSET: &str = "TopOffset";
/// The column's height. Set it for an unconnected column; with a top level
/// it's recomputed.
pub const HEIGHT: &str = "Height";
/// The column's top above the project's origin. Recomputed. Its base is
/// the level-based element's [`BASE_ELEVATION`](crate::level::BASE_ELEVATION).
pub const TOP_ELEVATION: &str = "TopElevation";
/// How far the base is above the base level, as Revit's "Base Offset":
/// the level-based element's level offset.
pub use crate::level::LEVEL_OFFSET as BASE_OFFSET;

/// How close two points must be to count as one: 1 mm. A point typed this
/// close to a grid intersection places the column on the grids.
pub const TOLERANCE: f64 = 0.001;

const LENGTH: DataType = DataType::Quantity(Dimension::LENGTH);

/// Where a column's top is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Top {
    /// This high above its base.
    Unconnected { height: f64 },
    /// At this level, raised by `offset`.
    Level { level: ElementId, offset: f64 },
}

/// A column to add with [`insert`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub column_type: ElementId,
    pub base_level: ElementId,
    pub base_offset: f64,
    pub top: Top,
    /// The column's centre in plan. On grids, it's recomputed as their
    /// intersection.
    pub location: [f64; 2],
    pub rotation: f64,
    /// The two grids whose intersection the column is placed on.
    pub grids: Option<[ElementId; 2]>,
}

impl Placement {
    /// A column of `column_type` on `base_level` at `location`, 3 m high
    /// and unconnected, not turned and not on grids.
    pub fn new(column_type: ElementId, base_level: ElementId, location: [f64; 2]) -> Self {
        Self {
            column_type,
            base_level,
            base_offset: 0.0,
            top: Top::Unconnected { height: 3.0 },
            location,
            rotation: 0.0,
            grids: None,
        }
    }

    pub fn with_top(mut self, top: Top) -> Self {
        self.top = top;
        self
    }

    pub fn with_base_offset(mut self, offset: f64) -> Self {
        self.base_offset = offset;
        self
    }

    pub fn with_rotation(mut self, rotation: f64) -> Self {
        self.rotation = rotation;
        self
    }

    /// Places the column on the intersection of these two grids, so it
    /// follows them. Its location is kept as the offset from the
    /// intersection.
    pub fn on_grids(mut self, grids: [ElementId; 2]) -> Self {
        self.grids = Some(grids);
        self
    }
}

/// Adds a column to `model`. Returns the column.
pub fn insert(
    tx: &mut Transaction<'_>,
    model: ModelId,
    placement: &Placement,
) -> Result<ElementId, StoreError> {
    let p = placement;
    let [x, y] = p.location;
    let mut element = Element::new(class(COLUMN), model)
        .with_type(p.column_type)
        .with_property(X, x)
        .with_property(Y, y)
        .with_property(ROTATION, p.rotation)
        .with_property(LEVEL_OFFSET, p.base_offset);
    element = match p.top {
        Top::Unconnected { height } => element.with_property(HEIGHT, height),
        Top::Level { offset, .. } => element.with_property(TOP_OFFSET, offset),
    };
    if let Some(grids) = p.grids {
        let view = tx.view();
        let crossing = grid_point(&view, grids).unwrap_or(p.location);
        element = element
            .with_property(GRID_OFFSET_X, x - crossing[0])
            .with_property(GRID_OFFSET_Y, y - crossing[1]);
    }
    let column = tx.insert(element)?;
    tx.relate(Relationship::new(
        class(ATTACHED_TO_LEVEL),
        column,
        p.base_level,
    ))?;
    if let Top::Level { level, .. } = p.top {
        tx.relate(Relationship::new(class(TOP_LEVEL), column, level))?;
    }
    for grid in p.grids.into_iter().flatten() {
        tx.relate(Relationship::new(class(ON_GRID), column, grid))?;
    }
    Ok(column)
}

/// The column's centre in plan.
pub fn location(store: &StoreView<'_>, column: ElementId) -> Option<[f64; 2]> {
    read_location(store, column)
}

/// The column's base and top elevations, from its levels and offsets, as
/// they're recomputed.
pub fn elevations(store: &StoreView<'_>, column: ElementId) -> Option<(f64, f64)> {
    read_elevations(store, column)
}

/// The grids the column is placed on, if it's on two.
pub fn grids(store: &StoreView<'_>, column: ElementId) -> Option<[ElementId; 2]> {
    read_grids(store, column)
}

/// Every column in the store, oldest first.
pub fn columns(store: &StoreView<'_>) -> Vec<ElementId> {
    let registry = store.registry();
    let mut columns: Vec<_> = store
        .elements()
        .filter(|e| registry.is_a(e.class(), &class(COLUMN)))
        .map(Element::id)
        .collect();
    columns.sort();
    columns
}

/// The column standing on `level` (its base level) whose section contains
/// `point` in plan, the oldest if several do. What a footing placed at a
/// point on a column goes under.
pub fn standing_at(store: &StoreView<'_>, level: ElementId, point: [f64; 2]) -> Option<ElementId> {
    columns(store).into_iter().find(|&c| {
        let on_level = store.targets(c, &class(ATTACHED_TO_LEVEL)).first() == Some(&level);
        let inside = read_section(store, c)
            .zip(to_section(store, c, point))
            .is_some_and(|(section, local)| section.contains(local));
        on_level && inside
    })
}

/// The column classes.
pub(crate) fn classes() -> [ClassDef; 2] {
    let column_type = ClassDef::new(COLUMN_TYPE, class(SECTION_TYPE));
    let column = ClassDef::new(COLUMN, class(LEVEL_BASED_ELEMENT))
        .typed_by(class(COLUMN_TYPE))
        .property(X, LENGTH)
        .property(Y, LENGTH)
        .property(ROTATION, DataType::Quantity(Dimension::ANGLE))
        .property(GRID_OFFSET_X, LENGTH)
        .property(GRID_OFFSET_Y, LENGTH)
        .property(TOP_OFFSET, LENGTH)
        .property(HEIGHT, LENGTH)
        .property(TOP_ELEVATION, LENGTH);
    [column_type, column]
}

pub(crate) fn relationships() -> [RelationshipDef; 2] {
    let top_level = RelationshipDef::new(TOP_LEVEL, class(COLUMN), class(LEVEL)).driving();
    let on_grid = RelationshipDef::new(ON_GRID, class(COLUMN), class(GRID)).driving();
    [top_level, on_grid]
}

/// A column follows its grids and its levels.
pub(crate) fn recomputes() -> Vec<Recompute> {
    let follow = Recompute::new(class(COLUMN), |store, column| {
        let mut values = Properties::new();
        if let Some([x, y]) = on_grids_location(store, column.id()) {
            values.insert(X.into(), x.into());
            values.insert(Y.into(), y.into());
        }
        if let Some((bottom, top)) = read_elevations(store, column.id()) {
            values.insert(TOP_ELEVATION.into(), top.into());
            if !store.targets(column.id(), &class(TOP_LEVEL)).is_empty() {
                values.insert(HEIGHT.into(), (top - bottom).into());
            }
        }
        values
    });
    vec![follow]
}

/// The column rules:
///
/// - `ColumnType`: the column has a column type.
/// - `ColumnLocation`: the column has a location.
/// - `ColumnHeight`: the column's top is above its base.
/// - `ColumnConstraints`: at most one top level and two grids.
pub(crate) fn rules() -> Vec<Rule> {
    // The core already refuses a type of another class.
    let column_type = Rule::new("ColumnType", class(COLUMN), |_, column| {
        match column.type_id() {
            Some(_) => Ok(()),
            None => Err("the column has no type".into()),
        }
    });

    let location = Rule::new(
        "ColumnLocation",
        class(COLUMN),
        |store, column| match read_location(store, column.id()) {
            Some(_) => Ok(()),
            None => Err("the column has no location".into()),
        },
    );

    let height = Rule::new(
        "ColumnHeight",
        class(COLUMN),
        |store, column| match read_elevations(store, column.id()) {
            None => Err("the column has no base level or no height".into()),
            Some((bottom, top)) if top - bottom <= TOLERANCE => Err(format!(
                "the column's top ({top} m) isn't above its base ({bottom} m)"
            )),
            Some(_) => Ok(()),
        },
    );

    let constraints = Rule::new("ColumnConstraints", class(COLUMN), |store, column| {
        let tops = store.targets(column.id(), &class(TOP_LEVEL)).len();
        if tops > 1 {
            return Err(format!("the column has {tops} top levels"));
        }
        match store.targets(column.id(), &class(ON_GRID)).len() {
            n if n > 2 => Err(format!("the column is on {n} grids")),
            _ => Ok(()),
        }
    });

    vec![column_type, location, height, constraints]
}

pub(crate) fn read_location<'a>(store: &impl Read<'a>, column: ElementId) -> Option<[f64; 2]> {
    Some([store.real(column, X)?, store.real(column, Y)?])
}

pub(crate) fn read_rotation<'a>(store: &impl Read<'a>, column: ElementId) -> f64 {
    store.real(column, ROTATION).unwrap_or(0.0)
}

/// The column's section, from its type.
pub(crate) fn read_section<'a>(store: &impl Read<'a>, column: ElementId) -> Option<Section> {
    section::read_section(store, store.type_of(column)?)
}

/// `point`, in plan, in the column's section axes: from its centre, turned
/// back by its rotation.
pub(crate) fn to_section<'a>(
    store: &impl Read<'a>,
    column: ElementId,
    point: [f64; 2],
) -> Option<[f64; 2]> {
    let [x, y] = read_location(store, column)?;
    let (sin, cos) = read_rotation(store, column).sin_cos();
    let [dx, dy] = [point[0] - x, point[1] - y];
    Some([dx * cos + dy * sin, -dx * sin + dy * cos])
}

/// The base and top elevations, read from what drives them rather than
/// from the recomputed properties, so they're right inside a transaction
/// before it commits.
pub(crate) fn read_elevations<'a>(store: &impl Read<'a>, column: ElementId) -> Option<(f64, f64)> {
    let level_elevation = |level| store.real(level, ELEVATION).unwrap_or(0.0);
    let base = *store.targets(column, &class(ATTACHED_TO_LEVEL)).first()?;
    let bottom = level_elevation(base) + store.real(column, LEVEL_OFFSET).unwrap_or(0.0);
    let top = match store.targets(column, &class(TOP_LEVEL)).first() {
        Some(&level) => level_elevation(level) + store.real(column, TOP_OFFSET).unwrap_or(0.0),
        None => bottom + store.real(column, HEIGHT)?,
    };
    Some((bottom, top))
}

fn read_grids<'a>(store: &impl Read<'a>, column: ElementId) -> Option<[ElementId; 2]> {
    match store.targets(column, &class(ON_GRID))[..] {
        [a, b] => Some([a, b]),
        _ => None,
    }
}

/// Where two grids cross, if they do.
fn grid_point<'a>(store: &impl Read<'a>, [a, b]: [ElementId; 2]) -> Option<[f64; 2]> {
    let line = |id| {
        let get = |name| store.real(id, name);
        Some(GridLine::new(
            [get(grid::START_X)?, get(grid::START_Y)?],
            [get(grid::END_X)?, get(grid::END_Y)?],
        ))
    };
    line(a)?.intersection(&line(b)?)
}

/// A column on two crossing grids: where it goes, their intersection plus
/// its offsets.
fn on_grids_location<'a>(store: &impl Read<'a>, column: ElementId) -> Option<[f64; 2]> {
    let [x, y] = grid_point(store, read_grids(store, column)?)?;
    let offset = |name| store.real(column, name).unwrap_or(0.0);
    Some([x + offset(GRID_OFFSET_X), y + offset(GRID_OFFSET_Y)])
}
