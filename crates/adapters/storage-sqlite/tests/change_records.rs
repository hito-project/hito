//! Change records (#21): every commit becomes a record that reads back
//! exactly, and replaying the records rebuilds the model.

use std::collections::BTreeMap;

use hito_core::{
    Aspect, ClassDef, ClassId, Code, CodeSpec, Command, DataType, Dimension, Element,
    ElementChange, ElementId, ElementStore, ExternalSource, FieldChange, Model, ModelKind,
    Properties, Recompute, Relationship, RelationshipChange, RelationshipDef, Rule, Schema,
    SchemaRegistry, SchemaVersion, Transaction, Value,
};
use hito_storage_sqlite::{ChangeRecord, DecodeError, RecordId, Target, Timestamp};

const LENGTH: DataType = DataType::Quantity(Dimension::LENGTH);

fn class(name: &str) -> ClassId {
    ClassId::new("Test", name)
}

/// Levels, typed columns that follow their level, and two aspect classes.
fn registry() -> SchemaRegistry {
    let schema = Schema::new("Test", SchemaVersion::new(2, 1, 3))
        .class(
            ClassDef::new("Level", ClassId::SPATIAL_LOCATION_ELEMENT).property("Elevation", LENGTH),
        )
        .class(ClassDef::new("ColumnType", ClassId::DEFINITION_ELEMENT).property("Width", LENGTH))
        .class(
            ClassDef::new("Column", ClassId::GEOMETRIC_ELEMENT_3D)
                .typed_by(class("ColumnType"))
                .property("Height", LENGTH)
                .property("BaseElevation", LENGTH)
                .property("Count", DataType::Integer)
                .property("LoadBearing", DataType::Boolean)
                .property("Grade", DataType::Text)
                .property("Ratio", DataType::Real),
        )
        .class(ClassDef::new("Reinforcement", ClassId::UNIQUE_ASPECT).property("Cover", LENGTH))
        .class(ClassDef::new("Note", ClassId::MULTI_ASPECT).property("Text", DataType::Text))
        .relationship(
            RelationshipDef::new("AttachedToLevel", class("Column"), class("Level")).driving(),
        );
    let mut registry = SchemaRegistry::new();
    registry.register(schema).unwrap();
    let follow_level = Recompute::new(class("Column"), |store, column| {
        let mut values = Properties::new();
        let level = store.relationships_from(column.id()).next();
        if let Some(elevation) = level.and_then(|l| store.property(l.target(), "Elevation")) {
            values.insert("BaseElevation".into(), elevation.clone());
        }
        values
    });
    registry.add_recompute(follow_level).unwrap();
    let tall = Rule::new("Tall", class("Column"), |_, column| {
        match column.properties().get("Height") {
            Some(Value::Real(height)) if *height > 10.0 => Err(format!("{height} m is tall")),
            _ => Ok(()),
        }
    });
    registry.add_rule(tall.warning()).unwrap();
    registry
}

/// A store and the record of every commit made to it.
struct History {
    store: ElementStore,
    records: Vec<Vec<u8>>,
}

impl History {
    fn new() -> Self {
        Self {
            store: ElementStore::new(registry()),
            records: Vec::new(),
        }
    }

    /// Commits a change as a user command and appends its encoded record.
    fn commit(&mut self, command: Command, change: impl FnOnce(&mut Transaction)) -> ChangeRecord {
        let mut tx = self.store.transaction_for(command);
        change(&mut tx);
        let changes = tx.commit().unwrap();
        let record = ChangeRecord::new(changes, &self.store, "Ana");
        self.records.push(record.encode());
        record
    }
}

/// A day of edits touching every kind of change and value the record holds.
fn edits() -> (History, [ElementId; 4]) {
    let mut h = History::new();
    let physical = Model::new(ModelKind::Physical);
    let definitions = Model::new(ModelKind::Definition);
    let (p, d) = (physical.id(), definitions.id());
    let level = Element::new(class("Level"), p).with_property("Elevation", 0.0);
    let level_id = level.id();
    let mark = |value| Code::new(CodeSpec::new("Test:Mark"), level_id, value).unwrap();
    let column_type = Element::new(class("ColumnType"), d).with_property("Width", 0.3);
    let column = Element::new(class("Column"), p)
        .with_type(column_type.id())
        .with_code(mark("C1"))
        .with_source(ExternalSource::new(level_id, "2O2Fr$t4X7Zf8NOew3FLOH"))
        .with_property("Height", 3.0)
        .with_property("Count", i64::MIN)
        .with_property("LoadBearing", true)
        .with_property("Grade", "Hormigón H-25 «ñ»")
        // Values that only read back if the bits are kept.
        .with_property("Ratio", 0.1 + 0.2)
        .with_aspect(Aspect::new(class("Reinforcement")).with_property("Cover", 5e-324))
        .with_aspect(Aspect::new(class("Note")).with_property("Text", "check"))
        .with_aspect(Aspect::new(class("Note")).with_property("Text", ""));
    let other = Element::new(class("Column"), p).with_property("Height", -0.0);
    let ids = [level_id, column_type.id(), column.id(), other.id()];
    let [_, type_id, column_id, other_id] = ids;
    let attached = |c| Relationship::new(class("AttachedToLevel"), c, level_id);

    h.commit(Command::new("NewProject"), |tx| {
        tx.insert_model(physical).unwrap();
        tx.insert_model(definitions).unwrap();
        tx.insert_model(Model::new(ModelKind::Drawing)).unwrap();
        tx.insert_model(Model::new(ModelKind::Analytical)).unwrap();
        tx.insert_model(Model::new(ModelKind::Linked)).unwrap();
        tx.insert(level).unwrap();
        tx.insert(column_type).unwrap();
    });
    let command = Command::new("PlaceColumn")
        .with_parameter("Height", 3.0)
        .with_parameter("Grade", "H-25");
    h.commit(command, |tx| {
        tx.insert(column).unwrap();
        tx.insert(other).unwrap();
        tx.relate(attached(column_id)).unwrap();
        tx.relate(attached(other_id)).unwrap();
    });
    h.commit(Command::new("MoveLevel"), |tx| {
        tx.set_property(level_id, "Elevation", 3.05).unwrap();
    });
    h.commit(Command::new("Edit"), |tx| {
        tx.set_code(column_id, Some(mark("C2"))).unwrap();
        tx.set_code(other_id, Some(mark("C1"))).unwrap();
        tx.clear_property(column_id, "Grade").unwrap();
        tx.set_property(type_id, "Width", f64::MAX).unwrap();
        tx.unrelate(&attached(other_id)).unwrap();
    });
    h.commit(Command::new("Delete"), |tx| {
        tx.delete(column_id).unwrap();
    });
    (h, ids)
}

#[test]
fn every_commit_records_its_changes_with_before_and_after_values() {
    let (h, [level, _, column, other]) = edits();
    let records: Vec<_> = h.records.iter().map(|r| decode(r)).collect();
    assert_eq!(records.len(), 5);

    let moved = &records[2];
    assert_eq!(moved.changes.command().unwrap().name(), "MoveLevel");
    assert_eq!(moved.author, "Ana");
    assert_eq!(moved.target, Target::Main);
    assert!(moved.parents.is_empty());
    // The level, and the two columns that follow it.
    let base = |id| ElementChange::Modified {
        id,
        fields: vec![FieldChange::Property {
            name: "BaseElevation".into(),
            before: Some(Value::Real(0.0)),
            after: Some(Value::Real(3.05)),
        }],
    };
    let level_moved = ElementChange::Modified {
        id: level,
        fields: vec![FieldChange::Property {
            name: "Elevation".into(),
            before: Some(Value::Real(0.0)),
            after: Some(Value::Real(3.05)),
        }],
    };
    assert_eq!(
        moved.changes.elements(),
        [level_moved, base(column), base(other)]
    );

    let deleted = &records[4];
    assert!(matches!(
        deleted.changes.elements(),
        [ElementChange::Deleted(e)] if e.id() == column && e.aspects().len() == 3
    ));
    assert_eq!(
        deleted.changes.relationships(),
        [RelationshipChange::Removed(Relationship::new(
            class("AttachedToLevel"),
            column,
            level
        ))]
    );
}

#[test]
fn records_list_the_versions_of_the_schemas_they_use() {
    let (h, _) = edits();
    let moved = decode(&h.records[2]);
    let expected = BTreeMap::from([("Test".to_owned(), SchemaVersion::new(2, 1, 3))]);
    assert_eq!(moved.schemas, expected);
    // Levels and columns derive from core classes, but use only the test
    // schema's classes.
    let first = decode(&h.records[0]);
    assert_eq!(first.schemas, expected);
}

#[test]
fn records_read_back_exactly() {
    let (mut h, [level, ..]) = edits();
    let mut record = h.commit(Command::new("Rename"), |tx| {
        tx.set_property(level, "Elevation", 6.1).unwrap();
    });
    // A merge, by someone else, at the start of time.
    record.parents = vec![RecordId([7; 32]), RecordId([0xff; 32])];
    record.author = "Bruno Pérez".into();
    record.time = Timestamp::from_millis(-1);
    let bytes = record.encode();
    assert_eq!(ChangeRecord::decode(&bytes), Ok(record.clone()));

    for bytes in &h.records {
        let record = decode(bytes);
        // The same bytes again, so a record's hash never changes.
        assert_eq!(&record.encode(), bytes);
        assert_eq!(decode(&record.encode()), record);
    }
    let created = decode(&h.records[1]);
    let ElementChange::Created(column) = &created.changes.elements()[0] else {
        panic!("the column was created");
    };
    let real = |name| match column.properties()[name] {
        Value::Real(x) => x.to_bits(),
        _ => panic!("a real"),
    };
    assert_eq!(real("Ratio"), (0.1_f64 + 0.2).to_bits());
    let ElementChange::Created(other) = &created.changes.elements()[1] else {
        panic!("the other column was created");
    };
    assert!(matches!(other.properties()["Height"], Value::Real(x) if x.is_sign_negative()));
}

#[test]
fn warnings_are_not_recorded() {
    let (mut h, [_, _, _, other]) = edits();
    let record = h.commit(Command::new("Stretch"), |tx| {
        tx.set_property(other, "Height", 12.0).unwrap();
    });
    assert_eq!(record.changes.warnings().len(), 1);
    let read = decode(&record.encode());
    assert!(read.changes.warnings().is_empty());
    assert_eq!(read.changes.elements(), record.changes.elements());
}

#[test]
fn anything_else_is_refused() {
    let (h, _) = edits();
    let record = &h.records[1];
    assert_eq!(ChangeRecord::decode(&[]), Err(DecodeError::Malformed));
    // A version this HITO doesn't know.
    assert_eq!(ChangeRecord::decode(&[1]), Err(DecodeError::Malformed));
    let cut = &record[..record.len() - 1];
    assert_eq!(ChangeRecord::decode(cut), Err(DecodeError::Malformed));
    let longer = [&record[..], &[0][..]].concat();
    assert_eq!(ChangeRecord::decode(&longer), Err(DecodeError::Malformed));
}

#[test]
fn replaying_the_records_rebuilds_the_model() {
    let (h, [_, _, column, other]) = edits();
    let mut rebuilt = ElementStore::new(registry());
    for bytes in &h.records {
        rebuilt.replay(&decode(bytes).changes);
    }
    assert_same_contents(&rebuilt, &h.store);
    assert!(rebuilt.get(column).is_none());
    let mark = h.store.get(other).unwrap().code().unwrap();
    assert_eq!(rebuilt.find_by_code(mark), Some(other));
}

fn decode(bytes: &[u8]) -> ChangeRecord {
    ChangeRecord::decode(bytes).unwrap()
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
        let mut models: Vec<_> = store.models().cloned().collect();
        models.sort_by_key(Model::id);
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
