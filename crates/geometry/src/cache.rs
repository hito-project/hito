//! The geometry cache: every element's mesh, kept up to date.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use hito_core::{ChangeSet, ClassId, ElementId, ElementStore, StoreView};
use rayon::prelude::*;

use crate::{CHORD_TOLERANCE, Generator, GeometryError, Kernel, Mesh};

/// Every element's body mesh, generated from its parameters and kept up to
/// date as the model changes.
///
/// [`rebuild`](GeometryCache::rebuild) generates every element, when a file
/// opens. After each commit, undo or redo,
/// [`update`](GeometryCache::update) takes the [`ChangeSet`] and generates
/// again only the elements it affects: those it changed, and those whose
/// generator read one of them. Both run the generators in parallel.
///
/// Meshes are shared as [`Arc`]s, so a view can keep the ones it draws.
pub struct GeometryCache {
    kernel: Arc<dyn Kernel>,
    generators: HashMap<ClassId, Generator>,
    tolerance: f64,
    meshes: HashMap<ElementId, Result<Arc<Mesh>, GeometryError>>,
    /// What each generated element read, and the other way round.
    inputs: HashMap<ElementId, Vec<ElementId>>,
    dependents: HashMap<ElementId, HashSet<ElementId>>,
}

/// What an [`update`](GeometryCache::update) or
/// [`rebuild`](GeometryCache::rebuild) did, so views redraw only that.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Regenerated {
    /// Elements whose geometry, or geometry error, was generated again.
    pub updated: Vec<ElementId>,
    /// Elements that had geometry and no longer have any: deleted, or
    /// their generator returned nothing.
    pub removed: Vec<ElementId>,
}

impl GeometryCache {
    /// An empty cache that meshes with `kernel` and generates elements with
    /// the domains' `generators`, at most one per class. An element of a
    /// class without a generator uses its nearest base class's.
    pub fn new(kernel: Arc<dyn Kernel>, generators: impl IntoIterator<Item = Generator>) -> Self {
        Self {
            kernel,
            generators: generators
                .into_iter()
                .map(|g| (g.class().clone(), g))
                .collect(),
            tolerance: CHORD_TOLERANCE,
            meshes: HashMap::new(),
            inputs: HashMap::new(),
            dependents: HashMap::new(),
        }
    }

    /// The element's mesh, `None` if it has no geometry or it failed.
    pub fn mesh(&self, id: ElementId) -> Option<&Arc<Mesh>> {
        self.meshes.get(&id)?.as_ref().ok()
    }

    /// Why the element's geometry failed, if it did.
    pub fn error(&self, id: ElementId) -> Option<&GeometryError> {
        self.meshes.get(&id)?.as_ref().err()
    }

    /// Every element's mesh, in no particular order.
    pub fn meshes(&self) -> impl Iterator<Item = (ElementId, &Arc<Mesh>)> {
        let meshes = self.meshes.iter();
        meshes.filter_map(|(id, mesh)| Some((*id, mesh.as_ref().ok()?)))
    }

    /// Generates every element's geometry again, from scratch.
    pub fn rebuild(&mut self, store: &ElementStore) -> Regenerated {
        let removed = self.meshes.keys().copied().collect();
        self.meshes.clear();
        self.inputs.clear();
        self.dependents.clear();
        let all: Vec<_> = store.elements().map(|e| e.id()).collect();
        let mut done = self.regenerate(store.view(), all);
        done.removed = removed;
        done.removed.retain(|id| !self.meshes.contains_key(id));
        done
    }

    /// Generates again the geometry of the elements a committed change set
    /// affects. `store` must be the store the change set was committed to.
    pub fn update(&mut self, store: &ElementStore, changes: &ChangeSet) -> Regenerated {
        let mut changed: HashSet<ElementId> = changes.elements().iter().map(|c| c.id()).collect();
        for change in changes.relationships() {
            let r = change.relationship();
            changed.extend([r.source(), r.target()]);
        }
        let mut affected = changed.clone();
        for id in &changed {
            affected.extend(self.dependents.get(id).into_iter().flatten());
        }
        self.regenerate(store.view(), affected.into_iter().collect())
    }

    /// Generates `ids` in parallel, replacing what the cache had for them.
    fn regenerate(&mut self, view: StoreView<'_>, ids: Vec<ElementId>) -> Regenerated {
        for id in &ids {
            self.forget_inputs(*id);
        }
        let (kernel, tolerance) = (self.kernel.as_ref(), self.tolerance);
        let generated: Vec<_> = ids
            .par_iter()
            .map(|&id| {
                let element = view.get(id);
                let generator = element.and_then(|e| self.generator(view, e.class()));
                let (solid, inputs) = match (element, generator) {
                    (Some(element), Some(generator)) => generator.generate(view, element),
                    _ => (None, Vec::new()),
                };
                let mesh = solid.map(|s| s.mesh(kernel, tolerance).map(Arc::new));
                (id, mesh, inputs)
            })
            .collect();

        let mut done = Regenerated::default();
        for (id, mesh, inputs) in generated {
            if !inputs.is_empty() {
                for input in &inputs {
                    self.dependents.entry(*input).or_default().insert(id);
                }
                self.inputs.insert(id, inputs);
            }
            match mesh {
                Some(mesh) => {
                    self.meshes.insert(id, mesh);
                    done.updated.push(id);
                }
                None => {
                    if self.meshes.remove(&id).is_some() {
                        done.removed.push(id);
                    }
                }
            }
        }
        done
    }

    fn forget_inputs(&mut self, id: ElementId) {
        for input in self.inputs.remove(&id).into_iter().flatten() {
            if let Some(dependents) = self.dependents.get_mut(&input) {
                dependents.remove(&id);
                if dependents.is_empty() {
                    self.dependents.remove(&input);
                }
            }
        }
    }

    /// The generator for a class: its own, or its nearest base class's.
    fn generator(&self, view: StoreView<'_>, class: &ClassId) -> Option<&Generator> {
        let registry = view.registry();
        let mut class = Some(class);
        while let Some(id) = class {
            if let Some(generator) = self.generators.get(id) {
                return Some(generator);
            }
            class = registry.class(id).and_then(|c| c.base());
        }
        None
    }
}
