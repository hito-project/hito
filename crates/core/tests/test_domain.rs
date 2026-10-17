//! A test domain that defines its element types, relationships, recompute
//! functions and validation rules, using only the core's public API: adding
//! a discipline needs no change to the core (stories #14, #16 and #20).

use std::mem;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use hito_core::{
    Access, Aspect, ChangeSet, ChangedField, ClassDef, ClassId, ClassKind, Code, CodeSpec, Command,
    DataType, Dimension, Element, ElementChange, ElementId, ElementStore, FieldChange, Model,
    ModelId, ModelKind, OnDelete, Origin, Properties, Recompute, Relationship, RelationshipChange,
    RelationshipDef, Rule, Schema, SchemaError, SchemaRegistry, SchemaVersion, Severity,
    StoreError, StoreView, Transaction, Value, Violation,
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
                .property("ConcreteStrength", STRESS)
                .property("BaseOffset", LENGTH)
                // Recomputed from the level and the offset.
                .property("BaseElevation", LENGTH),
        )
        .class(ClassDef::new("Beam", ClassId::GEOMETRIC_ELEMENT_3D).property("Elevation", LENGTH))
        .class(ClassDef::new("Slab", ClassId::GEOMETRIC_ELEMENT_3D).property("Thickness", LENGTH))
        .class(ClassDef::new("Opening", ClassId::GEOMETRIC_ELEMENT_3D).property("Width", LENGTH))
        .class(ClassDef::new("Reinforcement", ClassId::UNIQUE_ASPECT).property("Cover", LENGTH))
        .class(ClassDef::new("Note", ClassId::MULTI_ASPECT).property("Text", DataType::Text))
        // A column stands on a level, and follows its elevation. A level in
        // use can't be deleted.
        .relationship(
            RelationshipDef::new("AttachedToLevel", class("Column"), class("Level"))
                .driving()
                .on_delete(OnDelete::Block),
        )
        // A beam frames into columns or other beams, and follows their tops.
        // It stays when one of them is deleted.
        .relationship(
            RelationshipDef::new("FramesInto", class("Beam"), ClassId::GEOMETRIC_ELEMENT_3D)
                .driving()
                .on_delete(OnDelete::Detach),
        )
        // An opening is deleted with its slab.
        .relationship(
            RelationshipDef::new("HostedBy", class("Opening"), class("Slab"))
                .on_delete(OnDelete::Cascade),
        )
}

fn real(store: &StoreView, id: ElementId, name: &str) -> Option<f64> {
    match store.property(id, name) {
        Some(Value::Real(x)) => Some(*x),
        _ => None,
    }
}

/// The test domain's recompute functions: how a column and a beam follow
/// the elements they depend on.
fn recomputes() -> [Recompute; 2] {
    let column = Recompute::new(class("Column"), |store, column| {
        let mut values = Properties::new();
        let level = store
            .relationships_from(column.id())
            .find(|r| r.class() == &class("AttachedToLevel"));
        if let Some(level) = level {
            let elevation = real(store, level.target(), "Elevation").unwrap_or(0.0);
            let offset = real(store, column.id(), "BaseOffset").unwrap_or(0.0);
            values.insert("BaseElevation".into(), (elevation + offset).into());
        }
        values
    });
    // A beam sits on the highest of its supports.
    let beam = Recompute::new(class("Beam"), |store, beam| {
        let top = |id| {
            if store.get(id)?.class() == &class("Column") {
                Some(real(store, id, "BaseElevation")? + real(store, id, "Height")?)
            } else {
                real(store, id, "Elevation")
            }
        };
        let supports = store
            .relationships_from(beam.id())
            .filter(|r| r.class() == &class("FramesInto"));
        let elevation = supports.filter_map(|r| top(r.target())).reduce(f64::max);
        let mut values = Properties::new();
        if let Some(elevation) = elevation {
            values.insert("Elevation".into(), elevation.into());
        }
        values
    });
    [column, beam]
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

/// What the app does at startup: the core schema, then each domain's schema,
/// recompute functions and rules.
fn registry() -> SchemaRegistry {
    let mut registry = SchemaRegistry::new();
    registry.register(schema()).unwrap();
    for recompute in recomputes() {
        registry.add_recompute(recompute).unwrap();
    }
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
    project_with(registry())
}

fn project_with(registry: SchemaRegistry) -> Project {
    let mut store = ElementStore::new(registry);
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
        [
            "Lifting",
            "Height",
            "LoadBearing",
            "ConcreteStrength",
            "BaseOffset",
            "BaseElevation"
        ]
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
    assert_eq!(
        tx.view().property(column, "Width"),
        Some(&Value::Real(0.50))
    );
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
            severity: Severity::Error,
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

    // One record per element, in the order they were first changed: the
    // whole new element for a creation, only what changed for a change.
    assert_eq!(
        changes.elements(),
        [
            ElementChange::Created {
                element: p.store.get(column).unwrap().clone(),
                origin: Origin::User,
            },
            ElementChange::Modified {
                id: column_type,
                fields: vec![by_user(FieldChange::Property {
                    name: "Width".into(),
                    before: type_before.properties().get("Width").cloned(),
                    after: Some(Value::Real(0.35)),
                })],
            },
        ]
    );
}

fn by_user(change: FieldChange) -> ChangedField {
    let origin = Origin::User;
    ChangedField { change, origin }
}

fn propagated(change: FieldChange) -> ChangedField {
    let origin = Origin::Propagated;
    ChangedField { change, origin }
}

#[test]
fn several_changes_to_a_field_collapse_into_one() {
    let mut p = project();
    let column_type = p.column_type();
    let level = p.insert(Element::new(class("Level"), p.physical)).unwrap();
    let c1 = Code::new(CodeSpec::new("TestStructural:Mark"), level, "C1").unwrap();

    let mut tx = p.store.transaction();
    tx.set_property(column_type, "Width", 0.35).unwrap();
    tx.set_property(column_type, "Width", 0.40).unwrap();
    tx.set_property(column_type, "Depth", 0.50).unwrap();
    tx.set_property(column_type, "Depth", 0.40).unwrap(); // back to where it was
    tx.set_code(column_type, Some(c1.clone())).unwrap();
    let changes = tx.commit().unwrap();

    assert_eq!(
        changes.elements(),
        [ElementChange::Modified {
            id: column_type,
            fields: vec![
                by_user(FieldChange::Property {
                    name: "Width".into(),
                    before: Some(Value::Real(0.30)),
                    after: Some(Value::Real(0.40)),
                }),
                by_user(FieldChange::Code {
                    before: None,
                    after: Some(c1),
                }),
            ],
        }]
    );
}

#[test]
fn a_leaked_transaction_changes_nothing() {
    let mut p = project();
    let column_type = p.column_type();
    let before = p.store.clone();

    let mut tx = p.store.transaction();
    tx.set_property(column_type, "Width", 0.45).unwrap();
    tx.insert(Element::new(class("Column"), p.physical))
        .unwrap();
    tx.insert_model(Model::new(ModelKind::Drawing)).unwrap();
    // Leaking is safe Rust: no rollback runs, yet nothing may show.
    mem::forget(tx);

    assert_same_contents(&p.store, &before);
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

// Relationships (#16).

impl Project {
    fn level(&mut self, elevation: f64) -> ElementId {
        let level =
            Element::new(class("Level"), self.physical).with_property("Elevation", elevation);
        self.insert(level).unwrap()
    }

    /// A 3 m column standing on `level`.
    fn column_on(&mut self, level: ElementId) -> ElementId {
        let physical = self.physical;
        self.change(|tx| {
            let column = Element::new(class("Column"), physical).with_property("Height", 3.0);
            let column = tx.insert(column)?;
            tx.relate(Relationship::new(class("AttachedToLevel"), column, level))?;
            Ok(column)
        })
        .unwrap()
    }

    /// A beam framing into `supports`.
    fn beam_on(&mut self, supports: &[ElementId]) -> ElementId {
        let physical = self.physical;
        self.change(|tx| {
            let beam = tx.insert(Element::new(class("Beam"), physical))?;
            for &support in supports {
                tx.relate(Relationship::new(class("FramesInto"), beam, support))?;
            }
            Ok(beam)
        })
        .unwrap()
    }
}

#[test]
fn domains_declare_relationship_types() {
    let registry = registry();
    let attached = registry.relationship(&class("AttachedToLevel")).unwrap();
    assert_eq!(attached.source(), &class("Column"));
    assert_eq!(attached.target(), &class("Level"));
    assert!(attached.drives());
    assert_eq!(attached.delete_rule(), OnDelete::Block);
    assert!(!registry.relationship(&class("HostedBy")).unwrap().drives());

    let mut p = project();
    let level = p.level(0.0);
    let column = p.column_on(level);
    let attached = Relationship::new(class("AttachedToLevel"), column, level);
    assert_eq!(
        p.store.relationships_from(column).collect::<Vec<_>>(),
        [&attached]
    );
    assert_eq!(
        p.store.relationships_to(level).collect::<Vec<_>>(),
        [&attached]
    );

    let mut tx = p.store.transaction();
    assert_eq!(
        tx.relate(attached.clone()),
        Err(StoreError::DuplicateRelationship(attached.clone()))
    );
    let backwards = Relationship::new(class("AttachedToLevel"), level, column);
    assert_eq!(
        tx.relate(backwards.clone()),
        Err(StoreError::InvalidRelationship(backwards))
    );
    let unknown = Relationship::new(class("Column"), column, level);
    assert_eq!(
        tx.relate(unknown),
        Err(StoreError::UnknownRelationship(class("Column")))
    );
    tx.unrelate(&attached).unwrap();
    assert_eq!(tx.view().relationships_from(column).count(), 0);
}

#[test]
fn relationships_are_checked_when_their_schema_registers() {
    let check = |def: RelationshipDef| {
        let mut registry = registry();
        let schema = Schema::new("Other", SchemaVersion::new(1, 0, 0)).relationship(def);
        registry.register(schema).unwrap_err()
    };
    let missing = ClassId::new("Missing", "Wall");
    assert_eq!(
        check(RelationshipDef::new("On", missing.clone(), class("Level"))),
        SchemaError::UnknownClass(missing)
    );
    let to_an_aspect = RelationshipDef::new("Noted", class("Column"), class("Note"));
    assert_eq!(
        check(to_an_aspect),
        SchemaError::InvalidRelationship(ClassId::new("Other", "Noted"))
    );
}

#[test]
fn a_change_recomputes_its_dependants_in_order() {
    let mut p = project();
    let level = p.level(3.0);
    let column = p.column_on(level);
    let beam = p.beam_on(&[column]);
    assert_eq!(real(&p.store.view(), column, "BaseElevation"), Some(3.0));
    assert_eq!(real(&p.store.view(), beam, "Elevation"), Some(6.0));

    // A beam on both the level and the first beam: it's one step from the
    // level but three from the column, and must wait for the first beam.
    let upper = p.beam_on(&[level, beam]);
    assert_eq!(real(&p.store.view(), upper, "Elevation"), Some(6.0));

    // The level drives the column, which drives the beam: the beam must see
    // the column's new base.
    p.set_property(level, "Elevation", 3.5).unwrap();
    assert_eq!(real(&p.store.view(), column, "BaseElevation"), Some(3.5));
    assert_eq!(real(&p.store.view(), beam, "Elevation"), Some(6.5));
    assert_eq!(real(&p.store.view(), upper, "Elevation"), Some(6.5));

    // Changing the column itself recomputes it and its beams.
    p.set_property(column, "BaseOffset", -0.5).unwrap();
    assert_eq!(real(&p.store.view(), column, "BaseElevation"), Some(3.0));
    assert_eq!(real(&p.store.view(), beam, "Elevation"), Some(6.0));
    assert_eq!(real(&p.store.view(), upper, "Elevation"), Some(6.0));
}

#[test]
fn only_the_dependants_of_a_change_are_recomputed() {
    let runs = Arc::new(AtomicUsize::new(0));
    let mut registry = registry();
    let counter = Arc::clone(&runs);
    registry
        .add_recompute(Recompute::new(class("Beam"), move |_, _| {
            counter.fetch_add(1, Ordering::Relaxed);
            Properties::new()
        }))
        .unwrap();
    let mut p = project_with(registry);
    let level = p.level(0.0);
    let other_level = p.level(3.0);
    let column = p.column_on(level);
    p.beam_on(&[column]);
    let before = runs.load(Ordering::Relaxed);

    p.set_property(other_level, "Elevation", 3.2).unwrap();
    assert_eq!(runs.load(Ordering::Relaxed), before);
    p.set_property(level, "Elevation", 0.2).unwrap();
    assert_eq!(runs.load(Ordering::Relaxed), before + 1);
}

#[test]
fn circular_dependencies_are_refused() {
    let mut p = project();
    let level = p.level(0.0);
    let column = p.column_on(level);
    let a = p.beam_on(&[column]);
    let b = p.beam_on(&[a]);
    let c = p.beam_on(&[b]);
    let before = p.store.clone();

    let closing = Relationship::new(class("FramesInto"), a, c);
    assert_eq!(
        p.change(|tx| tx.relate(closing)),
        Err(StoreError::CircularDependency(vec![a, c, b, a]))
    );
    let onto_itself = Relationship::new(class("FramesInto"), a, a);
    assert_eq!(
        p.change(|tx| tx.relate(onto_itself)),
        Err(StoreError::CircularDependency(vec![a, a]))
    );
    assert_same_contents(&p.store, &before);
}

#[test]
fn deleting_an_element_deletes_what_it_owns() {
    let mut p = project();
    let slab = p.insert(Element::new(class("Slab"), p.physical)).unwrap();
    let physical = p.physical;
    let opening = p
        .change(|tx| {
            let opening = tx.insert(Element::new(class("Opening"), physical))?;
            tx.relate(Relationship::new(class("HostedBy"), opening, slab))?;
            Ok(opening)
        })
        .unwrap();

    let mut deleted = p.change(|tx| tx.delete(slab)).unwrap();
    deleted.sort();
    let mut expected = vec![slab, opening];
    expected.sort();
    assert_eq!(deleted, expected);
    assert!(p.store.get(opening).is_none());
    assert_eq!(p.store.relationships_to(slab).count(), 0);
}

#[test]
fn deleting_an_element_detaches_what_refers_to_it() {
    let mut p = project();
    let ground = p.level(0.0);
    let first = p.level(3.0);
    let short = p.column_on(ground);
    let tall = p.column_on(first);
    let beam = p.beam_on(&[short, tall]);
    assert_eq!(real(&p.store.view(), beam, "Elevation"), Some(6.0));

    p.change(|tx| tx.delete(tall)).unwrap();
    assert!(p.store.get(beam).is_some());
    let supports: Vec<_> = p
        .store
        .relationships_from(beam)
        .map(|r| r.target())
        .collect();
    assert_eq!(supports, [short]);
    // The beam was recomputed from the support it has left.
    assert_eq!(real(&p.store.view(), beam, "Elevation"), Some(3.0));
}

#[test]
fn deleting_an_element_others_depend_on_can_be_blocked() {
    let mut p = project();
    let level = p.level(0.0);
    let column = p.column_on(level);
    let before = p.store.clone();

    let blocked = Err(StoreError::DeleteBlocked {
        element: level,
        relationship: Relationship::new(class("AttachedToLevel"), column, level),
    });
    assert_eq!(p.store.preview_delete(level), blocked);
    assert_eq!(p.change(|tx| tx.delete(level)), blocked);
    assert_same_contents(&p.store, &before);

    // Deleting the column first frees the level.
    p.change(|tx| {
        tx.delete(column)?;
        tx.delete(level)
    })
    .unwrap();
    assert!(p.store.get(level).is_none());
}

#[test]
fn deleting_a_type_deletes_its_instances_and_applies_their_rules() {
    let mut p = project();
    let column_type = p.column_type();
    let c1 = p.column(column_type);
    let c2 = p.column(column_type);
    let other_type = p.column_type();
    let other = p.column(other_type);
    // A beam framing into c1 is detached when c1 goes, not deleted.
    let beam = p.beam_on(&[c1, other]);

    let mut deleted = p.change(|tx| tx.delete(column_type)).unwrap();
    deleted.sort();
    let mut expected = vec![column_type, c1, c2];
    expected.sort();
    assert_eq!(deleted, expected);
    assert!(p.store.get(other).is_some());
    let supports: Vec<_> = p
        .store
        .relationships_from(beam)
        .map(|r| r.target())
        .collect();
    assert_eq!(supports, [other]);
}

#[test]
fn a_delete_can_be_previewed_without_changing_anything() {
    let mut p = project();
    let column_type = p.column_type();
    let column = p.column(column_type);
    let slab = p.insert(Element::new(class("Slab"), p.physical)).unwrap();
    let physical = p.physical;
    let opening = p
        .change(|tx| {
            let opening = tx.insert(Element::new(class("Opening"), physical))?;
            tx.relate(Relationship::new(class("HostedBy"), opening, slab))?;
            Ok(opening)
        })
        .unwrap();
    let before = p.store.clone();

    // What the UI shows before asking to confirm: the element first, then
    // everything it takes with it.
    assert_eq!(
        p.store.preview_delete(column_type),
        Ok(vec![column_type, column])
    );
    assert_eq!(p.store.preview_delete(slab), Ok(vec![slab, opening]));
    assert_same_contents(&p.store, &before);

    // Inside a transaction, the preview sees the transaction's changes.
    let mut tx = p.store.transaction();
    let column = Element::new(class("Column"), p.physical).with_type(column_type);
    let new_column = tx.insert(column).unwrap();
    let preview = tx.view().preview_delete(column_type).unwrap();
    assert!(preview.contains(&new_column));
}

#[test]
fn a_rolled_back_delete_restores_elements_and_relationships() {
    let mut p = project();
    let level = p.level(0.0);
    let column = p.column_on(level);
    let beam = p.beam_on(&[column]);
    let before = p.store.clone();

    let mut tx = p.store.transaction();
    tx.delete(beam).unwrap();
    tx.delete(column).unwrap();
    assert_eq!(tx.view().relationships_to(level).count(), 0);
    tx.roll_back();

    assert_same_contents(&p.store, &before);
    assert_eq!(p.store.relationships_from(beam).count(), 1);
    assert_eq!(p.store.relationships_to(level).count(), 1);
}

#[test]
fn a_leaked_delete_changes_nothing() {
    let mut p = project();
    let level = p.level(0.0);
    let column = p.column_on(level);
    p.beam_on(&[column]);
    let before = p.store.clone();

    let mut tx = p.store.transaction();
    tx.delete(column).unwrap();
    tx.set_property(level, "Elevation", 4.0).unwrap();
    mem::forget(tx);

    assert_same_contents(&p.store, &before);
}

#[test]
fn changing_a_type_recomputes_and_rechecks_its_instances() {
    let runs = Arc::new(AtomicUsize::new(0));
    let mut registry = registry();
    let counter = Arc::clone(&runs);
    let count_columns = Recompute::new(class("Column"), move |_, _| {
        counter.fetch_add(1, Ordering::Relaxed);
        Properties::new()
    });
    registry.add_recompute(count_columns).unwrap();
    let mut p = project_with(registry);
    let column_type = p.column_type();
    let c1 = p.column(column_type);
    p.column(column_type);
    let before = runs.load(Ordering::Relaxed);

    p.set_property(column_type, "Width", 0.35).unwrap();
    assert_eq!(runs.load(Ordering::Relaxed), before + 2);

    // The instances inherit the weaker concrete, so their rule breaks.
    let Err(StoreError::Violations(violations)) =
        p.set_property(column_type, "ConcreteStrength", 15e6)
    else {
        panic!("the instances should break MinimumStrength");
    };
    assert_eq!(violations.len(), 2);
    assert!(
        violations
            .iter()
            .any(|v| v.element == c1 && v.rule == "MinimumStrength")
    );
}

#[test]
fn warnings_dont_block_the_commit() {
    let mut registry = registry();
    let slender = Rule::new("Slender", class("Column"), |_, column| {
        match column.properties().get("Height") {
            Some(Value::Real(height)) if *height > 4.0 => Err(format!("{height} m is tall")),
            _ => Ok(()),
        }
    })
    .warning();
    assert_eq!(slender.severity(), Severity::Warning);
    registry.add_rule(slender).unwrap();
    let mut p = project_with(registry);
    let column_type = p.column_type();
    let column = p.column(column_type);

    let mut tx = p.store.transaction();
    tx.set_property(column, "Height", 5.0).unwrap();
    let changes = tx.commit().unwrap();
    assert_eq!(
        changes.warnings(),
        [Violation {
            rule: "Slender".into(),
            severity: Severity::Warning,
            element: column,
            message: "5 m is tall".into(),
        }]
    );
    assert_eq!(real(&p.store.view(), column, "Height"), Some(5.0));

    // An error alongside a warning still blocks, and only errors are
    // reported.
    let mut tx = p.store.transaction();
    tx.set_property(column, "Height", -6.0).unwrap();
    tx.set_property(column, "Height", 6.0).unwrap();
    tx.set_property(column, "ConcreteStrength", 10e6).unwrap();
    let Err(StoreError::Violations(violations)) = tx.commit() else {
        panic!("the error should block the commit");
    };
    let broken: Vec<_> = violations.iter().map(|v| v.rule.as_str()).collect();
    assert_eq!(broken, ["MinimumStrength"]);
}

#[test]
fn propagated_changes_are_part_of_the_change_set() {
    let mut p = project();
    let level = p.level(0.0);
    let column = p.column_on(level);
    let beam = p.beam_on(&[column]);
    let before = p.store.get(column).unwrap().clone();

    let mut tx = p.store.transaction();
    tx.set_property(level, "Elevation", 3.0).unwrap();
    let changes = tx.commit().unwrap();
    let changed: Vec<_> = changes.elements().iter().map(|c| c.id()).collect();
    assert_eq!(changed, [level, column, beam]);
    // Only the level's elevation is the user's; the rest followed it.
    let origins: Vec<_> = changes
        .elements()
        .iter()
        .flat_map(|c| match c {
            ElementChange::Modified { fields, .. } => fields.iter().map(|f| f.origin),
            _ => panic!("only modified"),
        })
        .collect();
    assert_eq!(
        origins,
        [Origin::User, Origin::Propagated, Origin::Propagated]
    );

    // A field the user set stays the user's, even if recompute set it too.
    let mut tx = p.store.transaction();
    tx.set_property(column, "BaseElevation", 9.0).unwrap();
    tx.set_property(level, "Elevation", 0.0).unwrap();
    let changes = tx.commit().unwrap();
    let ElementChange::Modified { id, fields } = &changes.elements()[0] else {
        panic!("the column is modified");
    };
    assert_eq!(*id, column);
    assert_eq!(fields[0].origin, Origin::User);
    assert_eq!(
        p.store.property(column, "BaseElevation"),
        Some(&Value::Real(0.0))
    );
    p.set_property(level, "Elevation", 3.0).unwrap();

    let mut tx = p.store.transaction();
    tx.delete(column).unwrap();
    let changes = tx.commit().unwrap();
    // A deletion carries the whole element, as it was before. The user
    // deleted it, and its relationships went with it.
    let deleted = before.with_property("BaseElevation", 3.0);
    assert_eq!(
        changes.elements(),
        [ElementChange::Deleted {
            element: deleted,
            origin: Origin::User,
        }]
    );
    let removed = |relationship| RelationshipChange::Removed {
        relationship,
        origin: Origin::Propagated,
    };
    assert_eq!(
        changes.relationships(),
        [
            removed(Relationship::new(class("AttachedToLevel"), column, level)),
            removed(Relationship::new(class("FramesInto"), beam, column)),
        ]
    );

    // What a deletion takes with it is propagated.
    let slab = p.insert(Element::new(class("Slab"), p.physical)).unwrap();
    let physical = p.physical;
    let opening = p
        .change(|tx| {
            let opening = tx.insert(Element::new(class("Opening"), physical))?;
            tx.relate(Relationship::new(class("HostedBy"), opening, slab))?;
            Ok(opening)
        })
        .unwrap();
    let mut tx = p.store.transaction();
    tx.delete(slab).unwrap();
    let changes = tx.commit().unwrap();
    let origin = |id| match changes.elements().iter().find(|c| c.id() == id) {
        Some(ElementChange::Deleted { origin, .. }) => *origin,
        _ => panic!("deleted"),
    };
    assert_eq!(origin(slab), Origin::User);
    assert_eq!(origin(opening), Origin::Propagated);
    assert_eq!(changes.relationships()[0].origin(), Origin::Propagated);
    // What the user relates or unrelates is the user's.
    let support = p.beam_on(&[]);
    let relationship = Relationship::new(class("FramesInto"), beam, support);
    p.change(|tx| tx.relate(relationship.clone())).unwrap();
    let mut tx = p.store.transaction();
    tx.unrelate(&relationship).unwrap();
    let changes = tx.commit().unwrap();
    assert_eq!(changes.relationships()[0].origin(), Origin::User);
}

// Change sets (#21).

#[test]
fn replaying_every_change_set_rebuilds_the_model() {
    let mut store = ElementStore::new(registry());
    let mut history = Vec::new();
    let mut commit = |store: &mut ElementStore, change: &dyn Fn(&mut Transaction)| {
        let mut tx = store.transaction_for(Command::new("Test"));
        change(&mut tx);
        history.push(tx.commit().unwrap());
    };

    let physical = Model::new(ModelKind::Physical);
    let definitions = Model::new(ModelKind::Definition);
    let (p, d) = (physical.id(), definitions.id());
    let level = Element::new(class("Level"), p).with_property("Elevation", 0.0);
    let level_id = level.id();
    let mark = |value| Code::new(CodeSpec::new("TestStructural:Mark"), level_id, value).unwrap();
    let column_type = Element::new(class("ColumnType"), d).with_property("Width", 0.30);
    let column = Element::new(class("Column"), p)
        .with_type(column_type.id())
        .with_code(mark("C1"))
        .with_property("Height", 3.0)
        .with_aspect(Aspect::new(class("Note")).with_property("Text", "check"));
    let other = Element::new(class("Column"), p)
        .with_code(mark("C2"))
        .with_property("Height", 3.0);
    let (slab, opening) = (
        Element::new(class("Slab"), p),
        Element::new(class("Opening"), p),
    );
    let ids = [column.id(), other.id(), slab.id(), opening.id()];
    let [column_id, other_id, slab_id, opening_id] = ids;
    let attached = |column| Relationship::new(class("AttachedToLevel"), column, level_id);

    commit(&mut store, &|tx| {
        tx.insert_model(physical.clone()).unwrap();
        tx.insert_model(definitions.clone()).unwrap();
        tx.insert(level.clone()).unwrap();
    });
    commit(&mut store, &|tx| {
        tx.insert(column_type.clone()).unwrap();
        for element in [&column, &other, &slab, &opening] {
            tx.insert(element.clone()).unwrap();
        }
        tx.relate(attached(column_id)).unwrap();
        tx.relate(attached(other_id)).unwrap();
        tx.relate(Relationship::new(class("HostedBy"), opening_id, slab_id))
            .unwrap();
    });
    // Moving the level recomputes both columns.
    commit(&mut store, &|tx| {
        tx.set_property(level_id, "Elevation", 3.0).unwrap()
    });
    // The two columns swap codes.
    commit(&mut store, &|tx| {
        tx.set_code(column_id, None).unwrap();
        tx.set_code(other_id, Some(mark("C1"))).unwrap();
        tx.set_code(column_id, Some(mark("C2"))).unwrap();
    });
    commit(&mut store, &|tx| {
        tx.set_property(column_id, "ConcreteStrength", 30e6)
            .unwrap();
        tx.clear_property(other_id, "Height").unwrap();
        tx.unrelate(&attached(other_id)).unwrap();
    });
    // Deleting the slab deletes its opening.
    commit(&mut store, &|tx| {
        tx.delete(slab_id).unwrap();
    });

    let mut rebuilt = ElementStore::new(registry());
    for changes in &history {
        rebuilt.replay(changes).unwrap();
    }
    assert_same_contents(&rebuilt, &store);
    assert_eq!(rebuilt.find_by_code(&mark("C1")), Some(other_id));
    assert!(rebuilt.get(opening_id).is_none());

    // Replaying part of the history gives the model as it was then.
    let mut partial = ElementStore::new(registry());
    for changes in &history[..2] {
        partial.replay(changes).unwrap();
    }
    assert_eq!(partial.find_by_code(&mark("C1")), Some(column_id));
    assert!(partial.get(opening_id).is_some());
}

#[test]
fn a_change_set_that_doesnt_fit_the_store_is_refused_whole() {
    let mut p = project();
    let level = p.level(0.0);
    let column = p.column_on(level);
    let before = p.store.clone();
    let element = p.store.get(column).unwrap().clone();
    let attached = Relationship::new(class("AttachedToLevel"), column, level);
    let user = Origin::User;
    let changes =
        |elements, relationships| ChangeSet::from_parts(None, Vec::new(), elements, relationships);
    let height = by_user(FieldChange::Property {
        name: "Height".into(),
        before: Some(Value::Real(3.0)),
        after: Some(Value::Real(4.0)),
    });
    let modified = |id| ElementChange::Modified {
        id,
        fields: vec![height.clone()],
    };
    let unknown = ElementId::generate();
    let note = Aspect::new(class("Note"));
    let cases = [
        (
            changes(vec![modified(column), modified(unknown)], vec![]),
            StoreError::UnknownElement(unknown),
        ),
        (
            changes(
                vec![ElementChange::Created {
                    element,
                    origin: user,
                }],
                vec![],
            ),
            StoreError::DuplicateElement(column),
        ),
        (
            changes(
                vec![],
                vec![RelationshipChange::Added {
                    relationship: attached.clone(),
                    origin: user,
                }],
            ),
            StoreError::DuplicateRelationship(attached.clone()),
        ),
        (
            changes(
                vec![],
                vec![
                    RelationshipChange::Removed {
                        relationship: attached.clone(),
                        origin: user,
                    },
                    RelationshipChange::Removed {
                        relationship: attached.clone(),
                        origin: user,
                    },
                ],
            ),
            StoreError::MissingRelationship(attached),
        ),
        (
            changes(
                vec![ElementChange::Modified {
                    id: column,
                    fields: vec![by_user(FieldChange::AspectProperty {
                        aspect: note.id(),
                        name: "Text".into(),
                        before: None,
                        after: Some("check".into()),
                    })],
                }],
                vec![],
            ),
            StoreError::UnknownAspect(note.id()),
        ),
    ];
    for (changes, error) in cases {
        assert_eq!(p.store.replay(&changes), Err(error));
        assert_same_contents(&p.store, &before);
    }
    let model = p.store.model(p.physical).unwrap().clone();
    let again = ChangeSet::from_parts(None, vec![model], Vec::new(), Vec::new());
    let duplicate = StoreError::DuplicateModel(p.physical);
    assert_eq!(p.store.replay(&again), Err(duplicate));
}

#[test]
fn each_aspect_changes_on_its_own() {
    let mut p = project();
    let first = Aspect::new(class("Note")).with_property("Text", "check");
    let second = Aspect::new(class("Note")).with_property("Text", "recheck");
    let element = Element::new(class("Column"), p.physical)
        .with_aspect(second.clone())
        .with_aspect(first.clone());
    // Aspects keep their ID order, whatever order they were added in.
    let ids: Vec<_> = element.aspects().iter().map(Aspect::id).collect();
    assert_eq!(ids, [first.id(), second.id()]);
    let column = p.insert(element).unwrap();

    // Edit the first note, remove the second and add a third.
    let third = Aspect::new(class("Reinforcement")).with_property("Cover", 0.03);
    let fields = vec![
        by_user(FieldChange::AspectProperty {
            aspect: first.id(),
            name: "Text".into(),
            before: Some("check".into()),
            after: Some("checked".into()),
        }),
        by_user(FieldChange::Aspect {
            id: second.id(),
            before: Some(second.clone()),
            after: None,
        }),
        propagated(FieldChange::Aspect {
            id: third.id(),
            before: None,
            after: Some(third.clone()),
        }),
    ];
    let modified = ElementChange::Modified { id: column, fields };
    let changes = ChangeSet::from_parts(None, Vec::new(), vec![modified], Vec::new());
    p.store.replay(&changes).unwrap();
    let edited = p.store.get(column).unwrap();
    let first = first.with_property("Text", "checked");
    assert_eq!(edited.aspects(), [first, third.clone()]);

    // Adding an aspect it already has is refused.
    let again = ElementChange::Modified {
        id: column,
        fields: vec![by_user(FieldChange::Aspect {
            id: third.id(),
            before: None,
            after: Some(third),
        })],
    };
    let changes = ChangeSet::from_parts(None, Vec::new(), vec![again], Vec::new());
    let duplicate = StoreError::DuplicateAspect(class("Reinforcement"));
    assert_eq!(p.store.replay(&changes), Err(duplicate));

    // Two aspects with one ID are refused.
    let note = Aspect::new(class("Note"));
    let twice = Element::new(class("Column"), p.physical)
        .with_aspect(note.clone())
        .with_aspect(note);
    let duplicate = StoreError::DuplicateAspect(class("Note"));
    assert_eq!(p.insert(twice), Err(duplicate));
}

/// Checks that a store holds the same models, elements, codes and
/// relationships as another.
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
    let relationships = |store: &ElementStore| {
        let elements = store.elements().map(Element::id);
        let mut relationships: Vec<_> = elements
            .flat_map(|e| store.relationships_from(e).chain(store.relationships_to(e)))
            .cloned()
            .collect();
        relationships.sort();
        relationships
    };
    assert_eq!(elements(store), elements(expected));
    assert_eq!(models(store), models(expected));
    assert_eq!(relationships(store), relationships(expected));
    for element in expected.elements() {
        if let Some(code) = element.code() {
            assert_eq!(store.find_by_code(code), Some(element.id()));
        }
    }
}
