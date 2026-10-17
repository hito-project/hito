//! The geometry cache (#28): elements get their geometry from their
//! parameters, and only the affected elements are generated again when the
//! model changes. Uses a test domain and the core's public API only.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, ThreadId};
use std::time::Duration;

use hito_core::{
    ClassDef, ClassId, DataType, Dimension, Element, ElementId, ElementStore, Model, ModelId,
    ModelKind, Relationship, RelationshipDef, Schema, SchemaRegistry, SchemaVersion,
};
use hito_geometry::{
    Csg, Frame, Generator, GeometryCache, GeometryError, Kernel, Mesh, Profile, Regenerated, Solid,
};

const SCHEMA: &str = "TestGeometry";
const LENGTH: DataType = DataType::Quantity(Dimension::LENGTH);

fn class(name: &str) -> ClassId {
    ClassId::new(SCHEMA, name)
}

fn schema() -> Schema {
    Schema::new(SCHEMA, SchemaVersion::new(1, 0, 0))
        .class(
            ClassDef::new("Level", ClassId::SPATIAL_LOCATION_ELEMENT).property("Elevation", LENGTH),
        )
        .class(ClassDef::new("ColumnType", ClassId::DEFINITION_ELEMENT).property("Width", LENGTH))
        .class(
            ClassDef::new("Column", ClassId::GEOMETRIC_ELEMENT_3D)
                .typed_by(class("ColumnType"))
                .property("X", LENGTH)
                .property("Height", LENGTH),
        )
        // A subclass with no generator of its own uses the column's.
        .class(ClassDef::new("RoundColumn", class("Column")))
        .class(ClassDef::new("Slab", ClassId::GEOMETRIC_ELEMENT_3D).property("Thickness", LENGTH))
        .class(ClassDef::new("Opening", ClassId::GEOMETRIC_ELEMENT_3D).property("Width", LENGTH))
        .relationship(RelationshipDef::new(
            "OnLevel",
            class("Column"),
            class("Level"),
        ))
        .relationship(RelationshipDef::new(
            "Hosts",
            class("Opening"),
            class("Slab"),
        ))
}

/// Calls to the generators, per element.
#[derive(Default)]
struct Calls(Mutex<Vec<ElementId>>);

impl Calls {
    fn take(&self) -> HashSet<ElementId> {
        self.0.lock().unwrap().drain(..).collect()
    }
}

/// A column: a square of its type's width, standing on its level.
/// A slab: 4 m square, minus its openings.
fn generators(calls: Arc<Calls>) -> Vec<Generator> {
    let column_calls = calls.clone();
    let column = Generator::new(class("Column"), move |inputs, column| {
        column_calls.0.lock().unwrap().push(column.id());
        let level = inputs.relationships_from(column.id()).next()?.target();
        let width = inputs.real(column.id(), "Width")?;
        Some(Solid::Extrusion {
            profile: Profile::Rectangle {
                width,
                height: width,
            },
            frame: Frame::at([
                inputs.real(column.id(), "X")?,
                0.0,
                inputs.real(level, "Elevation")?,
            ]),
            depth: inputs.real(column.id(), "Height")?,
        })
    });
    let slab = Generator::new(class("Slab"), move |inputs, slab| {
        calls.0.lock().unwrap().push(slab.id());
        let square = |width: f64, depth: f64| Solid::Extrusion {
            profile: Profile::Rectangle {
                width,
                height: width,
            },
            frame: Frame::WORLD,
            depth,
        };
        let thickness = inputs.real(slab.id(), "Thickness")?;
        let openings: Vec<_> = inputs.relationships_to(slab.id()).collect();
        let voids = openings
            .into_iter()
            .filter_map(|r| Some(square(inputs.real(r.source(), "Width")?, thickness)))
            .collect();
        Some(Solid::Difference {
            base: Box::new(square(4.0, thickness)),
            voids,
        })
    });
    vec![column, slab]
}

/// Takes a difference as its base, after counting it: enough to see the
/// program the cache sends. Real booleans are tested with the Manifold
/// adapter.
#[derive(Default)]
struct TestKernel {
    voids: AtomicUsize,
}

impl Kernel for TestKernel {
    fn evaluate(&self, csg: &Csg) -> Result<Mesh, GeometryError> {
        match csg {
            Csg::Difference { base, voids } => {
                self.voids.fetch_add(voids.len(), Ordering::Relaxed);
                self.evaluate(base)
            }
            Csg::Mesh(mesh) => Ok(mesh.clone()),
            Csg::Union(_) => Err(GeometryError::Kernel("no unions here".into())),
        }
    }
}

struct Project {
    store: ElementStore,
    model: ModelId,
    cache: GeometryCache,
    kernel: Arc<TestKernel>,
    calls: Arc<Calls>,
}

fn project() -> Project {
    let mut registry = SchemaRegistry::new();
    registry.register(schema()).unwrap();
    let mut store = ElementStore::new(registry);
    let mut tx = store.transaction();
    let model = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
    tx.commit().unwrap();
    let kernel = Arc::new(TestKernel::default());
    let calls = Arc::new(Calls::default());
    let cache = GeometryCache::new(kernel.clone(), generators(calls.clone()));
    Project {
        store,
        model,
        cache,
        kernel,
        calls,
    }
}

impl Project {
    /// Inserts elements and relationships in one transaction, and updates
    /// the cache with its change set.
    fn insert(
        &mut self,
        elements: Vec<Element>,
        relationships: Vec<Relationship>,
    ) -> (Vec<ElementId>, Regenerated) {
        let mut tx = self.store.transaction();
        let ids = elements
            .into_iter()
            .map(|e| tx.insert(e).unwrap())
            .collect();
        relationships
            .into_iter()
            .for_each(|r| tx.relate(r).unwrap());
        let changes = tx.commit().unwrap();
        (ids, self.cache.update(&self.store, &changes))
    }

    fn set(&mut self, id: ElementId, name: &str, value: f64) -> Regenerated {
        let mut tx = self.store.transaction();
        tx.set_property(id, name, value).unwrap();
        let changes = tx.commit().unwrap();
        self.cache.update(&self.store, &changes)
    }

    fn element(&self, name: &str) -> Element {
        Element::new(class(name), self.model)
    }

    /// A level, a column type and `n` columns on them, 1 m apart.
    fn columns(&mut self, n: usize) -> (ElementId, ElementId, Vec<ElementId>) {
        let level = self.element("Level").with_property("Elevation", 3.0);
        let column_type = self.element("ColumnType").with_property("Width", 0.3);
        let (ids, _) = self.insert(vec![level, column_type], vec![]);
        let (level, column_type) = (ids[0], ids[1]);
        let columns = (0..n)
            .map(|i| {
                self.element("Column")
                    .with_type(column_type)
                    .with_property("X", i as f64)
                    .with_property("Height", 2.8)
            })
            .collect();
        let (columns, _) = self.insert(columns, vec![]);
        let on_level = columns
            .iter()
            .map(|&c| Relationship::new(class("OnLevel"), c, level))
            .collect();
        self.insert(vec![], on_level);
        self.calls.take();
        (level, column_type, columns)
    }

    fn volume(&self, id: ElementId) -> f64 {
        self.cache.mesh(id).unwrap().volume()
    }
}

fn sorted(mut ids: Vec<ElementId>) -> Vec<ElementId> {
    ids.sort();
    ids
}

#[test]
fn elements_get_their_geometry_from_their_parameters() {
    let mut p = project();
    let (_, _, columns) = p.columns(2);
    for (i, &column) in columns.iter().enumerate() {
        let mesh = p.cache.mesh(column).unwrap();
        assert!(mesh.is_closed());
        assert!((mesh.volume() - 0.3 * 0.3 * 2.8).abs() < 1e-12);
        let (lo, hi) = mesh.bounds().unwrap();
        assert_eq!([lo[0], lo[2]], [i as f64 - 0.15, 3.0]);
        assert!((hi[2] - 5.8).abs() < 1e-12);
    }
}

#[test]
fn a_changed_parameter_regenerates_only_that_element() {
    let mut p = project();
    let (_, _, columns) = p.columns(3);
    let done = p.set(columns[1], "Height", 4.0);
    assert_eq!(done.updated, [columns[1]]);
    assert_eq!(p.calls.take(), HashSet::from([columns[1]]));
    assert!((p.volume(columns[1]) - 0.3 * 0.3 * 4.0).abs() < 1e-12);
    assert!((p.volume(columns[0]) - 0.3 * 0.3 * 2.8).abs() < 1e-12);
}

#[test]
fn a_related_element_regenerates_what_read_it() {
    let mut p = project();
    let (level, _, columns) = p.columns(3);
    let slab = p.element("Slab").with_property("Thickness", 0.15);
    p.insert(vec![slab], vec![]);
    p.calls.take();

    let done = p.set(level, "Elevation", 3.5);
    assert_eq!(sorted(done.updated), sorted(columns.clone()));
    assert_eq!(p.calls.take(), HashSet::from_iter(columns.clone()));
    let (lo, _) = p.cache.mesh(columns[0]).unwrap().bounds().unwrap();
    assert_eq!(lo[2], 3.5);
}

#[test]
fn a_changed_type_regenerates_its_instances() {
    let mut p = project();
    let (_, column_type, columns) = p.columns(2);
    let done = p.set(column_type, "Width", 0.4);
    assert_eq!(sorted(done.updated), sorted(columns.clone()));
    assert!((p.volume(columns[0]) - 0.4 * 0.4 * 2.8).abs() < 1e-12);
}

#[test]
fn a_subclass_uses_its_base_class_generator() {
    let mut p = project();
    let (level, column_type, _) = p.columns(0);
    let round = p
        .element("RoundColumn")
        .with_type(column_type)
        .with_property("X", 0.0)
        .with_property("Height", 3.0);
    let (ids, _) = p.insert(vec![round], vec![]);
    let on_level = Relationship::new(class("OnLevel"), ids[0], level);
    let done = p.insert(vec![], vec![on_level]).1;
    assert_eq!(done.updated, ids);
    assert!((p.volume(ids[0]) - 0.3 * 0.3 * 3.0).abs() < 1e-12);
}

#[test]
fn openings_reach_the_kernel_as_voids() {
    let mut p = project();
    let slab = p.element("Slab").with_property("Thickness", 0.15);
    let (slab, done) = p.insert(vec![slab], vec![]);
    assert_eq!(done.updated, slab);
    assert_eq!(p.kernel.voids.load(Ordering::Relaxed), 0);

    // Adding an opening regenerates the slab it's related to.
    let opening = p.element("Opening").with_property("Width", 1.0);
    let (opening, _) = p.insert(vec![opening], vec![]);
    let hosts = Relationship::new(class("Hosts"), opening[0], slab[0]);
    let done = p.insert(vec![], vec![hosts]).1;
    assert_eq!(done.updated, slab);
    assert_eq!(p.kernel.voids.load(Ordering::Relaxed), 1);

    // And so does resizing it.
    p.calls.take();
    let done = p.set(opening[0], "Width", 1.5);
    assert_eq!(done.updated, slab);
    assert_eq!(p.kernel.voids.load(Ordering::Relaxed), 2);
}

#[test]
fn deleted_elements_lose_their_geometry() {
    let mut p = project();
    let (level, _, columns) = p.columns(2);
    let mut tx = p.store.transaction();
    let deleted = tx.delete(level).unwrap();
    let changes = tx.commit().unwrap();
    assert!(deleted.contains(&level));
    // The columns lost their level, and their generator now returns nothing.
    let done = p.cache.update(&p.store, &changes);
    assert_eq!(sorted(done.removed), sorted(columns.clone()));
    assert!(p.cache.mesh(columns[0]).is_none());
    assert_eq!(p.cache.meshes().count(), 0);
}

#[test]
fn bad_parameters_keep_an_error_instead_of_a_mesh() {
    let mut p = project();
    let (_, _, columns) = p.columns(1);
    p.set(columns[0], "Height", -1.0);
    assert!(p.cache.mesh(columns[0]).is_none());
    assert_eq!(
        p.cache.error(columns[0]),
        Some(&GeometryError::InvalidLength(-1.0))
    );
    p.set(columns[0], "Height", 1.0);
    assert!(p.cache.error(columns[0]).is_none());
}

#[test]
fn a_rebuild_generates_every_element_the_same() {
    let mut p = project();
    let (_, _, columns) = p.columns(5);
    let before: Vec<_> = columns.iter().map(|&c| p.cache.mesh(c).cloned()).collect();
    let done = p.cache.rebuild(&p.store);
    assert_eq!(sorted(done.updated), sorted(columns.clone()));
    assert!(done.removed.is_empty());
    let after: Vec<_> = columns.iter().map(|&c| p.cache.mesh(c).cloned()).collect();
    assert_eq!(before, after);
}

#[test]
fn elements_are_generated_in_parallel() {
    let threads = Arc::new(Mutex::new(HashSet::<ThreadId>::new()));
    let seen = threads.clone();
    let slow = Generator::new(class("Slab"), move |_, _| {
        seen.lock().unwrap().insert(thread::current().id());
        thread::sleep(Duration::from_millis(5));
        None
    });
    let mut p = project();
    p.cache = GeometryCache::new(Arc::new(TestKernel::default()), [slow]);
    let slabs: Vec<_> = (0..32)
        .map(|_| p.element("Slab").with_property("Thickness", 0.15))
        .collect();
    let mut tx = p.store.transaction();
    for slab in slabs {
        tx.insert(slab).unwrap();
    }
    tx.commit().unwrap();

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    pool.install(|| p.cache.rebuild(&p.store));
    assert!(threads.lock().unwrap().len() > 1);
}
