//! A small structural domain and a model generator, for realistic change
//! sets: levels drive columns, beams and slabs, so moving a level touches
//! thousands of elements in one transaction.

use hito_core::{
    ClassDef, ClassId, DataType, Dimension, Element, ElementId, ElementStore, Model, ModelId,
    ModelKind, OnDelete, Properties, Recompute, Relationship, RelationshipDef, Schema,
    SchemaRegistry, SchemaVersion, Value,
};

pub const SCHEMA: &str = "SpikeStructural";
const LENGTH: DataType = DataType::Quantity(Dimension::LENGTH);
const STRESS: DataType = DataType::Quantity(Dimension::PRESSURE);

pub fn class(name: &str) -> ClassId {
    ClassId::new(SCHEMA, name)
}

fn schema() -> Schema {
    let located = |name: &str| {
        ClassDef::new(name, ClassId::GEOMETRIC_ELEMENT_3D)
            .property("X", LENGTH)
            .property("Y", LENGTH)
            .property("BaseOffset", LENGTH)
            .property("BaseElevation", LENGTH)
            .property("Mark", DataType::Text)
            .property("Comments", DataType::Text)
    };
    Schema::new(SCHEMA, SchemaVersion::new(1, 0, 0))
        .class(
            ClassDef::new("Level", ClassId::SPATIAL_LOCATION_ELEMENT)
                .property("Elevation", LENGTH)
                .property("Name", DataType::Text),
        )
        .class(
            ClassDef::new("FramingType", ClassId::DEFINITION_ELEMENT)
                .property("Width", LENGTH)
                .property("Depth", LENGTH)
                .property("ConcreteStrength", STRESS)
                .property("Grade", DataType::Text),
        )
        .class(
            located("Column")
                .typed_by(class("FramingType"))
                .property("Height", LENGTH),
        )
        .class(
            located("Beam")
                .typed_by(class("FramingType"))
                .property("X2", LENGTH)
                .property("Y2", LENGTH),
        )
        .class(
            located("Slab")
                .property("Thickness", LENGTH)
                .property("Width", LENGTH)
                .property("Length", LENGTH),
        )
        .relationship(
            RelationshipDef::new("OnLevel", ClassId::GEOMETRIC_ELEMENT_3D, class("Level"))
                .driving()
                .on_delete(OnDelete::Block),
        )
}

fn real(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Real(x)) => *x,
        _ => 0.0,
    }
}

pub fn registry() -> SchemaRegistry {
    let mut registry = SchemaRegistry::new();
    registry.register(schema()).unwrap();
    for name in ["Column", "Beam", "Slab"] {
        let recompute = Recompute::new(class(name), |store, element| {
            let mut values = Properties::new();
            let level = store
                .relationships_from(element.id())
                .find(|r| r.class() == &class("OnLevel"));
            if let Some(level) = level {
                let elevation = real(store.property(level.target(), "Elevation"));
                let offset = real(element.properties().get("BaseOffset"));
                values.insert("BaseElevation".into(), (elevation + offset).into());
            }
            values
        });
        registry.add_recompute(recompute).unwrap();
    }
    registry
}

/// A deterministic pseudo-random generator, so runs are comparable.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    pub fn real(&mut self, max: f64) -> f64 {
        (self.next() % 1_000_000) as f64 / 1_000_000.0 * max
    }
}

pub struct Project {
    pub store: ElementStore,
    pub physical: ModelId,
    pub definitions: ModelId,
    pub levels: Vec<ElementId>,
    pub types: Vec<ElementId>,
    /// Columns, beams and slabs, by level.
    pub members: Vec<Vec<ElementId>>,
}

impl Project {
    /// The IDs, for a branch that gets its own store and members.
    pub fn clone_ids(&self) -> Self {
        Self {
            store: ElementStore::new(registry()),
            physical: self.physical,
            definitions: self.definitions,
            levels: self.levels.clone(),
            types: self.types.clone(),
            members: Vec::new(),
        }
    }
}

/// Per level: columns, beams and slabs.
pub const COLUMNS: usize = 1500;
pub const BEAMS: usize = 3000;
pub const SLABS: usize = 500;

pub fn member(project: &Project, rng: &mut Rng, kind: &str, level: usize) -> Element {
    let model = project.physical;
    let mut element = Element::new(class(kind), model)
        .with_property("X", rng.real(120.0))
        .with_property("Y", rng.real(80.0))
        .with_property("BaseOffset", 0.0)
        .with_property("Mark", format!("{}{}", &kind[..1], rng.below(10_000)))
        .with_property("Comments", "");
    match kind {
        "Column" => {
            element = element
                .with_type(project.types[rng.below(project.types.len())])
                .with_property("Height", 3.0)
        }
        "Beam" => {
            element = element
                .with_type(project.types[rng.below(project.types.len())])
                .with_property("X2", rng.real(120.0))
                .with_property("Y2", rng.real(80.0))
        }
        _ => {
            element = element
                .with_property("Thickness", 0.15)
                .with_property("Width", rng.real(8.0))
                .with_property("Length", rng.real(8.0))
        }
    }
    let _ = level;
    element
}

/// Creates the project's models, types and levels, then `levels` levels of
/// members, each in its own transaction, and returns every change set.
pub fn build(levels: usize, rng: &mut Rng) -> (Project, Vec<hito_core::ChangeSet>) {
    let mut store = ElementStore::new(registry());
    let mut changes = Vec::new();
    let mut tx = store.transaction();
    let physical = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
    let definitions = tx.insert_model(Model::new(ModelKind::Definition)).unwrap();
    let mut types = Vec::new();
    for i in 0..20 {
        let size = 0.2 + 0.05 * i as f64;
        let id = tx
            .insert(
                Element::new(class("FramingType"), definitions)
                    .with_property("Width", size)
                    .with_property("Depth", size + 0.2)
                    .with_property("ConcreteStrength", 30e6)
                    .with_property("Grade", "H-30"),
            )
            .unwrap();
        types.push(id);
    }
    let mut level_ids = Vec::new();
    for i in 0..levels {
        let id = tx
            .insert(
                Element::new(class("Level"), physical)
                    .with_property("Elevation", 3.0 * i as f64)
                    .with_property("Name", format!("Nivel {i}")),
            )
            .unwrap();
        level_ids.push(id);
    }
    changes.push(tx.commit().unwrap());
    let mut project = Project {
        store,
        physical,
        definitions,
        levels: level_ids,
        types,
        members: Vec::new(),
    };
    for level in 0..levels {
        let mut kinds = Vec::new();
        kinds.extend(std::iter::repeat_n("Column", COLUMNS));
        kinds.extend(std::iter::repeat_n("Beam", BEAMS));
        kinds.extend(std::iter::repeat_n("Slab", SLABS));
        let elements: Vec<_> = kinds
            .into_iter()
            .map(|kind| member(&project, rng, kind, level))
            .collect();
        let level_id = project.levels[level];
        let mut ids = Vec::new();
        let mut tx = project.store.transaction();
        for element in elements {
            let id = tx.insert(element).unwrap();
            tx.relate(Relationship::new(class("OnLevel"), id, level_id))
                .unwrap();
            ids.push(id);
        }
        changes.push(tx.commit().unwrap());
        project.members.push(ids);
    }
    (project, changes)
}
