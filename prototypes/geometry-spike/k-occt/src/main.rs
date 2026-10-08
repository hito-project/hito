//! OpenCascade (C++, LGPL-2.1 with exception) through opencascade-rs.

use common::*;
use glam::DVec3;
use opencascade::primitives::Shape;
use std::collections::HashMap;

type Res<T> = std::result::Result<T, String>;

const KERNEL: &str = "occt";
/// Triangulation deflection, in metres.
const TOL: f64 = 1e-4;

fn v(p: [f64; 3]) -> DVec3 {
    DVec3::new(p[0], p[1], p[2])
}

fn cuboid((min, max): ([f64; 3], [f64; 3])) -> Shape {
    Shape::box_from_corners(v(min), v(max))
}

/// OCCT meshes each face separately, so shared vertices are welded by
/// position before measuring.
fn stats(s: &Shape) -> Res<(f64, usize, bool)> {
    let m = s.mesh_with_tolerance(TOL).map_err(|e| format!("{e:?}"))?;
    let mut ids: HashMap<[i64; 3], u32> = HashMap::new();
    let mut positions = Vec::new();
    let remap: Vec<u32> = m
        .vertices
        .iter()
        .map(|p| {
            let key = [(p.x * 1e7).round() as i64, (p.y * 1e7).round() as i64, (p.z * 1e7).round() as i64];
            *ids.entry(key).or_insert_with(|| {
                positions.push([p.x, p.y, p.z]);
                positions.len() as u32 - 1
            })
        })
        .collect();
    let triangles: Vec<[u32; 3]> = m
        .indices
        .chunks(3)
        .map(|t| [remap[t[0]], remap[t[1]], remap[t[2]]])
        .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
        .collect();
    Ok((mesh_volume(&positions, &triangles), triangles.len(), is_closed(&triangles)))
}

fn wall(w: &Wall) -> Shape {
    cuboid(w.body()).subtract(&cuboid(w.void())).shape
}

fn hole(p: &Plate, c: [f64; 2]) -> Shape {
    let t = p.thickness;
    Shape::cylinder(DVec3::new(c[0], c[1], -t), p.hole_radius, DVec3::Z, 3.0 * t)
}

fn main() {
    let mut out = Vec::new();
    for (case, w) in [("wall-window", wall_with_window()), ("wall-door", wall_with_door())] {
        out.push(run(KERNEL, case, w.exact_volume(), || {
            let (v, t, ok) = stats(&wall(&w))?;
            Ok((v, t, ok, String::new()))
        }));
    }
    let p = plate_with_holes();
    out.push(run(KERNEL, "plate-200-holes", p.exact_volume(), || {
        let mut solid = cuboid(([0.0, 0.0, 0.0], [p.size[0], p.size[1], p.thickness]));
        for c in &p.holes {
            solid = solid.subtract(&hole(&p, *c)).shape;
        }
        let (v, t, ok) = stats(&solid)?;
        Ok((v, t, ok, "true circles, sequential subtraction".into()))
    }));
    let walls = many_walls(10_000);
    let sample = &walls[..1_000];
    let exact: f64 = sample.iter().map(Wall::exact_volume).sum();
    out.push(run(KERNEL, "1k-walls", exact, || {
        let (mut v, mut t, mut ok) = (0.0, 0, true);
        for w in sample {
            let (wv, wt, wok) = stats(&wall(w))?;
            v += wv;
            t += wt;
            ok &= wok;
        }
        Ok((v, t, ok, "far from origin; x10 for 10k".into()))
    }));
    report(&out);
}
