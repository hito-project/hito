//! Spike #3: write the M1 structural elements to IFC with our own STEP writer.
//!
//!   ifc-export-spike [OUT_DIR]          sample model, IFC4 and IFC4X3, plus expected.json
//!   ifc-export-spike --bench N OUT_DIR  N x N bays, timing and size

mod guid;
mod ifc;
mod model;
mod step;

use ifc::{Exporter, Schema};
use model::*;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--bench") {
        let n: usize = args.get(1).and_then(|s| s.parse().ok()).expect("--bench N OUT_DIR");
        let out = args.get(2).cloned().unwrap_or_else(|| "out".into());
        return bench(n, &out);
    }
    let out = args.first().cloned().unwrap_or_else(|| "out".into());
    std::fs::create_dir_all(&out).unwrap();
    let m = sample();
    for (schema, grid, file) in [
        (Schema::Ifc4, false, "m1-ifc4"),
        (Schema::Ifc4x3, false, "m1-ifc4x3"),
        // C1 and C2 placed with IfcGridPlacement, to test reader support.
        (Schema::Ifc4, true, "m1-ifc4-gridplacement"),
        (Schema::Ifc4x3, true, "m1-ifc4x3-gridplacement"),
    ] {
        let (text, expected, n) = Exporter::export(&m, schema, grid);
        std::fs::write(format!("{out}/{file}.ifc"), &text).unwrap();
        std::fs::write(format!("{out}/{file}.expected.json"), ifc::expected_json(schema, &expected)).unwrap();
        println!("{file}.ifc\t{} entities\t{} bytes\t{} elements checked", n, text.len(), expected.len());
    }
}

/// A one-storey frame of N x N bays: columns at every grid intersection,
/// beams on every grid line, one slab with an opening per bay.
fn bench_model(n: usize) -> Model {
    let (sx, sy) = (6.0, 5.0);
    let axes = |count: usize, step: f64, tag: &dyn Fn(usize) -> String| {
        (0..=count).map(|i| GridAxis { tag: tag(i), offset: i as f64 * step }).collect::<Vec<_>>()
    };
    let grid = Grid {
        name: "Grilla".into(),
        u: axes(n, sx, &|i| format!("U{i}")),
        v: axes(n, sy, &|i| format!("{}", i + 1)),
        extent: (-1.0, -1.0, n as f64 * sx + 1.0, n as f64 * sy + 1.0),
    };
    let mut columns = vec![];
    let mut beams = vec![];
    let mut slabs = vec![];
    let idx = |i: usize, j: usize| i * (n + 1) + j;
    for i in 0..=n {
        for j in 0..=n {
            columns.push(Column {
                mark: format!("C{}", idx(i, j) + 1),
                ty: 0,
                base_level: 0,
                top_level: 1,
                at: ColumnAt::Point(i as f64 * sx, j as f64 * sy),
            });
        }
    }
    for i in 0..=n {
        for j in 0..=n {
            let (x, y) = (i as f64 * sx, j as f64 * sy);
            if i < n {
                beams.push(Beam {
                    mark: format!("V{}", beams.len() + 1),
                    ty: 1,
                    level: 1,
                    start: (x, y),
                    end: (x + sx, y),
                    frames_into: vec![idx(i, j), idx(i + 1, j)],
                });
            }
            if j < n {
                beams.push(Beam {
                    mark: format!("V{}", beams.len() + 1),
                    ty: 1,
                    level: 1,
                    start: (x, y),
                    end: (x, y + sy),
                    frames_into: vec![idx(i, j), idx(i, j + 1)],
                });
            }
            if i < n && j < n {
                slabs.push(Slab {
                    mark: format!("L{}", slabs.len() + 1),
                    level: 1,
                    thickness: 0.15,
                    outline: vec![(x, y), (x + sx, y), (x + sx, y + sy), (x, y + sy)],
                    material: 0,
                    openings: vec![Opening { x: x + 2.0, y: y + 2.0, w: 1.0, h: 1.0 }],
                });
            }
        }
    }
    Model {
        name: format!("Bench {n}x{n}"),
        georef: None,
        materials: vec![Material { name: "Hormigón H-30".into(), fck_mpa: 30.0 }],
        levels: vec![
            Level { name: "Planta baja".into(), elevation: 0.0 },
            Level { name: "Primer piso".into(), elevation: 3.0 },
        ],
        grid,
        frame_types: vec![
            FrameType { name: "C 30x30".into(), section: Section::Rect { b: 0.3, h: 0.3 }, material: 0 },
            FrameType { name: "V 20x50".into(), section: Section::Rect { b: 0.2, h: 0.5 }, material: 0 },
        ],
        columns,
        beams,
        slabs,
        walls: vec![],
        footings: vec![],
    }
}

fn bench(n: usize, out: &str) {
    std::fs::create_dir_all(out).unwrap();
    let m = bench_model(n);
    let elements = m.columns.len() + m.beams.len() + m.slabs.len();
    let t = Instant::now();
    let (text, _, entities) = Exporter::export(&m, Schema::Ifc4, false);
    let export = t.elapsed();
    let t = Instant::now();
    let path = format!("{out}/bench-{n}.ifc");
    std::fs::write(&path, &text).unwrap();
    let write = t.elapsed();
    println!(
        "bench {n}x{n}\t{elements} elements\t{entities} entities\t{:.1} MB\texport {:.3} s\twrite {:.3} s",
        text.len() as f64 / 1e6,
        export.as_secs_f64(),
        write.as_secs_f64()
    );
}
