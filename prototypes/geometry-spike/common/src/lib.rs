//! Shared test cases for the geometry kernel spike (#2).
//!
//! Every kernel prototype builds the same cases and reports through
//! [`Outcome`], so results can be compared side by side.

use std::f64::consts::PI;
use std::time::{Duration, Instant};

/// A rectangular opening in a wall, in the wall's local coordinates.
#[derive(Clone, Copy, Debug)]
pub struct Opening {
    /// Distance from the wall start to the opening's left edge.
    pub x: f64,
    pub width: f64,
    /// Height of the opening's bottom edge above the wall base.
    pub sill: f64,
    pub height: f64,
}

/// A straight wall: a box from `origin` along +x, with one opening.
///
/// The opening box overshoots the wall faces by `OVERSHOOT` in y, as IFC
/// opening elements usually do. A door (`sill == 0`) shares the wall's
/// bottom face, which is a deliberate coplanar case.
#[derive(Clone, Copy, Debug)]
pub struct Wall {
    pub origin: [f64; 3],
    pub length: f64,
    pub thickness: f64,
    pub height: f64,
    pub opening: Opening,
}

pub const OVERSHOOT: f64 = 0.05;

impl Wall {
    pub fn exact_volume(&self) -> f64 {
        let o = self.opening;
        self.length * self.thickness * self.height - o.width * o.height * self.thickness
    }

    /// The wall box as (min, max).
    pub fn body(&self) -> ([f64; 3], [f64; 3]) {
        let [x, y, z] = self.origin;
        ([x, y, z], [x + self.length, y + self.thickness, z + self.height])
    }

    /// The opening box as (min, max), overshooting the wall in y.
    pub fn void(&self) -> ([f64; 3], [f64; 3]) {
        let [x, y, z] = self.origin;
        let o = self.opening;
        (
            [x + o.x, y - OVERSHOOT, z + o.sill],
            [x + o.x + o.width, y + self.thickness + OVERSHOOT, z + o.sill + o.height],
        )
    }
}

/// A 5 m concrete wall, 20 cm thick, 3 m high, with a 1.2 x 1.2 m window.
pub fn wall_with_window() -> Wall {
    Wall {
        origin: [0.0, 0.0, 0.0],
        length: 5.0,
        thickness: 0.2,
        height: 3.0,
        opening: Opening { x: 1.5, width: 1.2, sill: 0.9, height: 1.2 },
    }
}

/// The same wall with a 1.0 x 2.1 m door that starts at the wall base.
pub fn wall_with_door() -> Wall {
    Wall {
        opening: Opening { x: 2.0, width: 1.0, sill: 0.0, height: 2.1 },
        ..wall_with_window()
    }
}

/// `n` walls with windows, laid out on a grid far from the origin
/// (Gauss-Krüger-like coordinates), with varying sizes.
pub fn many_walls(n: usize) -> Vec<Wall> {
    let base = [5_500_000.0, 6_100_000.0, 0.0];
    (0..n)
        .map(|i| {
            let col = (i % 100) as f64;
            let row = (i / 100) as f64;
            let length = 3.0 + (i % 7) as f64 * 0.5;
            Wall {
                origin: [base[0] + col * 10.0, base[1] + row * 10.0, base[2]],
                length,
                thickness: 0.15 + (i % 3) as f64 * 0.05,
                height: 3.0,
                opening: Opening { x: 1.0, width: length - 2.0, sill: 0.9, height: 1.2 },
            }
        })
        .collect()
}

/// A 2 m x 1 m steel plate, 20 mm thick, with a 20 x 10 grid of
/// 22 mm bolt holes (200 holes).
#[derive(Clone, Debug)]
pub struct Plate {
    pub size: [f64; 2],
    pub thickness: f64,
    pub hole_radius: f64,
    pub holes: Vec<[f64; 2]>,
}

impl Plate {
    pub fn exact_volume(&self) -> f64 {
        self.size[0] * self.size[1] * self.thickness
            - self.holes.len() as f64 * PI * self.hole_radius.powi(2) * self.thickness
    }
}

pub fn plate_with_holes() -> Plate {
    let mut holes = Vec::new();
    for i in 0..20 {
        for j in 0..10 {
            holes.push([0.05 + i as f64 * 0.1, 0.05 + j as f64 * 0.1]);
        }
    }
    Plate { size: [2.0, 1.0], thickness: 0.02, hole_radius: 0.011, holes }
}

/// Segments used when a mesh kernel approximates a circle.
pub const CIRCLE_SEGMENTS: usize = 32;

/// The result of one test case on one kernel.
#[derive(Debug)]
pub struct Outcome {
    pub kernel: &'static str,
    pub case: &'static str,
    pub ok: bool,
    pub volume: f64,
    pub exact: f64,
    pub triangles: usize,
    pub elapsed: Duration,
    pub note: String,
}

impl Outcome {
    pub fn header() -> &'static str {
        "kernel\tcase\tok\tvolume\texact\trel_error\ttriangles\tms\tnote"
    }

    pub fn row(&self) -> String {
        let rel = if self.exact != 0.0 { (self.volume - self.exact) / self.exact } else { 0.0 };
        format!(
            "{}\t{}\t{}\t{:.9}\t{:.9}\t{:+.3e}\t{}\t{:.1}\t{}",
            self.kernel,
            self.case,
            self.ok,
            self.volume,
            self.exact,
            rel,
            self.triangles,
            self.elapsed.as_secs_f64() * 1000.0,
            self.note
        )
    }
}

/// Times `f`, which returns (volume, triangles, ok, note).
pub fn run(
    kernel: &'static str,
    case: &'static str,
    exact: f64,
    f: impl FnOnce() -> Result<(f64, usize, bool, String), String>,
) -> Outcome {
    let start = Instant::now();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    let elapsed = start.elapsed();
    match result {
        Ok(Ok((volume, triangles, ok, note))) => {
            Outcome { kernel, case, ok, volume, exact, triangles, elapsed, note }
        }
        Ok(Err(e)) => Outcome {
            kernel, case, ok: false, volume: 0.0, exact, triangles: 0, elapsed, note: format!("error: {e}"),
        },
        Err(_) => Outcome {
            kernel, case, ok: false, volume: 0.0, exact, triangles: 0, elapsed, note: "panicked".into(),
        },
    }
}

/// Prints the outcomes as a tab-separated table.
pub fn report(outcomes: &[Outcome]) {
    println!("{}", Outcome::header());
    for o in outcomes {
        println!("{}", o.row());
    }
}

// --- Civil sweep (SR-1): generated by our own code, not by a kernel. ---

/// A road-like path: a clothoid (curvature growing linearly with length)
/// from a straight start to radius `radius` over `length`, on a constant
/// `grade`, sampled every `step` metres.
pub fn clothoid_path(length: f64, radius: f64, grade: f64, step: f64) -> Vec<[f64; 3]> {
    let n = (length / step).ceil() as usize;
    let ds = length / n as f64;
    let (mut x, mut y, mut heading) = (0.0f64, 0.0f64, 0.0f64);
    let mut path = vec![[0.0, 0.0, 0.0]];
    for i in 0..n {
        // Midpoint rule on curvature k(s) = s / (radius * length).
        let s_mid = (i as f64 + 0.5) * ds;
        let k = s_mid / (radius * length);
        heading += k * ds;
        x += ds * heading.cos();
        y += ds * heading.sin();
        path.push([x, y, (i + 1) as f64 * ds * grade]);
    }
    path
}

/// Sweeps a `width` x `height` rectangle, centred on the path, along it.
/// Sections stay vertical (no banking), like a simple road corridor.
pub fn sweep_rectangle(path: &[[f64; 3]], width: f64, height: f64) -> Mesh {
    let n = path.len();
    let mut positions = Vec::with_capacity(n * 4);
    for i in 0..n {
        let a = path[i.saturating_sub(1)];
        let b = path[(i + 1).min(n - 1)];
        let (tx, ty) = (b[0] - a[0], b[1] - a[1]);
        let l = (tx * tx + ty * ty).sqrt();
        // Horizontal normal to the left of the direction of travel.
        let (nx, ny) = (-ty / l, tx / l);
        let p = path[i];
        let (hw, hh) = (width / 2.0, height / 2.0);
        positions.push([p[0] - nx * hw, p[1] - ny * hw, p[2] - hh]);
        positions.push([p[0] + nx * hw, p[1] + ny * hw, p[2] - hh]);
        positions.push([p[0] + nx * hw, p[1] + ny * hw, p[2] + hh]);
        positions.push([p[0] - nx * hw, p[1] - ny * hw, p[2] + hh]);
    }
    let mut triangles = Vec::new();
    for i in 0..(n as u32 - 1) {
        let (a, b) = (i * 4, (i + 1) * 4);
        for k in 0..4 {
            let k1 = (k + 1) % 4;
            triangles.push([a + k, a + k1, b + k1]);
            triangles.push([a + k, b + k1, b + k]);
        }
    }
    let last = (n as u32 - 1) * 4;
    triangles.extend([[0, 2, 1], [0, 3, 2], [last, last + 1, last + 2], [last, last + 2, last + 3]]);
    let mut m = Mesh { positions, triangles };
    if m.volume() < 0.0 {
        for t in &mut m.triangles {
            t.swap(1, 2);
        }
    }
    m
}

/// The corridor case: a 12 m x 1 m section swept along a 200 m clothoid
/// (radius 100 m at the end, 3% grade), sampled every 0.5 m. Returns the
/// mesh and its nominal volume (section area times path length).
pub fn corridor() -> (Mesh, f64) {
    let path = clothoid_path(200.0, 100.0, 0.03, 0.5);
    let length: f64 = path
        .windows(2)
        .map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2) + (w[1][2] - w[0][2]).powi(2)).sqrt())
        .sum();
    (sweep_rectangle(&path, 12.0, 1.0), 12.0 * 1.0 * length)
}

/// A 2 m x 2 m box culvert crossing the corridor near station 100.
pub fn culvert() -> Mesh {
    let path = clothoid_path(200.0, 100.0, 0.03, 0.5);
    let c = path[200];
    Mesh::cuboid([c[0] - 1.0, c[1] - 20.0, c[2] - 1.5], [c[0] + 1.0, c[1] + 20.0, c[2] + 0.2])
}

// --- Triangle-mesh helpers, for kernels that take raw meshes. ---

/// An indexed triangle mesh with f64 positions.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub positions: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    /// An axis-aligned box with outward-facing, counter-clockwise triangles.
    pub fn cuboid(min: [f64; 3], max: [f64; 3]) -> Mesh {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        let positions = vec![
            [x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0],
            [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1],
        ];
        let triangles = vec![
            [0, 2, 1], [0, 3, 2], // bottom (-z)
            [4, 5, 6], [4, 6, 7], // top (+z)
            [0, 1, 5], [0, 5, 4], // front (-y)
            [2, 3, 7], [2, 7, 6], // back (+y)
            [1, 2, 6], [1, 6, 5], // right (+x)
            [3, 0, 4], [3, 4, 7], // left (-x)
        ];
        Mesh { positions, triangles }
    }

    /// A vertical cylinder (z axis) approximated by `segments` sides.
    pub fn cylinder(center: [f64; 2], radius: f64, z0: f64, z1: f64, segments: usize) -> Mesh {
        let mut positions = Vec::with_capacity(segments * 2 + 2);
        for k in 0..segments {
            let a = 2.0 * PI * k as f64 / segments as f64;
            let (s, c) = a.sin_cos();
            positions.push([center[0] + radius * c, center[1] + radius * s, z0]);
        }
        for k in 0..segments {
            let a = 2.0 * PI * k as f64 / segments as f64;
            let (s, c) = a.sin_cos();
            positions.push([center[0] + radius * c, center[1] + radius * s, z1]);
        }
        let bottom = positions.len() as u32;
        positions.push([center[0], center[1], z0]);
        let top = positions.len() as u32;
        positions.push([center[0], center[1], z1]);
        let n = segments as u32;
        let mut triangles = Vec::new();
        for k in 0..n {
            let k1 = (k + 1) % n;
            triangles.push([bottom, k1, k]);
            triangles.push([top, n + k, n + k1]);
            triangles.push([k, k1, n + k1]);
            triangles.push([k, n + k1, n + k]);
        }
        Mesh { positions, triangles }
    }

    pub fn volume(&self) -> f64 {
        mesh_volume(&self.positions, &self.triangles)
    }
}

/// Signed volume of a closed triangle mesh (divergence theorem).
///
/// Positions are shifted to the first vertex first, so the result stays
/// accurate far from the origin.
pub fn mesh_volume(positions: &[[f64; 3]], triangles: &[[u32; 3]]) -> f64 {
    let Some(o) = positions.first().copied() else { return 0.0 };
    let p = |i: u32| {
        let v = positions[i as usize];
        [v[0] - o[0], v[1] - o[1], v[2] - o[2]]
    };
    triangles
        .iter()
        .map(|t| {
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0])
        })
        .sum::<f64>()
        / 6.0
}

/// True if every directed edge has exactly one opposite partner, which
/// means the mesh is closed and consistently oriented.
pub fn is_closed(triangles: &[[u32; 3]]) -> bool {
    use std::collections::HashMap;
    let mut edges: HashMap<(u32, u32), i32> = HashMap::new();
    for t in triangles {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *edges.entry((a, b)).or_default() += 1;
        }
    }
    edges.iter().all(|(&(a, b), &n)| n == 1 && edges.get(&(b, a)) == Some(&1))
}
