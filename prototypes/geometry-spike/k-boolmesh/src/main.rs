//! boolmesh (pure Rust, MPL-2.0): mesh booleans inspired by Manifold.

use boolmesh::prelude::*;
use common::*;

const KERNEL: &str = "boolmesh";

fn to_manifold(m: &Mesh) -> Result<Manifold, String> {
    let pos: Vec<f64> = m.positions.iter().flatten().copied().collect();
    let idx: Vec<usize> = m.triangles.iter().flatten().map(|&i| i as usize).collect();
    Manifold::new(&pos, &idx)
}

fn stats(m: &Manifold) -> (f64, usize, bool) {
    let positions: Vec<[f64; 3]> = m.ps.iter().map(|p| [p.x, p.y, p.z]).collect();
    let triangles: Vec<[u32; 3]> = m
        .hs
        .chunks(3)
        .map(|h| [h[0].tail as u32, h[1].tail as u32, h[2].tail as u32])
        .collect();
    let closed = is_closed(&triangles) && m.is_manifold();
    (mesh_volume(&positions, &triangles), triangles.len(), closed)
}

fn wall(w: &Wall) -> Result<Manifold, String> {
    let (b0, b1) = w.body();
    let (v0, v1) = w.void();
    let body = to_manifold(&Mesh::cuboid(b0, b1))?;
    let void = to_manifold(&Mesh::cuboid(v0, v1))?;
    compute_boolean(&body, &void, OpType::Subtract)
}

fn plate(p: &Plate) -> Result<Manifold, String> {
    let t = p.thickness;
    let mut solid = to_manifold(&Mesh::cuboid([0.0, 0.0, 0.0], [p.size[0], p.size[1], t]))?;
    for h in &p.holes {
        let hole = to_manifold(&Mesh::cylinder(*h, p.hole_radius, -t, 2.0 * t, CIRCLE_SEGMENTS))?;
        solid = compute_boolean(&solid, &hole, OpType::Subtract)?;
    }
    Ok(solid)
}

/// Unions all holes in a balanced tree, then subtracts them once.
fn plate_batched(p: &Plate) -> Result<Manifold, String> {
    let t = p.thickness;
    let solid = to_manifold(&Mesh::cuboid([0.0, 0.0, 0.0], [p.size[0], p.size[1], t]))?;
    let mut parts: Vec<Manifold> = p
        .holes
        .iter()
        .map(|h| to_manifold(&Mesh::cylinder(*h, p.hole_radius, -t, 2.0 * t, CIRCLE_SEGMENTS)))
        .collect::<Result<_, _>>()?;
    while parts.len() > 1 {
        let mut next = Vec::with_capacity(parts.len().div_ceil(2));
        for pair in parts.chunks(2) {
            next.push(match pair {
                [a, b] => compute_boolean(a, b, OpType::Add)?,
                [a] => a.clone(),
                _ => unreachable!(),
            });
        }
        parts = next;
    }
    compute_boolean(&solid, &parts[0], OpType::Subtract)
}

fn main() {
    let mut out = Vec::new();
    for (case, w) in [("wall-window", wall_with_window()), ("wall-door", wall_with_door())] {
        out.push(run(KERNEL, case, w.exact_volume(), || {
            let (v, t, ok) = stats(&wall(&w)?);
            Ok((v, t, ok, String::new()))
        }));
    }
    let p = plate_with_holes();
    out.push(run(KERNEL, "plate-200-holes", p.exact_volume(), || {
        let (v, t, ok) = stats(&plate(&p)?);
        Ok((v, t, ok, format!("{CIRCLE_SEGMENTS}-gon holes, sequential subtraction")))
    }));
    out.push(run(KERNEL, "plate-200-holes-batched", p.exact_volume(), || {
        let (v, t, ok) = stats(&plate_batched(&p)?);
        Ok((v, t, ok, format!("{CIRCLE_SEGMENTS}-gon holes, union tree then one subtraction")))
    }));
    let (road, nominal) = corridor();
    out.push(run(KERNEL, "corridor-sweep", nominal, || {
        let (v, t, ok) = stats(&to_manifold(&road)?);
        Ok((v, t, ok, "200 m clothoid sweep built by our code, imported".into()))
    }));
    out.push(run(KERNEL, "corridor-minus-culvert", f64::NAN, || {
        let r = compute_boolean(&to_manifold(&road)?, &to_manifold(&culvert())?, OpType::Subtract)?;
        let (v, t, ok) = stats(&r);
        Ok((v, t, ok, "box culvert cut across the sweep".into()))
    }));
    let walls = many_walls(10_000);
    let exact: f64 = walls.iter().map(Wall::exact_volume).sum();
    out.push(run(KERNEL, "10k-walls", exact, || {
        let (mut v, mut t, mut ok) = (0.0, 0, true);
        for w in &walls {
            let (wv, wt, wok) = stats(&wall(w)?);
            v += wv;
            t += wt;
            ok &= wok;
        }
        Ok((v, t, ok, "far from origin, single thread".into()))
    }));
    report(&out);
}
