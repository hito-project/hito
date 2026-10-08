//! monstertruck (pure Rust, Apache-2.0): a 2026 fork of truck with
//! reworked booleans and meshing.

use common::*;
use monstertruck_meshing::prelude::*;
use monstertruck_modeling::*;

type Res<T> = std::result::Result<T, String>;

const KERNEL: &str = "monstertruck";
/// Boolean and meshing tolerance, in metres.
const TOL: f64 = 1e-4;

fn cuboid((min, max): ([f64; 3], [f64; 3])) -> Solid {
    let v = builder::vertex(Point3::new(min[0], min[1], min[2]));
    let e: Edge = builder::extrude(&v, Vector3::unit_x() * (max[0] - min[0]));
    let f: Face = builder::extrude(&e, Vector3::unit_y() * (max[1] - min[1]));
    builder::extrude(&f, Vector3::unit_z() * (max[2] - min[2]))
}

/// A true circular cylinder (exact B-rep, not a polygon).
fn cylinder(c: [f64; 2], r: f64, z0: f64, z1: f64) -> Res<Solid> {
    let v = builder::vertex(Point3::new(c[0] + r, c[1], z0));
    let circle: Wire = builder::revolve(
        &v,
        Point3::new(c[0], c[1], z0),
        Vector3::unit_z(),
        builder::SweepAngle::Closed,
        4,
    );
    let face: Face = builder::try_attach_plane(vec![circle]).map_err(|e| format!("{e:?}"))?;
    Ok(builder::extrude(&face, Vector3::unit_z() * (z1 - z0)))
}

fn difference(a: &Solid, b: &Solid) -> Res<Solid> {
    monstertruck_solid::difference(a, b, TOL).map_err(|e| format!("{e:?}"))
}

/// Tessellates the solid, then measures the mesh like every other kernel.
fn stats(s: &Solid) -> (f64, usize, bool) {
    let mut poly = s.triangulation(TOL).to_polygon();
    poly.put_together_same_attrs(TOL * 0.1);
    let positions: Vec<[f64; 3]> = poly.positions().iter().map(|p| [p.x, p.y, p.z]).collect();
    let triangles: Vec<[u32; 3]> = poly
        .faces()
        .triangle_iter()
        .map(|t| [t[0].pos as u32, t[1].pos as u32, t[2].pos as u32])
        .collect();
    (mesh_volume(&positions, &triangles), triangles.len(), is_closed(&triangles))
}

fn wall(w: &Wall) -> Res<Solid> {
    difference(&cuboid(w.body()), &cuboid(w.void()))
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
    // Time-boxed: grow the hole count and stop at the first failure or
    // when one step takes over a minute.
    for n in [1usize, 10, 50, 200] {
        let case: &'static str = match n {
            1 => "plate-1-hole",
            10 => "plate-10-holes",
            50 => "plate-50-holes",
            _ => "plate-200-holes",
        };
        let sub = Plate { holes: p.holes[..n].to_vec(), ..p.clone() };
        let exact = sub.exact_volume();
        let o = run(KERNEL, case, exact, || {
            let t = sub.thickness;
            let mut solid = cuboid(([0.0, 0.0, 0.0], [sub.size[0], sub.size[1], t]));
            for c in &sub.holes {
                solid = difference(&solid, &cylinder(*c, sub.hole_radius, -t, 2.0 * t)?)?;
            }
            let (v, tri, ok) = stats(&solid);
            Ok((v, tri, ok, "true circles, sequential subtraction".into()))
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
            let (wv, wt, wok) = stats(&wall(w)?);
            v += wv;
            t += wt;
            ok &= wok;
        }
        Ok((v, t, ok, "far from origin; x10 for 10k".into()))
    }));
    report(&out);
}
