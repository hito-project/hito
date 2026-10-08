//! Manifold (C++, Apache-2.0) through the manifold-csg bindings.

use common::*;
use manifold_csg::Manifold;

const KERNEL: &str = "manifold";

fn cuboid((min, max): ([f64; 3], [f64; 3])) -> Manifold {
    Manifold::cube(max[0] - min[0], max[1] - min[1], max[2] - min[2], false)
        .translate(min[0], min[1], min[2])
}

fn from_mesh(m: &Mesh) -> Result<Manifold, String> {
    let pos: Vec<f64> = m.positions.iter().flatten().copied().collect();
    let idx: Vec<u64> = m.triangles.iter().flatten().map(|&i| i as u64).collect();
    Manifold::from_mesh_f64(&pos, 3, &idx).map_err(|e| format!("{e:?}"))
}

fn stats(m: &Manifold) -> Result<(f64, usize, bool), String> {
    m.status().map_err(|e| format!("{e:?}"))?;
    let (pos, stride, idx) = m.to_mesh_f64();
    let positions: Vec<[f64; 3]> = pos.chunks(stride).map(|p| [p[0], p[1], p[2]]).collect();
    let triangles: Vec<[u32; 3]> =
        idx.chunks(3).map(|t| [t[0] as u32, t[1] as u32, t[2] as u32]).collect();
    // Manifold's own volume works in absolute coordinates; ours shifts to
    // the first vertex. Report ours, so all kernels are measured the same way.
    Ok((mesh_volume(&positions, &triangles), triangles.len(), is_closed(&triangles)))
}

fn wall(w: &Wall) -> Manifold {
    cuboid(w.body()).difference(&cuboid(w.void()))
}

fn hole(p: &Plate, c: [f64; 2]) -> Manifold {
    let t = p.thickness;
    Manifold::cylinder(3.0 * t, p.hole_radius, p.hole_radius, CIRCLE_SEGMENTS as i32, false)
        .translate(c[0], c[1], -t)
}

fn plate_box(p: &Plate) -> Manifold {
    cuboid(([0.0, 0.0, 0.0], [p.size[0], p.size[1], p.thickness]))
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
        let mut solid = plate_box(&p);
        for c in &p.holes {
            solid = solid.difference(&hole(&p, *c));
        }
        let (v, t, ok) = stats(&solid)?;
        Ok((v, t, ok, format!("{CIRCLE_SEGMENTS}-gon holes, sequential subtraction")))
    }));
    out.push(run(KERNEL, "plate-200-holes-batched", p.exact_volume(), || {
        let mut parts = vec![plate_box(&p)];
        parts.extend(p.holes.iter().map(|c| hole(&p, *c)));
        let (v, t, ok) = stats(&Manifold::batch_difference(&parts))?;
        Ok((v, t, ok, format!("{CIRCLE_SEGMENTS}-gon holes, batch difference")))
    }));
    let (road, nominal) = corridor();
    out.push(run(KERNEL, "corridor-sweep", nominal, || {
        let (v, t, ok) = stats(&from_mesh(&road)?)?;
        Ok((v, t, ok, "200 m clothoid sweep built by our code, imported".into()))
    }));
    out.push(run(KERNEL, "corridor-minus-culvert", f64::NAN, || {
        let r = from_mesh(&road)?.difference(&from_mesh(&culvert())?);
        let (v, t, ok) = stats(&r)?;
        Ok((v, t, ok, "box culvert cut across the sweep".into()))
    }));
    let walls = many_walls(10_000);
    let exact: f64 = walls.iter().map(Wall::exact_volume).sum();
    out.push(run(KERNEL, "10k-walls", exact, || {
        let (mut v, mut t, mut ok) = (0.0, 0, true);
        for w in &walls {
            let (wv, wt, wok) = stats(&wall(w))?;
            v += wv;
            t += wt;
            ok &= wok;
        }
        Ok((v, t, ok, "far from origin; Manifold may use its own threads".into()))
    }));
    report(&out);
}
