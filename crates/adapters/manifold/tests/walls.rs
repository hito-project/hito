//! Joined walls (#193) through the geometry cache and Manifold: each
//! layer is extruded and the layers are unioned into one closed solid
//! whose volume is the footprint's area times the height.

use std::sync::Arc;

use hito_architectural::footprint;
use hito_architectural::plan::area;
use hito_architectural::wall::{self, Placement};
use hito_architectural::wall_type::{self, Function, Layer};
use hito_core::{Element, ElementStore, Model, ModelKind, SchemaRegistry};
use hito_geometry::GeometryCache;
use hito_manifold::Manifold;
use hito_structural::level;

#[test]
fn a_ring_of_joined_walls_meshes_closed() {
    let mut registry = SchemaRegistry::new();
    hito_architectural::register(&mut registry).unwrap();
    let mut store = ElementStore::new(registry);
    let mut tx = store.transaction();
    let model = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
    let ground = Element::new(hito_structural::class(level::LEVEL), model)
        .with_property(level::NAME, "Planta baja")
        .with_property(level::ELEVATION, 0.0);
    let ground = tx.insert(ground).unwrap();
    let brick = wall_type::insert(
        &mut tx,
        model,
        "Ladrillo 18 revocado",
        &[
            Layer::new(Function::Finish1, 0.02),
            Layer::new(Function::Structure, 0.18),
            Layer::new(Function::Finish2, 0.02),
        ],
    )
    .unwrap();
    // A ring far from the origin, with an inner wall abutting it.
    let o = [5_500_000.0, 6_100_000.0];
    let at = |x: f64, y: f64| [o[0] + x, o[1] + y];
    let corners = [
        at(0.0, 0.0),
        at(6.0, 0.0),
        at(6.0, 4.0),
        at(0.0, 4.0),
        at(0.0, 0.0),
    ];
    let mut walls = Vec::new();
    for pair in corners.windows(2) {
        let placement = Placement::new(brick, ground, pair[0], pair[1]);
        walls.push(wall::insert(&mut tx, model, &placement).unwrap());
    }
    let inner = Placement::new(brick, ground, at(3.0, 0.0), at(3.0, 4.0));
    walls.push(wall::insert(&mut tx, model, &inner).unwrap());
    tx.commit().unwrap();

    let mut cache = GeometryCache::new(Arc::new(Manifold), hito_architectural::generators());
    cache.rebuild(&store);
    let view = store.view();
    for wall in walls {
        let mesh = cache.mesh(wall).expect("the wall has geometry");
        assert!(mesh.is_closed());
        let footprint = footprint::footprint(&view, wall).unwrap();
        let expected: f64 = footprint
            .slices
            .iter()
            .map(|s| {
                let plan: f64 = s
                    .layers
                    .iter()
                    .flat_map(|l| &l.polygons)
                    .map(|p| area(p))
                    .sum();
                plan * (s.top - s.bottom)
            })
            .sum();
        assert!(
            (mesh.volume() - expected).abs() < 1e-6,
            "{} vs {expected}",
            mesh.volume()
        );
    }
}
