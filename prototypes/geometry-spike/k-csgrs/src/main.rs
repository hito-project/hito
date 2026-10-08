//! csgrs (pure Rust, MIT), repository head: mesh booleans on
//! exact-aware `hyperreal` scalars, driven through its f64 adapter.

use common::*;
use csgrs::adapter::TriangleMeshF64 as CsgMesh;

type Res<T> = std::result::Result<T, String>;

const KERNEL: &str = "csgrs";

fn to_csg(m: &Mesh) -> Res<CsgMesh> {
    let faces: Vec<[usize; 3]> =
        m.triangles.iter().map(|t| [t[0] as usize, t[1] as usize, t[2] as usize]).collect();
    let refs: Vec<&[usize]> = faces.iter().map(|f| f.as_slice()).collect();
    CsgMesh::polyhedron(&m.positions, &refs).map_err(|e| format!("{e:?}"))
}

fn stats(m: &CsgMesh) -> Res<(f64, usize, bool)> {
    let (positions, triangles) = m.vertices_and_indices().map_err(|e| format!("{e:?}"))?;
    Ok((mesh_volume(&positions, &triangles), triangles.len(), is_closed(&triangles)))
}

fn wall(w: &Wall) -> Res<CsgMesh> {
    let (b0, b1) = w.body();
    let (v0, v1) = w.void();
    to_csg(&Mesh::cuboid(b0, b1))?
        .difference(&to_csg(&Mesh::cuboid(v0, v1))?)
        .map_err(|e| format!("{e:?}"))
}

fn main() {
    let mut out = Vec::new();
    for (case, w) in [("wall-window", wall_with_window()), ("wall-door", wall_with_door())] {
        out.push(run(KERNEL, case, w.exact_volume(), || {
            let (v, t, ok) = stats(&wall(&w)?)?;
            Ok((v, t, ok, String::new()))
        }));
    }
    let p = plate_with_holes();
    for n in [10usize, 50, 200] {
        let case: &'static str = match n {
            10 => "plate-10-holes",
            50 => "plate-50-holes",
            _ => "plate-200-holes",
        };
        let sub = Plate { holes: p.holes[..n].to_vec(), ..p.clone() };
        let o = run(KERNEL, case, sub.exact_volume(), || {
            let t = sub.thickness;
            let mut solid = to_csg(&Mesh::cuboid([0.0, 0.0, 0.0], [sub.size[0], sub.size[1], t]))?;
            for c in &sub.holes {
                let hole = to_csg(&Mesh::cylinder(*c, sub.hole_radius, -t, 2.0 * t, CIRCLE_SEGMENTS))?;
                solid = solid.difference(&hole).map_err(|e| format!("{e:?}"))?;
            }
            let (v, tri, ok) = stats(&solid)?;
            Ok((v, tri, ok, format!("{CIRCLE_SEGMENTS}-gon holes, sequential subtraction")))
        });
        let stop = !o.ok || o.elapsed.as_secs() > 60;
        out.push(o);
        if stop {
            break;
        }
    }
    let walls = many_walls(10_000);
    let sample = &walls[..1_000];
    let exact: f64 = sample.iter().map(Wall::exact_volume).sum();
    out.push(run(KERNEL, "1k-walls", exact, || {
        let (mut v, mut t, mut ok) = (0.0, 0, true);
        for w in sample {
            let (wv, wt, wok) = stats(&wall(w)?)?;
            v += wv;
            t += wt;
            ok &= wok;
        }
        Ok((v, t, ok, "far from origin; x10 for 10k".into()))
    }));
    report(&out);
}
