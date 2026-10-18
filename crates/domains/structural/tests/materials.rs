//! Materials (#36), through the core's public API only, as the app and the
//! regional packs use them.

use hito_core::{
    ClassId, DataType, Dimension, Element, ElementId, ElementStore, Model, ModelId, ModelKind,
    SchemaRegistry, StoreError, Value,
};
use hito_structural::material::{
    COMPRESSIVE_STRENGTH, CONCRETE, MASS_DENSITY, MATERIAL, NAME, SHADING_COLOUR, YOUNG_MODULUS,
};
use hito_structural::{SCHEMA, VERSION, class};

struct Project {
    store: ElementStore,
    definitions: ModelId,
}

fn project() -> Project {
    let mut registry = SchemaRegistry::new();
    hito_structural::register(&mut registry).unwrap();
    let mut store = ElementStore::new(registry);
    let mut tx = store.transaction();
    let definitions = tx.insert_model(Model::new(ModelKind::Definition)).unwrap();
    tx.commit().unwrap();
    Project { store, definitions }
}

impl Project {
    fn insert(&mut self, element: Element) -> Result<ElementId, StoreError> {
        let mut tx = self.store.transaction();
        let id = tx.insert(element)?;
        tx.commit()?;
        Ok(id)
    }

    fn set(
        &mut self,
        id: ElementId,
        name: &str,
        value: impl Into<Value>,
    ) -> Result<(), StoreError> {
        let mut tx = self.store.transaction();
        tx.set_property(id, name, value)?;
        tx.commit().map(|_| ())
    }

    fn concrete(&self, name: &str) -> Element {
        Element::new(class(CONCRETE), self.definitions)
            .with_property(NAME, name)
            .with_property(COMPRESSIVE_STRENGTH, 25e6)
            .with_property(YOUNG_MODULUS, 23.5e9)
            .with_property(MASS_DENSITY, 2400.0)
            .with_property(SHADING_COLOUR, "#a0a0a0")
    }
}

/// The names of the rules a failed commit broke.
fn broken_rules(result: Result<impl Sized, StoreError>) -> Vec<String> {
    match result {
        Err(StoreError::Violations(violations)) => violations.into_iter().map(|v| v.rule).collect(),
        Err(other) => panic!("expected rule violations, got {other}"),
        Ok(_) => panic!("expected rule violations, the commit succeeded"),
    }
}

#[test]
fn materials_are_definition_elements_with_typed_properties() {
    let p = project();
    let registry = p.store.registry();
    assert_eq!(registry.schema_version(SCHEMA), Some(VERSION));
    assert!(registry.is_a(&class(CONCRETE), &class(MATERIAL)));
    assert!(registry.is_a(&class(MATERIAL), &ClassId::DEFINITION_ELEMENT));

    let data_type = |name| {
        registry
            .property(&class(CONCRETE), name)
            .unwrap()
            .data_type()
    };
    let stress = DataType::Quantity(Dimension::PRESSURE);
    assert_eq!(data_type(COMPRESSIVE_STRENGTH), stress);
    assert_eq!(data_type(YOUNG_MODULUS), stress);
    assert_eq!(
        data_type(MASS_DENSITY),
        DataType::Quantity(Dimension::DENSITY)
    );
    assert_eq!(data_type(NAME), DataType::Text);
    assert_eq!(data_type(SHADING_COLOUR), DataType::Text);
}

#[test]
fn users_create_and_edit_their_own_materials() {
    let mut p = project();
    let custom = p.concrete("Site mix, tested");
    let custom = p.insert(custom).unwrap();

    // A tested modulus replaces the one a code formula would give.
    p.set(custom, YOUNG_MODULUS, 27.1e9).unwrap();
    p.set(custom, NAME, "Site mix, 2026 tests").unwrap();
    p.set(custom, SHADING_COLOUR, "#8090A0").unwrap();

    let value = |name| p.store.property(custom, name).cloned();
    assert_eq!(value(YOUNG_MODULUS), Some(Value::Real(27.1e9)));
    assert_eq!(
        value(NAME),
        Some(Value::Text("Site mix, 2026 tests".into()))
    );
    assert_eq!(value(COMPRESSIVE_STRENGTH), Some(Value::Real(25e6)));
}

#[test]
fn quantities_must_be_numbers_in_si() {
    let mut p = project();
    let concrete = p
        .concrete("H-25")
        .with_property(COMPRESSIVE_STRENGTH, "25 MPa");
    assert!(matches!(
        p.insert(concrete),
        Err(StoreError::WrongValueType { .. })
    ));
}

#[test]
fn every_material_has_a_unique_name() {
    let mut p = project();
    let h25 = p.concrete("H-25");
    let h25 = p.insert(h25).unwrap();
    let h30 = p.concrete("H-30");
    let h30 = p.insert(h30).unwrap();

    let twin = p.concrete("H-25");
    assert_eq!(broken_rules(p.insert(twin)), ["MaterialName"]);
    assert_eq!(broken_rules(p.set(h30, NAME, "H-25")), ["MaterialName"]);
    assert_eq!(broken_rules(p.set(h30, NAME, "  ")), ["MaterialName"]);

    let unnamed = Element::new(class(CONCRETE), p.definitions);
    assert_eq!(broken_rules(p.insert(unnamed)), ["MaterialName"]);

    // Two materials can swap names in one transaction.
    let mut tx = p.store.transaction();
    tx.set_property(h25, NAME, "H-30").unwrap();
    tx.set_property(h30, NAME, "H-25").unwrap();
    tx.commit().unwrap();
    assert_eq!(
        p.store.property(h25, NAME),
        Some(&Value::Text("H-30".into()))
    );
}

#[test]
fn engineering_values_are_greater_than_zero() {
    let mut p = project();
    let h25 = p.concrete("H-25");
    let h25 = p.insert(h25).unwrap();
    for name in [COMPRESSIVE_STRENGTH, YOUNG_MODULUS, MASS_DENSITY] {
        assert_eq!(
            broken_rules(p.set(h25, name, 0.0)),
            ["PositiveMaterialProperty"]
        );
        assert_eq!(
            broken_rules(p.set(h25, name, -1.0)),
            ["PositiveMaterialProperty"]
        );
    }
}

#[test]
fn shading_colours_are_written_as_hex() {
    let mut p = project();
    let h25 = p.concrete("H-25");
    let h25 = p.insert(h25).unwrap();
    for colour in ["grey", "#a0a0a", "a0a0a0"] {
        assert_eq!(
            broken_rules(p.set(h25, SHADING_COLOUR, colour)),
            ["ShadingColour"]
        );
    }
    p.set(h25, SHADING_COLOUR, "#C0C0C0").unwrap();
}
