//! Schemas: how a domain declares its element classes
//! ([element model §3](https://github.com/hito-project/hito/blob/main/docs/architecture/element-model.md#3-classes-and-schemas-defined-in-rust-described-at-runtime)).
//!
//! A domain describes its classes in a [`Schema`] and registers it in the
//! [`SchemaRegistry`](crate::SchemaRegistry) at startup. Each class derives
//! from a core base class (or from another class that does) and declares its
//! own typed properties:
//!
//! ```
//! use hito_core::{ClassDef, ClassId, DataType, Dimension, Schema, SchemaVersion};
//!
//! let schema = Schema::new("Example", SchemaVersion::new(1, 0, 0))
//!     .class(
//!         ClassDef::new("ColumnType", ClassId::DEFINITION_ELEMENT)
//!             .property("Width", DataType::Quantity(Dimension::LENGTH)),
//!     )
//!     .class(
//!         ClassDef::new("Column", ClassId::GEOMETRIC_ELEMENT_3D)
//!             .typed_by(ClassId::new("Example", "ColumnType"))
//!             .property("Height", DataType::Quantity(Dimension::LENGTH)),
//!     );
//! ```
//!
//! The core has no knowledge of any domain. It only defines the base classes
//! listed as constants on [`ClassId`].

use std::borrow::Cow;
use std::fmt;

use crate::{DataType, RelationshipDef};

/// A schema's version, as **Read.Write.Minor** (BIS's rule).
///
/// - **Read** changes when older software can no longer read the data.
/// - **Write** changes when older software can still read the data, but
///   writing it would lose or break something.
/// - **Minor** changes only add things. Older software reads and writes the
///   data, keeping what it doesn't know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaVersion {
    pub read: u16,
    pub write: u16,
    pub minor: u16,
}

impl SchemaVersion {
    pub const fn new(read: u16, write: u16, minor: u16) -> Self {
        Self { read, write, minor }
    }

    /// What software with this version of a schema may do with data written
    /// with version `data` of the same schema.
    ///
    /// Data from the same or an older version is always read and written:
    /// the software knows every earlier version and upgrades the data.
    pub fn access(self, data: SchemaVersion) -> Access {
        if data <= self || (data.read, data.write) == (self.read, self.write) {
            Access::ReadWrite
        } else if data.read == self.read {
            Access::ReadOnly
        } else {
            Access::None
        }
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.read, self.write, self.minor)
    }
}

/// What software may do with data written with another schema version. See
/// [`SchemaVersion::access`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    ReadWrite,
    ReadOnly,
    None,
}

/// The name of a class: its schema's name and its own, written
/// `Schema:Class`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClassId {
    schema: Cow<'static, str>,
    name: Cow<'static, str>,
}

impl ClassId {
    /// The core schema's name.
    pub const CORE_SCHEMA: &'static str = "Core";

    /// The root of every element class.
    pub const ELEMENT: Self = Self::core("Element");
    /// Elements with 3D geometry, such as columns and slabs.
    pub const GEOMETRIC_ELEMENT_3D: Self = Self::core("GeometricElement3d");
    /// Elements with 2D geometry, such as drafting in a drawing.
    pub const GEOMETRIC_ELEMENT_2D: Self = Self::core("GeometricElement2d");
    /// Elements that locate others in space: levels, grids, and later
    /// alignments.
    pub const SPATIAL_LOCATION_ELEMENT: Self = Self::core("SpatialLocationElement");
    /// Elements that define things others use: types, materials, profiles.
    /// Every type class derives from it.
    pub const DEFINITION_ELEMENT: Self = Self::core("DefinitionElement");
    /// Elements that carry information, such as documents.
    pub const INFORMATION_ELEMENT: Self = Self::core("InformationElement");
    /// The root of aspect classes an element has at most one of.
    pub const UNIQUE_ASPECT: Self = Self::core("UniqueAspect");
    /// The root of aspect classes an element can have any number of.
    pub const MULTI_ASPECT: Self = Self::core("MultiAspect");

    /// The class `name` in the schema `schema`.
    pub fn new(schema: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            schema: Cow::Owned(schema.into()),
            name: Cow::Owned(name.into()),
        }
    }

    /// A core class, usable in constants.
    const fn core(name: &'static str) -> Self {
        Self {
            schema: Cow::Borrowed(Self::CORE_SCHEMA),
            name: Cow::Borrowed(name),
        }
    }

    /// The name of the schema that declares the class.
    pub fn schema(&self) -> &str {
        &self.schema
    }

    /// The class's name within its schema.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for ClassId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.schema, self.name)
    }
}

impl fmt::Debug for ClassId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ClassId({self})")
    }
}

/// A property a class declares: a name and a data type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyDef {
    name: String,
    data_type: DataType,
}

impl PropertyDef {
    pub fn new(name: impl Into<String>, data_type: DataType) -> Self {
        Self {
            name: name.into(),
            data_type,
        }
    }

    /// The property's name. It's an identifier, not interface text: the UI
    /// shows a translated label for it.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn data_type(&self) -> DataType {
        self.data_type
    }
}

/// A class as a schema declares it.
#[derive(Debug, Clone)]
pub struct ClassDef {
    pub(crate) name: String,
    pub(crate) base: ClassId,
    pub(crate) type_class: Option<ClassId>,
    pub(crate) properties: Vec<PropertyDef>,
}

impl ClassDef {
    /// A class called `name` that derives from `base`.
    ///
    /// Deriving from [`ClassId::UNIQUE_ASPECT`] or [`ClassId::MULTI_ASPECT`],
    /// directly or not, makes it an aspect class. Anything else is an element
    /// class.
    pub fn new(name: impl Into<String>, base: ClassId) -> Self {
        Self {
            name: name.into(),
            base,
            type_class: None,
            properties: Vec::new(),
        }
    }

    /// Adds a property. A class has its own properties and those of all its
    /// base classes.
    pub fn property(mut self, name: impl Into<String>, data_type: DataType) -> Self {
        self.properties.push(PropertyDef::new(name, data_type));
        self
    }

    /// Lets elements of this class have a type of class `type_class`, which
    /// must derive from [`ClassId::DEFINITION_ELEMENT`].
    ///
    /// The type's property values are shared by all its instances. When the
    /// instance's class declares a property with the same name, an instance
    /// value overrides the type's. Subclasses inherit the type class, or
    /// narrow it to a subclass of it.
    pub fn typed_by(mut self, type_class: ClassId) -> Self {
        self.type_class = Some(type_class);
        self
    }
}

/// A domain's schema: its name, version, classes and relationship types.
#[derive(Debug, Clone)]
pub struct Schema {
    pub(crate) name: String,
    pub(crate) version: SchemaVersion,
    pub(crate) classes: Vec<ClassDef>,
    pub(crate) relationships: Vec<RelationshipDef>,
}

impl Schema {
    /// An empty schema. The name is permanent: it's stored in files and
    /// prefixes every class name, so it shouldn't contain a product name.
    pub fn new(name: impl Into<String>, version: SchemaVersion) -> Self {
        Self {
            name: name.into(),
            version,
            classes: Vec::new(),
            relationships: Vec::new(),
        }
    }

    /// Adds a class. A class may derive from or be typed by classes declared
    /// earlier in the same schema, or in a schema registered before it.
    pub fn class(mut self, class: ClassDef) -> Self {
        self.classes.push(class);
        self
    }

    /// Adds a relationship type. It shares the schema's names with the
    /// classes, and links element classes of this schema or of schemas
    /// registered before it.
    pub fn relationship(mut self, relationship: RelationshipDef) -> Self {
        self.relationships.push(relationship);
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn version(&self) -> SchemaVersion {
        self.version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_rules_follow_read_write_minor() {
        let software = SchemaVersion::new(2, 1, 3);
        let access = |r, w, m| software.access(SchemaVersion::new(r, w, m));

        // The same or older: read, upgrade and write.
        assert_eq!(access(2, 1, 3), Access::ReadWrite);
        assert_eq!(access(2, 1, 0), Access::ReadWrite);
        assert_eq!(access(1, 9, 9), Access::ReadWrite);
        // Newer minor version: only additions, so still read and write.
        assert_eq!(access(2, 1, 7), Access::ReadWrite);
        // Newer write version: read only.
        assert_eq!(access(2, 2, 0), Access::ReadOnly);
        // Newer read version: can't read.
        assert_eq!(access(3, 0, 0), Access::None);
    }

    #[test]
    fn class_ids_print_as_schema_colon_name() {
        assert_eq!(ClassId::ELEMENT.to_string(), "Core:Element");
        assert_eq!(ClassId::new("Core", "Element"), ClassId::ELEMENT);
        assert_eq!(SchemaVersion::new(1, 0, 2).to_string(), "1.0.2");
    }
}
