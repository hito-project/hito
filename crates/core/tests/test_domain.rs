//! A test domain that defines its element types through a schema, using only
//! the core's public API: adding a discipline needs no change to the core
//! (story #14).

use hito_core::{
    Access, Aspect, ClassDef, ClassId, ClassKind, Code, CodeSpec, DataType, Dimension, Element,
    ElementId, ElementStore, Model, ModelId, ModelKind, Schema, SchemaError, SchemaRegistry,
    SchemaVersion, StoreError, Value,
};

const SCHEMA: &str = "TestStructural";
const LENGTH: DataType = DataType::Quantity(Dimension::LENGTH);
const STRESS: DataType = DataType::Quantity(Dimension::PRESSURE);

fn class(name: &str) -> ClassId {
    ClassId::new(SCHEMA, name)
}

/// The test domain's schema, as a domain crate would publish it.
fn schema() -> Schema {
    Schema::new(SCHEMA, SchemaVersion::new(1, 0, 0))
        .class(
            ClassDef::new("Level", ClassId::SPATIAL_LOCATION_ELEMENT).property("Elevation", LENGTH),
        )
        .class(
            ClassDef::new("ColumnType", ClassId::DEFINITION_ELEMENT)
                .property("Width", LENGTH)
                .property("Depth", LENGTH)
                .property("ConcreteStrength", STRESS)
                .property("Grade", DataType::Text),
        )
        .class(
            ClassDef::new("Column", ClassId::GEOMETRIC_ELEMENT_3D)
                .typed_by(class("ColumnType"))
                .property("Height", LENGTH)
                .property("LoadBearing", DataType::Boolean)
                // Same name as on the type: an instance value overrides it.
                .property("ConcreteStrength", STRESS),
        )
        .class(ClassDef::new("Reinforcement", ClassId::UNIQUE_ASPECT).property("Cover", LENGTH))
        .class(ClassDef::new("Note", ClassId::MULTI_ASPECT).property("Text", DataType::Text))
}

/// What the app does at startup: the core schema, then each domain's.
fn registry() -> SchemaRegistry {
    let mut registry = SchemaRegistry::new();
    registry.register(schema()).unwrap();
    registry
}

struct Project {
    store: ElementStore,
    physical: ModelId,
    definitions: ModelId,
}

fn project() -> Project {
    let mut store = ElementStore::new(registry());
    let physical = store.insert_model(Model::new(ModelKind::Physical)).unwrap();
    let definitions = store
        .insert_model(Model::new(ModelKind::Definition))
        .unwrap();
    Project {
        store,
        physical,
        definitions,
    }
}

impl Project {
    /// A 30 × 40 cm column type in H-25 concrete (25 MPa).
    fn column_type(&mut self) -> ElementId {
        let column_type = Element::new(class("ColumnType"), self.definitions)
            .with_property("Width", 0.30)
            .with_property("Depth", 0.40)
            .with_property("ConcreteStrength", 25e6)
            .with_property("Grade", "H-25");
        self.store.insert(column_type).unwrap()
    }

    fn column(&mut self, type_id: ElementId) -> ElementId {
        let column = Element::new(class("Column"), self.physical)
            .with_type(type_id)
            .with_property("Height", 3.0);
        self.store.insert(column).unwrap()
    }
}

// A schema declares element classes, inheritance from core base classes, and
// typed properties with units.

#[test]
fn classes_derive_from_core_base_classes() {
    let registry = registry();
    assert!(registry.is_a(&class("Column"), &ClassId::GEOMETRIC_ELEMENT_3D));
    assert!(registry.is_a(&class("Column"), &ClassId::ELEMENT));
    assert!(registry.is_a(&class("ColumnType"), &ClassId::DEFINITION_ELEMENT));
    assert!(registry.is_a(&class("Level"), &ClassId::SPATIAL_LOCATION_ELEMENT));
    assert!(!registry.is_a(&class("Column"), &ClassId::DEFINITION_ELEMENT));

    let kind = |name| registry.class(&class(name)).unwrap().kind();
    assert_eq!(kind("Column"), ClassKind::Element);
    assert_eq!(kind("Reinforcement"), ClassKind::UniqueAspect);
    assert_eq!(kind("Note"), ClassKind::MultiAspect);
}

#[test]
fn properties_have_data_types_with_physical_dimensions() {
    let registry = registry();
    let data_type = |c, p| registry.property(&class(c), p).unwrap().data_type();
    assert_eq!(data_type("Column", "Height"), LENGTH);
    assert_eq!(data_type("ColumnType", "ConcreteStrength"), STRESS);
    assert_eq!(data_type("Column", "LoadBearing"), DataType::Boolean);
    assert!(registry.property(&class("Column"), "Width").is_none());
}

#[test]
fn subclasses_inherit_properties_and_the_type_class() {
    let mut registry = registry();
    let extension = Schema::new("TestPrecast", SchemaVersion::new(1, 0, 0)).class(
        ClassDef::new("PrecastColumn", class("Column")).property("Lifting", DataType::Boolean),
    );
    registry.register(extension).unwrap();

    let precast = ClassId::new("TestPrecast", "PrecastColumn");
    assert!(registry.is_a(&precast, &ClassId::GEOMETRIC_ELEMENT_3D));
    assert!(registry.property(&precast, "Height").is_some());
    let names: Vec<_> = registry.properties(&precast).map(|p| p.name()).collect();
    assert_eq!(
        names,
        ["Lifting", "Height", "LoadBearing", "ConcreteStrength"]
    );
    assert_eq!(
        registry.class(&precast).unwrap().type_class(),
        Some(&class("ColumnType"))
    );
}

#[test]
fn values_must_match_the_declared_data_type() {
    let mut p = project();
    let wrong = Element::new(class("Column"), p.physical).with_property("Height", "3 m");
    assert_eq!(
        p.store.insert(wrong),
        Err(StoreError::WrongValueType {
            class: class("Column"),
            property: "Height".into()
        })
    );
    let undeclared = Element::new(class("Column"), p.physical).with_property("Colour", "red");
    assert!(matches!(
        p.store.insert(undeclared),
        Err(StoreError::UnknownProperty { .. })
    ));
    let aspect_as_element = Element::new(class("Note"), p.physical);
    assert_eq!(
        p.store.insert(aspect_as_element),
        Err(StoreError::WrongClassKind(class("Note")))
    );
}

// Schemas are versioned and registered at startup. The core has no knowledge
// of specific domains.

#[test]
fn registered_schemas_keep_their_versions() {
    let registry = registry();
    let schemas: Vec<_> = registry.schemas().collect();
    assert_eq!(
        schemas,
        [
            ("Core", SchemaRegistry::CORE_VERSION),
            (SCHEMA, SchemaVersion::new(1, 0, 0))
        ]
    );
    // A file written by version 1.0.4 of the domain is still writable.
    let installed = registry.schema_version(SCHEMA).unwrap();
    assert_eq!(
        installed.access(SchemaVersion::new(1, 0, 4)),
        Access::ReadWrite
    );
    assert_eq!(
        installed.access(SchemaVersion::new(1, 1, 0)),
        Access::ReadOnly
    );
}

#[test]
fn a_schema_registers_once() {
    let mut registry = registry();
    assert_eq!(
        registry.register(schema()),
        Err(SchemaError::DuplicateSchema(SCHEMA.into()))
    );
    let core = Schema::new("Core", SchemaVersion::new(9, 0, 0));
    assert!(registry.register(core).is_err());
}

#[test]
fn an_invalid_schema_registers_nothing() {
    let mut registry = registry();
    let broken = Schema::new("Broken", SchemaVersion::new(1, 0, 0))
        .class(ClassDef::new("Fine", ClassId::GEOMETRIC_ELEMENT_3D))
        .class(ClassDef::new("Orphan", ClassId::new("Missing", "Base")));
    assert_eq!(
        registry.register(broken),
        Err(SchemaError::UnknownClass(ClassId::new("Missing", "Base")))
    );
    assert!(registry.class(&ClassId::new("Broken", "Fine")).is_none());
    assert!(registry.schema_version("Broken").is_none());
}

#[test]
fn schemas_are_checked_when_registered() {
    let check = |class_def: ClassDef| {
        let mut registry = registry();
        let schema = Schema::new("Other", SchemaVersion::new(1, 0, 0)).class(class_def);
        registry.register(schema).unwrap_err()
    };
    let inherited = ClassDef::new("Tall", class("Column")).property("Height", LENGTH);
    assert!(matches!(
        check(inherited),
        SchemaError::DuplicateProperty { .. }
    ));

    let not_a_definition =
        ClassDef::new("Odd", ClassId::GEOMETRIC_ELEMENT_3D).typed_by(class("Column"));
    assert!(matches!(
        check(not_a_definition),
        SchemaError::InvalidTypeClass { .. }
    ));

    // A column-type subclass whose Height clashes with the column's Height.
    let clash = ClassDef::new("OddType", class("ColumnType")).property("Height", DataType::Text);
    assert!(matches!(check(clash), SchemaError::OverrideMismatch { .. }));

    assert!(matches!(
        check(ClassDef::new("Has space", ClassId::ELEMENT)),
        SchemaError::InvalidName(_)
    ));
}

// Type and instance parameters are supported: a type's parameters are shared
// by all its instances.

#[test]
fn instances_share_their_types_values() {
    let mut p = project();
    let column_type = p.column_type();
    let c1 = p.column(column_type);
    let c2 = p.column(column_type);

    for column in [c1, c2] {
        assert_eq!(p.store.property(column, "Width"), Some(&Value::Real(0.30)));
        assert_eq!(p.store.property(column, "Height"), Some(&Value::Real(3.0)));
    }

    // Editing the type changes every instance.
    p.store.set_property(column_type, "Width", 0.35).unwrap();
    for column in [c1, c2] {
        assert_eq!(p.store.property(column, "Width"), Some(&Value::Real(0.35)));
    }
}

#[test]
fn an_instance_value_overrides_the_types() {
    let mut p = project();
    let column_type = p.column_type();
    let c1 = p.column(column_type);
    let c2 = p.column(column_type);

    p.store.set_property(c1, "ConcreteStrength", 30e6).unwrap();
    assert_eq!(
        p.store.property(c1, "ConcreteStrength"),
        Some(&Value::Real(30e6))
    );
    assert_eq!(
        p.store.property(c2, "ConcreteStrength"),
        Some(&Value::Real(25e6))
    );

    // Removing the override brings back the type's value.
    p.store.clear_property(c1, "ConcreteStrength").unwrap();
    assert_eq!(
        p.store.property(c1, "ConcreteStrength"),
        Some(&Value::Real(25e6))
    );
}

#[test]
fn type_only_values_cant_be_set_on_an_instance() {
    let mut p = project();
    let column_type = p.column_type();
    let c1 = p.column(column_type);
    assert!(matches!(
        p.store.set_property(c1, "Width", 0.5),
        Err(StoreError::UnknownProperty { .. })
    ));
}

#[test]
fn an_element_type_must_be_of_its_class_type_class() {
    let mut p = project();
    let level = Element::new(class("Level"), p.physical).with_property("Elevation", 0.0);
    let level = p.store.insert(level).unwrap();
    let column_type = p.column_type();

    let wrong_type = Element::new(class("Column"), p.physical).with_type(level);
    assert!(matches!(
        p.store.insert(wrong_type),
        Err(StoreError::InvalidType { .. })
    ));
    let untyped_class = Element::new(class("Level"), p.physical).with_type(column_type);
    assert!(matches!(
        p.store.insert(untyped_class),
        Err(StoreError::InvalidType { .. })
    ));
}

// Aspects, codes and models, from the element model.

#[test]
fn aspects_add_checked_data_to_elements() {
    let mut p = project();
    let column = Element::new(class("Column"), p.physical)
        .with_aspect(Aspect::new(class("Reinforcement")).with_property("Cover", 0.025))
        .with_aspect(Aspect::new(class("Note")).with_property("Text", "check"))
        .with_aspect(Aspect::new(class("Note")).with_property("Text", "recheck"));
    let column = p.store.insert(column).unwrap();
    let element = p.store.get(column).unwrap();
    assert_eq!(element.aspects_of(&class("Note")).count(), 2);
    assert_eq!(element.aspects_of(&class("Reinforcement")).count(), 1);

    let two_unique = Element::new(class("Column"), p.physical)
        .with_aspect(Aspect::new(class("Reinforcement")))
        .with_aspect(Aspect::new(class("Reinforcement")));
    assert_eq!(
        p.store.insert(two_unique),
        Err(StoreError::DuplicateAspect(class("Reinforcement")))
    );
    let wrong_unit = Element::new(class("Column"), p.physical)
        .with_aspect(Aspect::new(class("Reinforcement")).with_property("Cover", "25 mm"));
    assert!(matches!(
        p.store.insert(wrong_unit),
        Err(StoreError::WrongValueType { .. })
    ));
}

#[test]
fn codes_are_unique_in_their_scope_and_renaming_keeps_the_id() {
    let mut p = project();
    let level = p
        .store
        .insert(Element::new(class("Level"), p.physical))
        .unwrap();
    let mark = CodeSpec::new("TestStructural:Mark");
    let c12 = Code::new(mark.clone(), level, "C12").unwrap();
    let c13 = Code::new(mark, level, "C13").unwrap();

    let column = Element::new(class("Column"), p.physical).with_code(c12.clone());
    let column = p.store.insert(column).unwrap();
    assert_eq!(p.store.find_by_code(&c12), Some(column));

    let clash = Element::new(class("Column"), p.physical).with_code(c12.clone());
    assert_eq!(
        p.store.insert(clash),
        Err(StoreError::CodeInUse {
            code: c12.clone(),
            element: column
        })
    );

    p.store.set_code(column, Some(c13.clone())).unwrap();
    assert_eq!(p.store.find_by_code(&c13), Some(column));
    assert_eq!(p.store.find_by_code(&c12), None);
    assert_eq!(p.store.get(column).unwrap().id(), column);
}

#[test]
fn every_element_lives_in_an_existing_model() {
    let mut p = project();
    let column = p.column_type();
    assert_eq!(p.store.get(column).unwrap().model(), p.definitions);
    assert_eq!(
        p.store.model(p.definitions).unwrap().kind(),
        ModelKind::Definition
    );

    let elsewhere = Model::new(ModelKind::Physical).id();
    assert_eq!(
        p.store.insert(Element::new(class("Column"), elsewhere)),
        Err(StoreError::UnknownModel(elsewhere))
    );
}
