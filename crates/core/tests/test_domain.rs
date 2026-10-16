//! A test domain that defines its element types and validation rules, using
//! only the core's public API: adding a discipline needs no change to the
//! core (stories #14 and #20).

use std::panic::{self, AssertUnwindSafe};

use hito_core::{
    Access, Aspect, ClassDef, ClassId, ClassKind, Code, CodeSpec, Command, DataType, Dimension,
    Element, ElementId, ElementStore, Model, ModelId, ModelKind, Rule, Schema, SchemaError,
    SchemaRegistry, SchemaVersion, StoreError, Transaction, Value, Violation,
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

/// The test domain's validation rules. Each returns a message when an
/// element breaks it.
fn rules() -> [Rule; 2] {
    let positive_height = Rule::new("PositiveHeight", class("Column"), |_, column| match column
        .properties()
        .get("Height")
    {
        Some(Value::Real(height)) if *height <= 0.0 => Err(format!("height is {height} m")),
        _ => Ok(()),
    });
    // Reads the value that applies, which may come from the column's type.
    let minimum_strength = Rule::new(
        "MinimumStrength",
        class("Column"),
        |store, column| match store.property(column.id(), "ConcreteStrength") {
            Some(Value::Real(strength)) if *strength < 20e6 => {
                Err(format!("concrete strength is {} MPa", strength / 1e6))
            }
            _ => Ok(()),
        },
    );
    [positive_height, minimum_strength]
}

/// What the app does at startup: the core schema, then each domain's schema
/// and rules.
fn registry() -> SchemaRegistry {
    let mut registry = SchemaRegistry::new();
    registry.register(schema()).unwrap();
    for rule in rules() {
        registry.add_rule(rule).unwrap();
    }
    registry
}

struct Project {
    store: ElementStore,
    physical: ModelId,
    definitions: ModelId,
}

fn project() -> Project {
    let mut store = ElementStore::new(registry());
    let mut tx = store.transaction();
    let physical = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
    let definitions = tx.insert_model(Model::new(ModelKind::Definition)).unwrap();
    tx.commit().unwrap();
    Project {
        store,
        physical,
        definitions,
    }
}

impl Project {
    /// Makes a change in a transaction, as a user command would: commits it
    /// if every step succeeds, otherwise rolls it all back.
    fn change<T>(
        &mut self,
        change: impl FnOnce(&mut Transaction) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let mut tx = self.store.transaction();
        let result = change(&mut tx)?;
        tx.commit()?;
        Ok(result)
    }

    fn insert(&mut self, element: Element) -> Result<ElementId, StoreError> {
        self.change(|tx| tx.insert(element))
    }

    fn set_property(&mut self, id: ElementId, name: &str, value: f64) -> Result<(), StoreError> {
        self.change(|tx| tx.set_property(id, name, value))
    }

    /// A 30 × 40 cm column type in H-25 concrete (25 MPa).
    fn column_type(&mut self) -> ElementId {
        let column_type = Element::new(class("ColumnType"), self.definitions)
            .with_property("Width", 0.30)
            .with_property("Depth", 0.40)
            .with_property("ConcreteStrength", 25e6)
            .with_property("Grade", "H-25");
        self.insert(column_type).unwrap()
    }

    fn column(&mut self, type_id: ElementId) -> ElementId {
        let column = Element::new(class("Column"), self.physical)
            .with_type(type_id)
            .with_property("Height", 3.0);
        self.insert(column).unwrap()
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
        p.insert(wrong),
        Err(StoreError::WrongValueType {
            class: class("Column"),
            property: "Height".into()
        })
    );
    let undeclared = Element::new(class("Column"), p.physical).with_property("Colour", "red");
    assert!(matches!(
        p.insert(undeclared),
        Err(StoreError::UnknownProperty { .. })
    ));
    let aspect_as_element = Element::new(class("Note"), p.physical);
    assert_eq!(
        p.insert(aspect_as_element),
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
    p.set_property(column_type, "Width", 0.35).unwrap();
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

    p.set_property(c1, "ConcreteStrength", 30e6).unwrap();
    assert_eq!(
        p.store.property(c1, "ConcreteStrength"),
        Some(&Value::Real(30e6))
    );
    assert_eq!(
        p.store.property(c2, "ConcreteStrength"),
        Some(&Value::Real(25e6))
    );

    // Removing the override brings back the type's value.
    p.change(|tx| tx.clear_property(c1, "ConcreteStrength"))
        .unwrap();
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
        p.set_property(c1, "Width", 0.5),
        Err(StoreError::UnknownProperty { .. })
    ));
}

#[test]
fn an_element_type_must_be_of_its_class_type_class() {
    let mut p = project();
    let level = Element::new(class("Level"), p.physical).with_property("Elevation", 0.0);
    let level = p.insert(level).unwrap();
    let column_type = p.column_type();

    let wrong_type = Element::new(class("Column"), p.physical).with_type(level);
    assert!(matches!(
        p.insert(wrong_type),
        Err(StoreError::InvalidType { .. })
    ));
    let untyped_class = Element::new(class("Level"), p.physical).with_type(column_type);
    assert!(matches!(
        p.insert(untyped_class),
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
    let column = p.insert(column).unwrap();
    let element = p.store.get(column).unwrap();
    assert_eq!(element.aspects_of(&class("Note")).count(), 2);
    assert_eq!(element.aspects_of(&class("Reinforcement")).count(), 1);

    let two_unique = Element::new(class("Column"), p.physical)
        .with_aspect(Aspect::new(class("Reinforcement")))
        .with_aspect(Aspect::new(class("Reinforcement")));
    assert_eq!(
        p.insert(two_unique),
        Err(StoreError::DuplicateAspect(class("Reinforcement")))
    );
    let wrong_unit = Element::new(class("Column"), p.physical)
        .with_aspect(Aspect::new(class("Reinforcement")).with_property("Cover", "25 mm"));
    assert!(matches!(
        p.insert(wrong_unit),
        Err(StoreError::WrongValueType { .. })
    ));
}

#[test]
fn codes_are_unique_in_their_scope_and_renaming_keeps_the_id() {
    let mut p = project();
    let level = p.insert(Element::new(class("Level"), p.physical)).unwrap();
    let mark = CodeSpec::new("TestStructural:Mark");
    let c12 = Code::new(mark.clone(), level, "C12").unwrap();
    let c13 = Code::new(mark, level, "C13").unwrap();

    let column = Element::new(class("Column"), p.physical).with_code(c12.clone());
    let column = p.insert(column).unwrap();
    assert_eq!(p.store.find_by_code(&c12), Some(column));

    let clash = Element::new(class("Column"), p.physical).with_code(c12.clone());
    assert_eq!(
        p.insert(clash),
        Err(StoreError::CodeInUse {
            code: c12.clone(),
            element: column
        })
    );

    p.change(|tx| tx.set_code(column, Some(c13.clone())))
        .unwrap();
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
        p.insert(Element::new(class("Column"), elsewhere)),
        Err(StoreError::UnknownModel(elsewhere))
    );
}

// Transactions (#20). The model can't be mutated outside a transaction: the
// store itself has no methods that change it (see the `compile_fail` example
// on `ElementStore`).

#[test]
fn changes_apply_only_when_committed() {
    let mut p = project();
    let column_type = p.column_type();

    let mut tx = p.store.transaction();
    let model = tx.insert_model(Model::new(ModelKind::Analytical)).unwrap();
    let column = Element::new(class("Column"), p.physical).with_type(column_type);
    let column = tx.insert(column).unwrap();
    tx.set_property(column_type, "Width", 0.50).unwrap();
    // The transaction sees its own changes.
    assert_eq!(tx.property(column, "Width"), Some(&Value::Real(0.50)));
    drop(tx);

    assert!(p.store.model(model).is_none());
    assert!(p.store.get(column).is_none());
    assert_eq!(
        p.store.property(column_type, "Width"),
        Some(&Value::Real(0.30))
    );

    let mut tx = p.store.transaction();
    let column = tx
        .insert(Element::new(class("Column"), p.physical))
        .unwrap();
    tx.commit().unwrap();
    assert!(p.store.get(column).is_some());
}

#[test]
fn a_failed_step_rolls_back_the_whole_transaction() {
    let mut p = project();
    let column_type = p.column_type();
    let before = p.store.clone();
    let physical = p.physical;

    let result = p.change(|tx| {
        let column = Element::new(class("Column"), physical).with_type(column_type);
        let column = tx.insert(column)?;
        tx.set_property(column_type, "Depth", 0.60)?;
        tx.set_property(column, "Height", "tall")?;
        Ok(column)
    });

    assert!(matches!(result, Err(StoreError::WrongValueType { .. })));
    assert_same_contents(&p.store, &before);
}

#[test]
fn a_broken_rule_rolls_back_the_whole_transaction() {
    let mut p = project();
    let column_type = p.column_type();
    let level = p.insert(Element::new(class("Level"), p.physical)).unwrap();
    let mark = CodeSpec::new("TestStructural:Mark");
    let c1 = Code::new(mark.clone(), level, "C1").unwrap();
    let c2 = Code::new(mark, level, "C2").unwrap();
    let column = Element::new(class("Column"), p.physical)
        .with_type(column_type)
        .with_code(c1.clone());
    let column = p.insert(column).unwrap();
    let before = p.store.clone();

    let mut tx = p.store.transaction();
    tx.set_code(column, Some(c2.clone())).unwrap();
    tx.set_property(column, "Height", -3.0).unwrap();
    tx.insert_model(Model::new(ModelKind::Drawing)).unwrap();
    let error = tx.commit().unwrap_err();

    assert_eq!(
        error,
        StoreError::Violations(vec![Violation {
            rule: "PositiveHeight".into(),
            element: column,
            message: "height is -3 m".into(),
        }])
    );
    assert_same_contents(&p.store, &before);
    assert_eq!(p.store.find_by_code(&c1), Some(column));
    assert_eq!(p.store.find_by_code(&c2), None);
}

#[test]
fn rules_see_the_whole_store_and_every_violation_is_reported() {
    let mut p = project();
    let weak_type =
        Element::new(class("ColumnType"), p.definitions).with_property("ConcreteStrength", 15e6);
    let weak_type = p.insert(weak_type).unwrap();

    let mut tx = p.store.transaction();
    let column = Element::new(class("Column"), p.physical)
        .with_type(weak_type)
        .with_property("Height", 0.0);
    let column = tx.insert(column).unwrap();
    let Err(StoreError::Violations(violations)) = tx.commit() else {
        panic!("the commit should fail");
    };
    let broken: Vec<_> = violations.iter().map(|v| v.rule.as_str()).collect();
    assert_eq!(broken, ["PositiveHeight", "MinimumStrength"]);
    assert!(violations.iter().all(|v| v.element == column));

    // An instance value that overrides the weak type's satisfies the rule.
    let strong = Element::new(class("Column"), p.physical)
        .with_type(weak_type)
        .with_property("ConcreteStrength", 25e6);
    assert!(p.insert(strong).is_ok());
}

#[test]
fn rules_apply_to_subclasses() {
    let mut registry = registry();
    let extension = Schema::new("TestPrecast", SchemaVersion::new(1, 0, 0))
        .class(ClassDef::new("PrecastColumn", class("Column")));
    registry.register(extension).unwrap();
    let mut store = ElementStore::new(registry);

    let mut tx = store.transaction();
    let model = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
    let precast = ClassId::new("TestPrecast", "PrecastColumn");
    let column = Element::new(precast, model).with_property("Height", -1.0);
    tx.insert(column).unwrap();
    assert!(matches!(tx.commit(), Err(StoreError::Violations(_))));
}

#[test]
fn a_panic_rolls_the_transaction_back() {
    let mut p = project();
    let column_type = p.column_type();
    let before = p.store.clone();

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let mut tx = p.store.transaction();
        tx.set_property(column_type, "Width", 0.45).unwrap();
        tx.insert(Element::new(class("Column"), p.physical))
            .unwrap();
        panic!("a bug in the middle of a command");
    }));

    assert!(result.is_err());
    assert_same_contents(&p.store, &before);
}

#[test]
fn a_panicking_rule_rolls_the_transaction_back() {
    let mut registry = registry();
    let buggy = Rule::new("Buggy", class("Level"), |_, _| panic!("a bug in a rule"));
    registry.add_rule(buggy).unwrap();
    let mut store = ElementStore::new(registry);
    let before = store.clone();

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let mut tx = store.transaction();
        let model = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
        tx.insert(Element::new(class("Level"), model)).unwrap();
        tx.commit()
    }));

    assert!(result.is_err());
    assert_same_contents(&store, &before);
}

#[test]
fn rules_are_only_added_for_registered_classes() {
    let mut registry = registry();
    let unknown = ClassId::new("Missing", "Beam");
    let rule = Rule::new("Orphan", unknown.clone(), |_, _| Ok(()));
    assert_eq!(
        registry.add_rule(rule),
        Err(SchemaError::UnknownClass(unknown))
    );
}

#[test]
fn a_commit_returns_a_record_of_what_changed() {
    let mut p = project();
    let column_type = p.column_type();
    let level = p.insert(Element::new(class("Level"), p.physical)).unwrap();
    let type_before = p.store.get(column_type).unwrap().clone();

    let command = Command::new("PlaceColumn").with_parameter("Height", 3.0);
    let mut tx = p.store.transaction_for(command.clone());
    let model = tx.insert_model(Model::new(ModelKind::Analytical)).unwrap();
    let column = Element::new(class("Column"), p.physical).with_type(column_type);
    let column = tx.insert(column).unwrap();
    tx.set_property(column_type, "Width", 0.35).unwrap();
    // Changed, then changed back: not part of the record.
    tx.set_property(level, "Elevation", 1.0).unwrap();
    tx.clear_property(level, "Elevation").unwrap();
    let changes = tx.commit().unwrap();

    assert_eq!(changes.command(), Some(&command));
    let models: Vec<_> = changes.models().iter().map(Model::id).collect();
    assert_eq!(models, [model]);

    // One record per element, in the order they were first changed, with
    // whole elements before and after.
    let [created, modified] = changes.elements() else {
        panic!("two elements changed");
    };
    assert_eq!(created.id(), column);
    assert_eq!(created.before(), None);
    assert_eq!(created.after(), p.store.get(column));
    assert_eq!(modified.id(), column_type);
    assert_eq!(modified.before(), Some(&type_before));
    assert_eq!(modified.after(), p.store.get(column_type));
}

#[test]
fn a_transaction_without_a_command_still_records_its_changes() {
    let mut p = project();
    let mut tx = p.store.transaction();
    let level = tx.insert(Element::new(class("Level"), p.physical)).unwrap();
    let changes = tx.commit().unwrap();
    assert_eq!(changes.command(), None);
    assert_eq!(changes.elements()[0].id(), level);
}

/// Checks that a store holds the same models, elements and codes as another.
fn assert_same_contents(store: &ElementStore, expected: &ElementStore) {
    let elements = |store: &ElementStore| {
        let mut elements: Vec<_> = store.elements().cloned().collect();
        elements.sort_by_key(Element::id);
        elements
    };
    let models = |store: &ElementStore| {
        let mut models: Vec<_> = store.models().map(Model::id).collect();
        models.sort();
        models
    };
    assert_eq!(elements(store), elements(expected));
    assert_eq!(models(store), models(expected));
    for element in expected.elements() {
        if let Some(code) = element.code() {
            assert_eq!(store.find_by_code(code), Some(element.id()));
        }
    }
}
