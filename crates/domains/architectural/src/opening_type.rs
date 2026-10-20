//! Door and window types: fixed geometry per type (#194).
//!
//! A door or window type is a definition element with a name and the
//! sizes that make up its geometry, like a Revit family type such as
//! "Puerta 80 × 205". There's no family editor
//! ([ADR 0020](https://github.com/hito-project/hito/blob/main/docs/decisions/0020-rvt-rfa-through-ifc.md)):
//! every door of a type has the same shape, and only these values change it.
//!
//! - [`WIDTH`] and [`HEIGHT`] are the frame's outside: the hole the door or
//!   window cuts in its wall, as IFC's `OverallWidth` and `OverallHeight`.
//! - [`SILL_HEIGHT`] is how far the bottom is above the wall's base level,
//!   as Revit's "Default Sill Height". Doors default to 0. Each door or
//!   window can [override](crate::opening::SILL_HEIGHT) it.
//! - [`FRAME_WIDTH`] is the frame's face, seen in elevation, and
//!   [`FRAME_DEPTH`] how deep it is across the wall.
//!
//! Which types a project starts with comes from a library that ships with
//! HITO, outside the domain: nothing here is specific to a country.
//!
//! ```
//! use hito_architectural::opening_type::{self, Definition};
//! use hito_core::{ElementStore, Model, ModelKind, SchemaRegistry};
//!
//! # let mut registry = SchemaRegistry::new();
//! # hito_architectural::register(&mut registry)?;
//! # let mut store = ElementStore::new(registry);
//! let mut tx = store.transaction();
//! let definitions = tx.insert_model(Model::new(ModelKind::Definition))?;
//! let door = Definition::door("Puerta 80 × 205", 0.8, 2.05);
//! let door = opening_type::insert(&mut tx, definitions, &door)?;
//! tx.commit()?;
//! let size = opening_type::size(&store.view(), door).unwrap();
//! assert_eq!((size.width, size.height, size.sill_height), (0.8, 2.05, 0.0));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use hito_core::{
    ClassDef, ClassId, DataType, Dimension, Element, ElementId, ModelId, Rule, StoreError,
    StoreView, Transaction, Value,
};

use crate::class;
use crate::read::Read;

/// What door and window types have in common. Not used on its own.
pub const OPENING_TYPE: &str = "WallOpeningType";
/// A door type.
pub const DOOR_TYPE: &str = "DoorType";
/// A window type.
pub const WINDOW_TYPE: &str = "WindowType";

/// The type's name, unique among the types of its kind. It's data, not
/// interface text, so it isn't translated.
pub const NAME: &str = "Name";
/// The frame's outside width: the width of the hole in the wall.
pub const WIDTH: &str = "Width";
/// The frame's outside height: the height of the hole in the wall.
pub const HEIGHT: &str = "Height";
/// The bottom's height above the wall's base level, unless a door or
/// window overrides it. Unset means 0.
pub const SILL_HEIGHT: &str = "SillHeight";
/// The width of the frame's face, in elevation.
pub const FRAME_WIDTH: &str = "FrameWidth";
/// How deep the frame is, across the wall.
pub const FRAME_DEPTH: &str = "FrameDepth";

const LENGTH: DataType = DataType::Quantity(Dimension::LENGTH);

/// A door or a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Door,
    Window,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Door, Kind::Window];

    /// The class of the types of this kind.
    pub fn type_class(self) -> ClassId {
        match self {
            Kind::Door => class(DOOR_TYPE),
            Kind::Window => class(WINDOW_TYPE),
        }
    }

    /// The class of the doors or windows of this kind.
    pub fn class(self) -> ClassId {
        match self {
            Kind::Door => class(crate::opening::DOOR),
            Kind::Window => class(crate::opening::WINDOW),
        }
    }
}

/// A door or window type to add with [`insert`].
#[derive(Debug, Clone, PartialEq)]
pub struct Definition {
    pub kind: Kind,
    pub name: String,
    pub width: f64,
    pub height: f64,
    pub sill_height: f64,
    pub frame_width: f64,
    pub frame_depth: f64,
}

impl Definition {
    /// A door type, `width` by `height`, standing on the level, with a
    /// 5 cm frame 10 cm deep.
    pub fn door(name: impl Into<String>, width: f64, height: f64) -> Self {
        Self {
            kind: Kind::Door,
            name: name.into(),
            width,
            height,
            sill_height: 0.0,
            frame_width: 0.05,
            frame_depth: 0.1,
        }
    }

    /// A window type, `width` by `height`, its sill `sill_height` above the
    /// level, with a 5 cm frame 10 cm deep.
    pub fn window(name: impl Into<String>, width: f64, height: f64, sill_height: f64) -> Self {
        Self {
            kind: Kind::Window,
            sill_height,
            ..Self::door(name, width, height)
        }
    }

    pub fn with_frame(mut self, width: f64, depth: f64) -> Self {
        self.frame_width = width;
        self.frame_depth = depth;
        self
    }
}

/// Adds a door or window type to `model`. Returns the type.
pub fn insert(
    tx: &mut Transaction<'_>,
    model: ModelId,
    definition: &Definition,
) -> Result<ElementId, StoreError> {
    let d = definition;
    let element = Element::new(d.kind.type_class(), model)
        .with_property(NAME, d.name.as_str())
        .with_property(WIDTH, d.width)
        .with_property(HEIGHT, d.height)
        .with_property(SILL_HEIGHT, d.sill_height)
        .with_property(FRAME_WIDTH, d.frame_width)
        .with_property(FRAME_DEPTH, d.frame_depth);
    tx.insert(element)
}

/// A type's sizes, as its doors and windows read them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
    pub sill_height: f64,
    pub frame_width: f64,
    pub frame_depth: f64,
}

/// A door or window type's sizes, `None` if it isn't one or a size is
/// missing.
pub fn size(store: &StoreView<'_>, opening_type: ElementId) -> Option<Size> {
    read_size(store, opening_type)
}

/// Whether the type is a door or a window type.
pub fn kind(store: &StoreView<'_>, opening_type: ElementId) -> Option<Kind> {
    read_kind(store, opening_type)
}

/// Every type of `kind`, sorted by name.
pub fn types(store: &StoreView<'_>, kind: Kind) -> Vec<ElementId> {
    let registry = store.registry();
    let class = kind.type_class();
    let mut types: Vec<_> = store
        .elements()
        .filter(|e| registry.is_a(e.class(), &class))
        .map(|e| (store.text(e.id(), NAME).unwrap_or_default(), e.id()))
        .collect();
    types.sort();
    types.into_iter().map(|(_, id)| id).collect()
}

pub(crate) fn read_kind<'a>(store: &impl Read<'a>, id: ElementId) -> Option<Kind> {
    Kind::ALL
        .into_iter()
        .find(|k| store.is_a(id, &k.type_class()))
}

pub(crate) fn read_size<'a>(store: &impl Read<'a>, id: ElementId) -> Option<Size> {
    read_kind(store, id)?;
    Some(Size {
        width: store.real(id, WIDTH)?,
        height: store.real(id, HEIGHT)?,
        sill_height: store.real(id, SILL_HEIGHT).unwrap_or(0.0),
        frame_width: store.real(id, FRAME_WIDTH).unwrap_or(0.0),
        frame_depth: store.real(id, FRAME_DEPTH)?,
    })
}

/// The type classes.
pub(crate) fn classes() -> [ClassDef; 3] {
    let base = ClassDef::new(OPENING_TYPE, ClassId::DEFINITION_ELEMENT)
        .property(NAME, DataType::Text)
        .property(WIDTH, LENGTH)
        .property(HEIGHT, LENGTH)
        .property(SILL_HEIGHT, LENGTH)
        .property(FRAME_WIDTH, LENGTH)
        .property(FRAME_DEPTH, LENGTH);
    let door = ClassDef::new(DOOR_TYPE, class(OPENING_TYPE));
    let window = ClassDef::new(WINDOW_TYPE, class(OPENING_TYPE));
    [base, door, window]
}

/// The type rules:
///
/// - `OpeningTypeName`: every door or window type has a name, and no
///   other type of its kind shares it, as in Revit.
/// - `OpeningTypeSize`: the width, height and frame depth are greater
///   than zero, the sill height and frame width aren't negative, and the
///   frame leaves a clear opening inside it.
pub(crate) fn rules() -> Vec<Rule> {
    let name = Rule::new("OpeningTypeName", class(OPENING_TYPE), |store, t| {
        let name = match t.properties().get(NAME) {
            Some(Value::Text(name)) if !name.trim().is_empty() => name,
            _ => return Err("the type has no name".into()),
        };
        let same_name = store.elements().find(|other| {
            other.id() != t.id()
                && other.class() == t.class()
                && other.properties().get(NAME) == Some(&Value::Text(name.clone()))
        });
        match same_name {
            Some(other) => Err(format!("type {} is also called `{name}`", other.id())),
            None => Ok(()),
        }
    });

    let size = Rule::new("OpeningTypeSize", class(OPENING_TYPE), |store, t| {
        let get = |name| store.real(t.id(), name);
        let positive = |name| match get(name) {
            Some(x) if x > 0.0 && x.is_finite() => Ok(x),
            Some(x) => Err(format!("the type's `{name}` is {x} m")),
            None => Err(format!("the type has no `{name}`")),
        };
        let width = positive(WIDTH)?;
        let height = positive(HEIGHT)?;
        positive(FRAME_DEPTH)?;
        for name in [SILL_HEIGHT, FRAME_WIDTH] {
            if let Some(x) = get(name).filter(|x| !(*x >= 0.0 && x.is_finite())) {
                return Err(format!("the type's `{name}` is {x} m"));
            }
        }
        let frame = get(FRAME_WIDTH).unwrap_or(0.0);
        if 2.0 * frame >= width || frame >= height {
            return Err(format!(
                "a {frame} m frame leaves no opening in {width} × {height} m"
            ));
        }
        Ok(())
    });

    vec![name, size]
}
