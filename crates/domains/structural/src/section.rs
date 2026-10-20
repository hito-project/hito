//! Section types: the cross-section and material that column types (#37)
//! and beam types (#38) share.
//!
//! A section type is a definition element with a name, a [`Shape`] and the
//! dimensions of that shape, as the `b` and `h` (or the diameter) of
//! Revit's concrete column and beam families. It's linked to its material
//! by [`SECTION_MATERIAL`]. Elements of a type take their section from it,
//! so editing the type changes every element of it.
//!
//! - A **rectangle** is [`WIDTH`] across and [`DEPTH`] deep. In a column,
//!   the width runs along the column's x axis (turned by its rotation) and
//!   the depth along its y axis. In a beam, the width is horizontal and the
//!   depth vertical. They're IFC's `IfcRectangleProfileDef` `XDim` and
//!   `YDim`.
//! - A **circle** has a [`DIAMETER`] (IFC's `IfcCircleProfileDef`, which
//!   stores the radius).
//!
//! ```
//! use hito_core::{ElementStore, Model, ModelKind, SchemaRegistry};
//! use hito_structural::section::{self, Section};
//! use hito_structural::column::COLUMN_TYPE;
//!
//! # let mut registry = SchemaRegistry::new();
//! # hito_structural::register(&mut registry)?;
//! # let mut store = ElementStore::new(registry);
//! let mut tx = store.transaction();
//! let definitions = tx.insert_model(Model::new(ModelKind::Definition))?;
//! let square = Section::Rectangle { width: 0.3, depth: 0.3 };
//! let c30 = section::insert(&mut tx, definitions, COLUMN_TYPE, "C 30 x 30", square, None)?;
//! tx.commit()?;
//! assert_eq!(section::of(&store.view(), c30), Some(square));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use hito_core::{
    ClassDef, ClassId, DataType, Dimension, Element, ElementId, ModelId, Relationship,
    RelationshipDef, Rule, StoreError, StoreView, Transaction, Value,
};
use hito_geometry::Profile;

use crate::read::Read;
use crate::{class, material};

/// The base class of column types and beam types.
pub const SECTION_TYPE: &str = "SectionType";
/// From a section type to its material.
pub const SECTION_MATERIAL: &str = "SectionMaterial";

/// The type's name, unique among types of its class, such as `C 30 x 30`.
/// It's data, not interface text, so it isn't translated.
pub const NAME: &str = "Name";
/// The section's shape, as one of [`Shape`]'s names.
pub const SHAPE: &str = "Shape";
/// A rectangle's side across: Revit's `b`.
pub const WIDTH: &str = "Width";
/// A rectangle's other side: Revit's `h`. A beam's depth is vertical.
pub const DEPTH: &str = "Depth";
/// A circle's diameter.
pub const DIAMETER: &str = "Diameter";

const LENGTH: DataType = DataType::Quantity(Dimension::LENGTH);

/// A section's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shape {
    Rectangular,
    Circular,
}

impl Shape {
    pub const ALL: [Shape; 2] = [Self::Rectangular, Self::Circular];

    /// The name stored in the [`SHAPE`] property.
    pub fn name(self) -> &'static str {
        match self {
            Self::Rectangular => "Rectangular",
            Self::Circular => "Circular",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// A cross-section, centred on the element's axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Section {
    /// `width` along the section's x axis, `depth` along its y axis.
    Rectangle {
        width: f64,
        depth: f64,
    },
    Circle {
        diameter: f64,
    },
}

impl Section {
    pub fn shape(&self) -> Shape {
        match self {
            Self::Rectangle { .. } => Shape::Rectangular,
            Self::Circle { .. } => Shape::Circular,
        }
    }

    /// Whether every dimension is greater than zero and finite.
    pub fn is_valid(&self) -> bool {
        let positive = |x: f64| x > 0.0 && x.is_finite();
        match *self {
            Self::Rectangle { width, depth } => positive(width) && positive(depth),
            Self::Circle { diameter } => positive(diameter),
        }
    }

    /// The section as a profile to extrude, centred on the origin.
    pub fn profile(&self) -> Profile {
        match *self {
            Self::Rectangle { width, depth } => Profile::Rectangle {
                width,
                height: depth,
            },
            Self::Circle { diameter } => Profile::Circle {
                radius: diameter / 2.0,
            },
        }
    }

    /// How far the outline is from the centre along `direction`, given in
    /// the section's axes. It needn't be a unit vector. A beam framing into
    /// a column stops this far from the column's centre.
    pub fn reach(&self, direction: [f64; 2]) -> f64 {
        let length = direction[0].hypot(direction[1]);
        if length == 0.0 {
            return 0.0;
        }
        let [dx, dy] = [direction[0] / length, direction[1] / length];
        match *self {
            Self::Rectangle { width, depth } => {
                let along = |half: f64, d: f64| match d.abs() {
                    0.0 => f64::INFINITY,
                    d => half / d,
                };
                along(width / 2.0, dx).min(along(depth / 2.0, dy))
            }
            Self::Circle { diameter } => diameter / 2.0,
        }
    }

    /// Whether `point`, given in the section's axes from its centre, is
    /// inside the section or on its outline.
    pub fn contains(&self, [x, y]: [f64; 2]) -> bool {
        match *self {
            Self::Rectangle { width, depth } => x.abs() <= width / 2.0 && y.abs() <= depth / 2.0,
            Self::Circle { diameter } => x.hypot(y) <= diameter / 2.0,
        }
    }
}

/// The section type class.
pub(crate) fn classes() -> [ClassDef; 1] {
    let section = ClassDef::new(SECTION_TYPE, ClassId::DEFINITION_ELEMENT)
        .property(NAME, DataType::Text)
        .property(SHAPE, DataType::Text)
        .property(WIDTH, LENGTH)
        .property(DEPTH, LENGTH)
        .property(DIAMETER, LENGTH);
    [section]
}

pub(crate) fn relationships() -> [RelationshipDef; 1] {
    let material = RelationshipDef::new(
        SECTION_MATERIAL,
        class(SECTION_TYPE),
        class(material::MATERIAL),
    );
    [material]
}

/// The section type rules:
///
/// - `SectionTypeName`: every section type has a name, and no other type of
///   the same class has it, as in Revit.
/// - `Section`: the type has a known shape, and that shape's dimensions,
///   each greater than zero.
/// - `SectionMaterial`: at most one material.
pub(crate) fn rules() -> Vec<Rule> {
    let name = Rule::new("SectionTypeName", class(SECTION_TYPE), |store, section| {
        let name = match section.properties().get(NAME) {
            Some(Value::Text(name)) if !name.trim().is_empty() => name,
            _ => return Err("the type has no name".into()),
        };
        let same_name = store.elements().find(|other| {
            other.id() != section.id()
                && other.class() == section.class()
                && other.properties().get(NAME) == Some(&Value::Text(name.clone()))
        });
        match same_name {
            Some(other) => Err(format!("type {} is also called `{name}`", other.id())),
            None => Ok(()),
        }
    });

    let shape = Rule::new("Section", class(SECTION_TYPE), |store, section| {
        let shape = store.text(section.id(), SHAPE);
        let Some(shape) = shape.and_then(Shape::from_name) else {
            return Err(match shape {
                Some(shape) => format!("`{shape}` isn't a section shape"),
                None => "the type has no shape".into(),
            });
        };
        match read_section(store, section.id()) {
            Some(s) if s.is_valid() => Ok(()),
            _ => Err(match shape {
                Shape::Rectangular => "the rectangle needs a width and a depth above zero",
                Shape::Circular => "the circle needs a diameter above zero",
            }
            .into()),
        }
    });

    let material = Rule::new(
        "SectionMaterial",
        class(SECTION_TYPE),
        |store, section| match store.targets(section.id(), &class(SECTION_MATERIAL)).len() {
            0 | 1 => Ok(()),
            n => Err(format!("the type has {n} materials")),
        },
    );

    vec![name, shape, material]
}

/// Adds a type of class `type_class` (such as
/// [`COLUMN_TYPE`](crate::column::COLUMN_TYPE)), called `name`, with
/// `section` and `material`, to `model`. Returns the type.
pub fn insert(
    tx: &mut Transaction<'_>,
    model: ModelId,
    type_class: &str,
    name: &str,
    section: Section,
    material: Option<ElementId>,
) -> Result<ElementId, StoreError> {
    let element = Element::new(class(type_class), model)
        .with_property(NAME, name)
        .with_property(SHAPE, section.shape().name());
    let element = match section {
        Section::Rectangle { width, depth } => element
            .with_property(WIDTH, width)
            .with_property(DEPTH, depth),
        Section::Circle { diameter } => element.with_property(DIAMETER, diameter),
    };
    let id = tx.insert(element)?;
    if let Some(material) = material {
        tx.relate(Relationship::new(class(SECTION_MATERIAL), id, material))?;
    }
    Ok(id)
}

/// The section of a type, or of an element through its type.
pub fn of(store: &StoreView<'_>, id: ElementId) -> Option<Section> {
    read_section(store, id)
}

/// The material of a type, or of an element through its type.
pub fn material_of(store: &StoreView<'_>, id: ElementId) -> Option<ElementId> {
    let section_type = match store.is_a(id, &class(SECTION_TYPE)) {
        true => id,
        false => store.get(id)?.type_id()?,
    };
    let materials = store.targets(section_type, &class(SECTION_MATERIAL));
    materials.first().copied()
}

/// Reads the section of `id`, a type or an element whose type has it.
pub(crate) fn read_section<'a>(store: &impl Read<'a>, id: ElementId) -> Option<Section> {
    let shape = store.text(id, SHAPE).and_then(Shape::from_name)?;
    Some(match shape {
        Shape::Rectangular => Section::Rectangle {
            width: store.real(id, WIDTH)?,
            depth: store.real(id, DEPTH)?,
        },
        Shape::Circular => Section::Circle {
            diameter: store.real(id, DIAMETER)?,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reach_is_the_distance_to_the_outline() {
        let rect = Section::Rectangle {
            width: 0.4,
            depth: 0.2,
        };
        assert_eq!(rect.reach([1.0, 0.0]), 0.2);
        assert_eq!(rect.reach([-3.0, 0.0]), 0.2);
        assert_eq!(rect.reach([0.0, 1.0]), 0.1);
        // Diagonally, to the nearer side.
        assert!((rect.reach([1.0, 1.0]) - 0.1 * 2f64.sqrt()).abs() < 1e-12);
        let circle = Section::Circle { diameter: 0.5 };
        assert_eq!(circle.reach([0.3, -0.7]), 0.25);
        assert_eq!(rect.reach([0.0, 0.0]), 0.0);
    }

    #[test]
    fn points_on_the_outline_are_inside() {
        let rect = Section::Rectangle {
            width: 0.4,
            depth: 0.2,
        };
        assert!(rect.contains([0.2, -0.1]));
        assert!(!rect.contains([0.21, 0.0]));
        let circle = Section::Circle { diameter: 0.5 };
        assert!(circle.contains([0.0, 0.25]));
        assert!(!circle.contains([0.2, 0.2]));
    }
}
