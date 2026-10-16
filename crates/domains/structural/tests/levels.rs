//! Levels (#33), through the core's public API only. The placement tool and
//! the views come later; this is what they build on.

use hito_core::{
    ClassDef, ClassId, Command, Element, ElementChange, ElementId, ElementStore, FieldChange,
    Model, ModelId, ModelKind, Relationship, Schema, SchemaRegistry, SchemaVersion, StoreError,
    Value,
};
use hito_structural::class;
use hito_structural::level::{
    self, ATTACHED_TO_LEVEL, BASE_ELEVATION, ELEVATION, LEVEL, LEVEL_BASED_ELEMENT, LEVEL_OFFSET,
    NAME,
};

/// A level-based element class, as the element stories (#37 to #41) will
/// add.
fn column() -> ClassId {
    ClassId::new("TestFraming", "Column")
}

struct Project {
    store: ElementStore,
    physical: ModelId,
}

fn project() -> Project {
    let mut registry = SchemaRegistry::new();
    hito_structural::register(&mut registry).unwrap();
    let framing = Schema::new("TestFraming", SchemaVersion::new(1, 0, 0))
        .class(ClassDef::new("Column", class(LEVEL_BASED_ELEMENT)));
    registry.register(framing).unwrap();
    let mut store = ElementStore::new(registry);
    let mut tx = store.transaction();
    let physical = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
    tx.commit().unwrap();
    Project { store, physical }
}

impl Project {
    fn level(&mut self, name: &str, elevation: f64) -> ElementId {
        let level = Element::new(class(LEVEL), self.physical)
            .with_property(NAME, name)
            .with_property(ELEVATION, elevation);
        let mut tx = self.store.transaction();
        let id = tx.insert(level).unwrap();
        tx.commit().unwrap();
        id
    }

    /// A column on `level`, `offset` above it.
    fn column(&mut self, level: ElementId, offset: f64) -> ElementId {
        let column = Element::new(column(), self.physical).with_property(LEVEL_OFFSET, offset);
        let mut tx = self.store.transaction();
        let id = tx.insert(column).unwrap();
        tx.relate(attached(id, level)).unwrap();
        tx.commit().unwrap();
        id
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

    fn real(&self, id: ElementId, name: &str) -> f64 {
        match self.store.property(id, name) {
            Some(Value::Real(x)) => *x,
            other => panic!("{name} is {other:?}"),
        }
    }
}

fn attached(element: ElementId, level: ElementId) -> Relationship {
    Relationship::new(class(ATTACHED_TO_LEVEL), element, level)
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
fn levels_are_created_renamed_moved_and_deleted() {
    let mut p = project();
    let ground = p.level("Planta baja", 0.0);
    let first = p.level("Primer piso", 3.0);

    p.set(first, NAME, "1° piso").unwrap();
    p.set(first, ELEVATION, 3.2).unwrap();
    assert_eq!(
        p.store.property(first, NAME),
        Some(&Value::Text("1° piso".into()))
    );
    assert_eq!(p.real(first, ELEVATION), 3.2);

    let mut tx = p.store.transaction();
    assert_eq!(tx.delete(first).unwrap(), [first]);
    tx.commit().unwrap();
    assert!(p.store.get(first).is_none());
    assert_eq!(level::levels(&p.store.view()), [(ground, 0.0)]);
}

#[test]
fn levels_list_lowest_first() {
    let mut p = project();
    let roof = p.level("Azotea", 9.0);
    let basement = p.level("Subsuelo", -3.0);
    let ground = p.level("Planta baja", 0.0);
    assert_eq!(
        level::levels(&p.store.view()),
        [(basement, -3.0), (ground, 0.0), (roof, 9.0)]
    );
}

#[test]
fn every_level_has_a_unique_name_and_an_elevation() {
    let mut p = project();
    let ground = p.level("Planta baja", 0.0);
    let first = p.level("Primer piso", 3.0);

    assert_eq!(
        broken_rules(p.set(first, NAME, "Planta baja")),
        ["LevelName"]
    );
    assert_eq!(broken_rules(p.set(first, NAME, "")), ["LevelName"]);

    let mut tx = p.store.transaction();
    let unnamed = Element::new(class(LEVEL), p.physical).with_property(ELEVATION, 6.0);
    tx.insert(unnamed).unwrap();
    assert_eq!(broken_rules(tx.commit()), ["LevelName"]);

    let mut tx = p.store.transaction();
    let flat = Element::new(class(LEVEL), p.physical).with_property(NAME, "Azotea");
    tx.insert(flat).unwrap();
    assert_eq!(broken_rules(tx.commit()), ["LevelElevation"]);

    // Swapping two names in one transaction is fine.
    let mut tx = p.store.transaction();
    tx.set_property(ground, NAME, "Primer piso").unwrap();
    tx.set_property(first, NAME, "Planta baja").unwrap();
    tx.commit().unwrap();
}

#[test]
fn level_based_elements_stand_at_their_level_plus_offset() {
    let mut p = project();
    let first = p.level("Primer piso", 3.0);
    let column = p.column(first, -0.5);
    assert_eq!(p.real(column, BASE_ELEVATION), 2.5);

    p.set(column, LEVEL_OFFSET, 0.2).unwrap();
    assert_eq!(p.real(column, BASE_ELEVATION), 3.2);

    // The base elevation is derived: setting it doesn't stick.
    p.set(column, BASE_ELEVATION, 100.0).unwrap();
    assert_eq!(p.real(column, BASE_ELEVATION), 3.2);
}

/// Changing a level's elevation moves every element attached to it, in the
/// same transaction, so it's one undo step (#19 undoes change sets).
#[test]
fn moving_a_level_moves_its_elements_in_one_change_set() {
    let mut p = project();
    let ground = p.level("Planta baja", 0.0);
    let first = p.level("Primer piso", 3.0);
    let c1 = p.column(first, 0.0);
    let c2 = p.column(first, 0.5);
    let below = p.column(ground, 0.0);

    let command = Command::new("ChangeLevelElevation").with_parameter(ELEVATION, 3.5);
    let mut tx = p.store.transaction_for(command);
    tx.set_property(first, ELEVATION, 3.5).unwrap();
    let changes = tx.commit().unwrap();

    assert_eq!(p.real(c1, BASE_ELEVATION), 3.5);
    assert_eq!(p.real(c2, BASE_ELEVATION), 4.0);
    assert_eq!(p.real(below, BASE_ELEVATION), 0.0);

    let changed: Vec<_> = changes.elements().iter().map(ElementChange::id).collect();
    assert_eq!(changed.len(), 3);
    for id in [first, c1, c2] {
        assert!(changed.contains(&id));
    }
    let c2_change = changes.elements().iter().find(|c| c.id() == c2).unwrap();
    let ElementChange::Modified { fields, .. } = c2_change else {
        panic!("c2 should be modified");
    };
    assert_eq!(
        fields,
        &[FieldChange::Property {
            name: BASE_ELEVATION.into(),
            before: Some(Value::Real(3.5)),
            after: Some(Value::Real(4.0)),
        }]
    );
}

#[test]
fn an_element_moved_to_another_level_follows_it() {
    let mut p = project();
    let ground = p.level("Planta baja", 0.0);
    let first = p.level("Primer piso", 3.0);
    let column = p.column(ground, 0.1);

    let mut tx = p.store.transaction();
    tx.unrelate(&attached(column, ground)).unwrap();
    tx.relate(attached(column, first)).unwrap();
    tx.commit().unwrap();

    assert_eq!(p.real(column, BASE_ELEVATION), 3.1);
    assert_eq!(level::level_of(&p.store.view(), column), Some(first));
}

#[test]
fn every_level_based_element_is_on_exactly_one_level() {
    let mut p = project();
    let ground = p.level("Planta baja", 0.0);
    let first = p.level("Primer piso", 3.0);

    let mut tx = p.store.transaction();
    tx.insert(Element::new(column(), p.physical)).unwrap();
    assert_eq!(broken_rules(tx.commit()), ["OneLevel"]);

    let column = p.column(ground, 0.0);
    let mut tx = p.store.transaction();
    tx.relate(attached(column, first)).unwrap();
    assert_eq!(broken_rules(tx.commit()), ["OneLevel"]);

    let mut tx = p.store.transaction();
    tx.unrelate(&attached(column, ground)).unwrap();
    assert_eq!(broken_rules(tx.commit()), ["OneLevel"]);
}

/// As in Revit, deleting a level deletes what's on it. The UI shows the
/// preview and asks first.
#[test]
fn deleting_a_level_deletes_its_elements() {
    let mut p = project();
    let ground = p.level("Planta baja", 0.0);
    let first = p.level("Primer piso", 3.0);
    let c1 = p.column(first, 0.0);
    let c2 = p.column(first, 0.0);
    let kept = p.column(ground, 0.0);

    let mut preview = p.store.preview_delete(first).unwrap();
    preview.sort();
    let mut expected = vec![first, c1, c2];
    expected.sort();
    assert_eq!(preview, expected);

    let mut tx = p.store.transaction();
    tx.delete(first).unwrap();
    tx.commit().unwrap();
    for id in [first, c1, c2] {
        assert!(p.store.get(id).is_none());
    }
    assert!(p.store.get(kept).is_some());
}

#[test]
fn only_level_based_elements_attach_to_levels() {
    let mut p = project();
    let ground = p.level("Planta baja", 0.0);
    let first = p.level("Primer piso", 3.0);
    let mut tx = p.store.transaction();
    assert_eq!(
        tx.relate(attached(first, ground)),
        Err(StoreError::InvalidRelationship(attached(first, ground)))
    );
}
