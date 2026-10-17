//! A real model for the compression study: the elements of an IFC file
//! (a Revit structural export), with their placement, names and every
//! property and quantity, repeated floor by floor up to the size wanted.
//! Edits pick realistic values: values that property has elsewhere in the
//! model, moves by whole centimetres, copies of real elements.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use hito_core::{
    ChangeSet, ClassDef, ClassId, Command, DataType, Element, ElementId, ElementStore, Model,
    ModelKind, Schema, SchemaRegistry, SchemaVersion, Value,
};
use hito_step::{EntityId, StepReader, Value as V};

use crate::domain::Rng;

const SCHEMA: &str = "Ifc";
const TYPES: [&str; 8] = [
    "IFCBEAM",
    "IFCCOLUMN",
    "IFCSLAB",
    "IFCWALL",
    "IFCFOOTING",
    "IFCMEMBER",
    "IFCREINFORCINGBAR",
    "IFCPILE",
];

struct Imported {
    class: String,
    properties: BTreeMap<String, Value>,
}

fn reference(v: &V) -> Option<EntityId> {
    match v {
        V::Reference(id) => Some(*id),
        _ => None,
    }
}

fn text(v: &V) -> Option<String> {
    match v {
        V::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// A property's or quantity's value, unwrapped from its IFC type.
fn value(v: &V) -> Option<Value> {
    match v {
        V::Typed(_, inner) => value(inner),
        V::Real(x) => Some(Value::Real(*x)),
        V::Integer(i) => Some(Value::Integer(*i)),
        V::Boolean(b) => Some(Value::Boolean(*b)),
        V::String(s) => Some(Value::Text(s.clone())),
        V::Enumeration(e) if e == "T" || e == "F" => Some(Value::Boolean(e == "T")),
        V::Enumeration(e) => Some(Value::Text(e.to_string())),
        _ => None,
    }
}

/// The placement's location, adding up the chain of relative placements
/// (rotations are ignored: the values matter here, not the geometry).
fn location(r: &StepReader, placement: Option<EntityId>, depth: usize) -> [f64; 3] {
    let Some(id) = placement else { return [0.0; 3] };
    let Ok(p) = r.get(id) else { return [0.0; 3] };
    if depth > 20 || p.attributes.len() < 2 {
        return [0.0; 3];
    }
    let parent = location(r, reference(&p.attributes[0]), depth + 1);
    let mut here = [0.0; 3];
    if let Some(axis) = reference(&p.attributes[1]).and_then(|a| r.get(a).ok())
        && let Some(point) = axis
            .attributes
            .first()
            .and_then(reference)
            .and_then(|c| r.get(c).ok())
        && let Some(V::List(coords)) = point.attributes.first()
    {
        for (i, c) in coords.iter().take(3).enumerate() {
            if let V::Real(x) = c {
                here[i] = *x;
            }
        }
    }
    [
        parent[0] + here[0],
        parent[1] + here[1],
        parent[2] + here[2],
    ]
}

fn import(path: &Path) -> Vec<Imported> {
    let data = std::fs::read(path).unwrap();
    let r = StepReader::new(&data).unwrap();
    let mut properties: HashMap<EntityId, BTreeMap<String, Value>> = HashMap::new();
    for &rel in r.instances_of("IFCRELDEFINESBYPROPERTIES") {
        let rel = r.get(rel).unwrap();
        let Some(V::List(objects)) = rel.attributes.get(4) else {
            continue;
        };
        let Some(set) = rel
            .attributes
            .get(5)
            .and_then(reference)
            .and_then(|s| r.get(s).ok())
        else {
            continue;
        };
        let set_name = set.attributes.get(2).and_then(text).unwrap_or_default();
        let mut values = BTreeMap::new();
        let items = match set.name {
            "IFCPROPERTYSET" => set.attributes.get(4),
            "IFCELEMENTQUANTITY" => set.attributes.get(5),
            _ => None,
        };
        let Some(V::List(items)) = items else {
            continue;
        };
        for item in items.iter().filter_map(reference) {
            let Ok(item) = r.get(item) else { continue };
            let name = item.attributes.first().and_then(text).unwrap_or_default();
            let v = match item.name {
                "IFCPROPERTYSINGLEVALUE" => item.attributes.get(2).and_then(value),
                n if n.starts_with("IFCQUANTITY") => item.attributes.get(3).and_then(value),
                _ => None,
            };
            if let Some(v) = v {
                values.insert(identifier(&format!("{set_name}_{name}")), v);
            }
        }
        for object in objects.iter().filter_map(reference) {
            properties.entry(object).or_default().extend(values.clone());
        }
    }
    let mut elements = Vec::new();
    for ty in TYPES {
        for &id in r.instances_of(ty) {
            let e = r.get(id).unwrap();
            let mut props = properties.remove(&id).unwrap_or_default();
            for (i, key) in [(2, "Name"), (4, "ObjectType"), (7, "Tag")] {
                if let Some(t) = e.attributes.get(i).and_then(text) {
                    props.insert(key.into(), Value::Text(t));
                }
            }
            let [x, y, z] = location(&r, e.attributes.get(5).and_then(reference), 0);
            props.insert("X".into(), Value::Real(x));
            props.insert("Y".into(), Value::Real(y));
            props.insert("Z".into(), Value::Real(z));
            elements.push(Imported {
                class: ty.trim_start_matches("IFC").to_owned(),
                properties: props,
            });
        }
    }
    elements
}

fn data_type(v: &Value) -> DataType {
    match v {
        Value::Boolean(_) => DataType::Boolean,
        Value::Integer(_) => DataType::Integer,
        Value::Real(_) => DataType::Real,
        Value::Text(_) => DataType::Text,
    }
}

/// A schema with one class per IFC type, declaring every property found.
fn registry(elements: &[Imported]) -> SchemaRegistry {
    let mut classes: BTreeMap<&str, BTreeMap<&str, DataType>> = BTreeMap::new();
    for e in elements {
        let class = classes.entry(&e.class).or_default();
        for (k, v) in &e.properties {
            class.entry(k).or_insert(data_type(v));
        }
    }
    let schema = classes.into_iter().fold(
        Schema::new(SCHEMA, SchemaVersion::new(1, 0, 0)),
        |s, (name, props)| {
            let class = props.into_iter().fold(
                ClassDef::new(name, ClassId::GEOMETRIC_ELEMENT_3D),
                |c, (p, t)| c.property(p, t),
            );
            s.class(class)
        },
    );
    let mut registry = SchemaRegistry::new();
    registry.register(schema).unwrap();
    registry
}

pub struct RealModel {
    pub store: ElementStore,
    model: hito_core::ModelId,
    ids: Vec<ElementId>,
    /// Every value each property has somewhere in the model.
    seen: HashMap<String, Vec<Value>>,
    floor_height: f64,
    floors: usize,
}

/// Imports the IFC file and repeats its elements on `floors` floors.
pub fn build(path: &Path, floors: usize) -> (RealModel, Vec<ChangeSet>, usize) {
    let mut elements = import(path);
    // One data type per class and property: the first one seen.
    let mut types: HashMap<(String, String), DataType> = HashMap::new();
    for e in &mut elements {
        let class = e.class.clone();
        e.properties.retain(|k, v| {
            *types
                .entry((class.clone(), k.clone()))
                .or_insert(data_type(v))
                == data_type(v)
        });
    }
    let per_floor = elements.len();
    let mut store = ElementStore::new(registry(&elements));
    let mut seen: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut seen_values: HashMap<String, Vec<Value>> = HashMap::new();
    for e in &elements {
        for (k, v) in &e.properties {
            let k = &format!("{}/{k}", e.class);
            if seen.entry(k.clone()).or_default().insert(format!("{v:?}")) {
                seen_values.entry(k.clone()).or_default().push(v.clone());
            }
        }
    }
    let z_max = elements
        .iter()
        .filter_map(|e| match e.properties.get("Z") {
            Some(Value::Real(z)) => Some(*z),
            _ => None,
        })
        .fold(0.0_f64, f64::max);
    let floor_height = (z_max + 3.0).max(3.0);
    let mut changes = Vec::new();
    let mut tx = store.transaction();
    let model = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
    changes.push(tx.commit().unwrap());
    let mut ids = Vec::new();
    for floor in 0..floors {
        let mut tx = store.transaction_for(Command::new("CopyFloor"));
        for e in &elements {
            let mut element = Element::new(ClassId::new(SCHEMA, e.class.clone()), model);
            for (k, v) in &e.properties {
                let v = match (k.as_str(), v) {
                    ("Z", Value::Real(z)) => Value::Real(z + floor as f64 * floor_height),
                    _ => v.clone(),
                };
                element = element.with_property(k.clone(), v);
            }
            ids.push(tx.insert(element).unwrap());
        }
        changes.push(tx.commit().unwrap());
    }
    let real = RealModel {
        store,
        model,
        ids,
        seen: seen_values,
        floor_height,
        floors,
    };
    (real, changes, per_floor)
}

impl RealModel {
    fn pick(&self, rng: &mut Rng) -> ElementId {
        self.ids[rng.below(self.ids.len())]
    }

    /// One realistic edit.
    pub fn edit(&mut self, rng: &mut Rng) -> ChangeSet {
        match rng.below(100) {
            // Set a property to a value it has elsewhere in the model.
            0..60 => {
                let id = self.pick(rng);
                let element = self.store.get(id).unwrap().clone();
                let names: Vec<_> = element
                    .properties()
                    .keys()
                    .filter(|k| {
                        !matches!(k.as_str(), "X" | "Y" | "Z")
                            && self.seen[&format!("{}/{k}", element.class().name())].len() > 1
                    })
                    .cloned()
                    .collect();
                let mut tx = self.store.transaction_for(Command::new("EditProperty"));
                if let Some(name) = names.get(rng.below(names.len().max(1))) {
                    let values = &self.seen[&format!("{}/{name}", element.class().name())];
                    let v = values[rng.below(values.len())].clone();
                    tx.set_property(id, name, v).unwrap();
                }
                tx.commit().unwrap()
            }
            // Move a selection by whole centimetres.
            60..80 => {
                let n = 1 + rng.below(20);
                let (dx, dy) = (
                    (rng.below(401) as f64 - 200.0) / 100.0,
                    (rng.below(401) as f64 - 200.0) / 100.0,
                );
                let mut tx = self.store.transaction_for(Command::new("Move"));
                let mut done = BTreeSet::new();
                for _ in 0..n {
                    let id = self.ids[rng.below(self.ids.len())];
                    if !done.insert(id) {
                        continue;
                    }
                    for (axis, d) in [("X", dx), ("Y", dy)] {
                        if let Some(Value::Real(x)) =
                            tx.view().get(id).unwrap().properties().get(axis).cloned()
                        {
                            tx.set_property(id, axis, x + d).unwrap();
                        }
                    }
                }
                tx.commit().unwrap()
            }
            // Copy a selection to another floor.
            80..90 => {
                let n = 1 + rng.below(20);
                let floors = 1 + rng.below(self.floors.max(2) - 1);
                let mut tx = self.store.transaction_for(Command::new("Copy"));
                let mut new = Vec::new();
                for _ in 0..n {
                    let source = tx
                        .view()
                        .get(self.ids[rng.below(self.ids.len())])
                        .unwrap()
                        .clone();
                    let mut element = Element::new(source.class().clone(), self.model);
                    for (k, v) in source.properties() {
                        let v = match (k.as_str(), v) {
                            ("Z", Value::Real(z)) => {
                                Value::Real(z + floors as f64 * self.floor_height)
                            }
                            _ => v.clone(),
                        };
                        element = element.with_property(k.clone(), v);
                    }
                    new.push(tx.insert(element).unwrap());
                }
                let changes = tx.commit().unwrap();
                self.ids.extend(new);
                changes
            }
            // Delete a selection.
            _ => {
                let n = 1 + rng.below(20);
                let mut tx = self.store.transaction_for(Command::new("Delete"));
                for _ in 0..n {
                    let id = self.ids.swap_remove(rng.below(self.ids.len()));
                    tx.delete(id).unwrap();
                }
                tx.commit().unwrap()
            }
        }
    }
}

/// A property name the core accepts: letters, digits and `_`.
fn identifier(name: &str) -> String {
    let mut id: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if !id.starts_with(|c: char| c.is_ascii_alphabetic()) {
        id.insert(0, 'P');
    }
    id
}
