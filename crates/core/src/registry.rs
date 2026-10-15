//! The runtime schema registry: every registered class, its properties and
//! its place in the class hierarchy.

use std::collections::HashMap;
use std::fmt;
use std::iter;

use crate::schema::ClassDef;
use crate::{ClassId, PropertyDef, Schema, SchemaVersion};

/// Whether a class describes elements or aspects. It comes from the core
/// root the class derives from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClassKind {
    Element,
    /// At most one per element.
    UniqueAspect,
    /// Any number per element.
    MultiAspect,
}

/// A registered class.
#[derive(Debug, Clone)]
pub struct Class {
    id: ClassId,
    base: Option<ClassId>,
    kind: ClassKind,
    type_class: Option<ClassId>,
    properties: Vec<PropertyDef>,
}

impl Class {
    pub fn id(&self) -> &ClassId {
        &self.id
    }

    /// The class it derives from. Only the core roots have none.
    pub fn base(&self) -> Option<&ClassId> {
        self.base.as_ref()
    }

    pub fn kind(&self) -> ClassKind {
        self.kind
    }

    /// The class its elements' types must be, declared or inherited. `None`
    /// if its elements have no type.
    pub fn type_class(&self) -> Option<&ClassId> {
        self.type_class.as_ref()
    }

    /// The properties the class declares itself, without inherited ones.
    pub fn own_properties(&self) -> &[PropertyDef] {
        &self.properties
    }
}

/// Every schema and class the running program knows.
///
/// It starts with the core schema. Each domain then
/// [registers](SchemaRegistry::register) its schema at startup, before any
/// element is created. A schema can only use classes from schemas registered
/// before it, so domains register in dependency order.
#[derive(Debug, Clone)]
pub struct SchemaRegistry {
    schemas: Vec<(String, SchemaVersion)>,
    classes: HashMap<ClassId, Class>,
}

impl SchemaRegistry {
    /// The version of the core schema.
    pub const CORE_VERSION: SchemaVersion = SchemaVersion::new(1, 0, 0);

    /// A registry with only the core schema.
    pub fn new() -> Self {
        let mut registry = Self {
            schemas: vec![(ClassId::CORE_SCHEMA.to_owned(), Self::CORE_VERSION)],
            classes: HashMap::new(),
        };
        let roots = [
            (ClassId::ELEMENT, ClassKind::Element),
            (ClassId::UNIQUE_ASPECT, ClassKind::UniqueAspect),
            (ClassId::MULTI_ASPECT, ClassKind::MultiAspect),
        ];
        for (id, kind) in roots {
            registry.insert(id, None, kind, None, Vec::new());
        }
        let element = [
            ClassId::GEOMETRIC_ELEMENT_3D,
            ClassId::GEOMETRIC_ELEMENT_2D,
            ClassId::DEFINITION_ELEMENT,
            ClassId::INFORMATION_ELEMENT,
        ];
        for id in element {
            registry.insert(
                id,
                Some(ClassId::ELEMENT),
                ClassKind::Element,
                None,
                Vec::new(),
            );
        }
        let spatial = ClassId::SPATIAL_LOCATION_ELEMENT;
        let base = Some(ClassId::GEOMETRIC_ELEMENT_3D);
        registry.insert(spatial, base, ClassKind::Element, None, Vec::new());
        registry
    }

    /// Adds a schema and all its classes, or nothing if any of it is
    /// invalid.
    pub fn register(&mut self, schema: Schema) -> Result<(), SchemaError> {
        if self.schema_version(&schema.name).is_some() {
            return Err(SchemaError::DuplicateSchema(schema.name));
        }
        if !is_identifier(&schema.name) {
            return Err(SchemaError::InvalidName(schema.name));
        }
        for class in &schema.classes {
            if let Err(error) = self.register_class(&schema.name, class) {
                self.classes.retain(|id, _| id.schema() != schema.name);
                return Err(error);
            }
        }
        self.schemas.push((schema.name, schema.version));
        Ok(())
    }

    /// The registered schemas and their versions, core first, then in the
    /// order they were registered. A file records these.
    pub fn schemas(&self) -> impl Iterator<Item = (&str, SchemaVersion)> {
        self.schemas.iter().map(|(name, v)| (name.as_str(), *v))
    }

    /// The version of a registered schema.
    pub fn schema_version(&self, name: &str) -> Option<SchemaVersion> {
        self.schemas().find(|(n, _)| *n == name).map(|(_, v)| v)
    }

    pub fn class(&self, id: &ClassId) -> Option<&Class> {
        self.classes.get(id)
    }

    /// Whether `class` is `base` or derives from it, directly or not.
    pub fn is_a(&self, class: &ClassId, base: &ClassId) -> bool {
        self.lineage(class).any(|c| &c.id == base)
    }

    /// The property `name` of `class`, declared by it or inherited.
    pub fn property(&self, class: &ClassId, name: &str) -> Option<&PropertyDef> {
        self.properties(class).find(|p| p.name() == name)
    }

    /// All properties of `class`: its own, then its base classes'.
    pub fn properties(&self, class: &ClassId) -> impl Iterator<Item = &PropertyDef> {
        self.lineage(class).flat_map(|c| &c.properties)
    }

    /// `class` and its base classes, from the class up to its core root.
    fn lineage(&self, class: &ClassId) -> impl Iterator<Item = &Class> {
        iter::successors(self.classes.get(class), |c| {
            c.base.as_ref().and_then(|base| self.classes.get(base))
        })
    }

    fn insert(
        &mut self,
        id: ClassId,
        base: Option<ClassId>,
        kind: ClassKind,
        type_class: Option<ClassId>,
        properties: Vec<PropertyDef>,
    ) {
        let class = Class {
            id: id.clone(),
            base,
            kind,
            type_class,
            properties,
        };
        self.classes.insert(id, class);
    }

    fn register_class(&mut self, schema: &str, def: &ClassDef) -> Result<(), SchemaError> {
        let id = ClassId::new(schema, &def.name);
        if !is_identifier(&def.name) {
            return Err(SchemaError::InvalidName(def.name.clone()));
        }
        if self.classes.contains_key(&id) {
            return Err(SchemaError::DuplicateClass(id));
        }
        let base = self
            .class(&def.base)
            .ok_or_else(|| SchemaError::UnknownClass(def.base.clone()))?;
        let kind = base.kind;
        let inherited_type_class = base.type_class.clone();

        for (i, property) in def.properties.iter().enumerate() {
            if !is_identifier(property.name()) {
                return Err(SchemaError::InvalidName(property.name().to_owned()));
            }
            let declared_twice = def.properties[..i]
                .iter()
                .any(|p| p.name() == property.name());
            if declared_twice || self.property(&def.base, property.name()).is_some() {
                return Err(SchemaError::DuplicateProperty {
                    class: id,
                    property: property.name().to_owned(),
                });
            }
        }

        let type_class = match &def.type_class {
            None => inherited_type_class,
            Some(type_class) => {
                if self.class(type_class).is_none() {
                    return Err(SchemaError::UnknownClass(type_class.clone()));
                }
                let narrows_inherited = inherited_type_class
                    .as_ref()
                    .is_none_or(|inherited| self.is_a(type_class, inherited));
                if kind != ClassKind::Element
                    || !self.is_a(type_class, &ClassId::DEFINITION_ELEMENT)
                    || !narrows_inherited
                {
                    return Err(SchemaError::InvalidTypeClass {
                        class: id,
                        type_class: type_class.clone(),
                    });
                }
                Some(type_class.clone())
            }
        };

        let base = Some(def.base.clone());
        self.insert(id.clone(), base, kind, type_class, def.properties.clone());
        self.check_overrides(&id)
    }

    /// Checks that wherever an instance property overrides a type property,
    /// both have the same data type. Runs for each new class, both as an
    /// instance class and as a possible type class.
    fn check_overrides(&self, new: &ClassId) -> Result<(), SchemaError> {
        let new_type_class = self.classes[new].type_class.as_ref();
        let mut pairs = Vec::new();
        for class in self.classes.values() {
            if new_type_class.is_some_and(|t| self.is_a(&class.id, t)) {
                pairs.push((new, &class.id));
            }
            if class.type_class.as_ref().is_some_and(|t| self.is_a(new, t)) {
                pairs.push((&class.id, new));
            }
        }
        for (instance_class, type_class) in pairs {
            for property in self.properties(instance_class) {
                let shared = self.property(type_class, property.name());
                if shared.is_some_and(|p| p.data_type() != property.data_type()) {
                    return Err(SchemaError::OverrideMismatch {
                        instance_class: instance_class.clone(),
                        type_class: type_class.clone(),
                        property: property.name().to_owned(),
                    });
                }
            }
        }
        Ok(())
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Names of schemas, classes and properties are identifiers: an ASCII letter
/// followed by ASCII letters, digits and underscores. They are stored in
/// files and used by exporters, so they stay simple.
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Why a schema couldn't be registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    /// A schema with this name is already registered.
    DuplicateSchema(String),
    /// A schema, class or property name isn't an identifier.
    InvalidName(String),
    /// The schema declares this class twice.
    DuplicateClass(ClassId),
    /// A base or type class isn't registered, nor declared earlier in the
    /// schema.
    UnknownClass(ClassId),
    /// The class declares a property twice, or one it already inherits.
    DuplicateProperty { class: ClassId, property: String },
    /// The type class isn't a definition element class, the class isn't an
    /// element class, or the type class doesn't derive from the inherited
    /// one.
    InvalidTypeClass { class: ClassId, type_class: ClassId },
    /// An instance property overrides a type property of another data type.
    OverrideMismatch {
        instance_class: ClassId,
        type_class: ClassId,
        property: String,
    },
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateSchema(name) => write!(f, "schema `{name}` is already registered"),
            Self::InvalidName(name) => write!(f, "`{name}` isn't a valid name"),
            Self::DuplicateClass(id) => write!(f, "class `{id}` is declared twice"),
            Self::UnknownClass(id) => write!(f, "class `{id}` isn't registered"),
            Self::DuplicateProperty { class, property } => {
                write!(f, "class `{class}` has property `{property}` twice")
            }
            Self::InvalidTypeClass { class, type_class } => {
                write!(f, "class `{class}` can't be typed by `{type_class}`")
            }
            Self::OverrideMismatch {
                instance_class,
                type_class,
                property,
            } => write!(
                f,
                "property `{property}` has different data types in `{instance_class}` \
                 and its type class `{type_class}`"
            ),
        }
    }
}

impl std::error::Error for SchemaError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_core_base_classes_are_registered() {
        let registry = SchemaRegistry::new();
        let level = ClassId::SPATIAL_LOCATION_ELEMENT;
        assert!(registry.is_a(&level, &ClassId::GEOMETRIC_ELEMENT_3D));
        assert!(registry.is_a(&level, &ClassId::ELEMENT));
        assert!(!registry.is_a(&ClassId::ELEMENT, &level));
        assert_eq!(
            registry.class(&ClassId::MULTI_ASPECT).unwrap().kind(),
            ClassKind::MultiAspect
        );
        assert_eq!(registry.schemas().count(), 1);
    }

    #[test]
    fn names_are_identifiers() {
        assert!(is_identifier("Column"));
        assert!(is_identifier("Pset_ColumnCommon"));
        assert!(!is_identifier(""));
        assert!(!is_identifier("2D"));
        assert!(!is_identifier("Core:Element"));
        assert!(!is_identifier("Columna Rectangular"));
    }
}
