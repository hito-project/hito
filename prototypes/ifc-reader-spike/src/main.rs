//! Spike #139: compare ifc-lite with our own Part 21 reader on real IFC files.
//!
//! ```text
//! ifc-reader-spike compare <file.ifc>      per-element results of both readers, as JSON lines
//! ifc-reader-spike bench-own <file.ifc>    our reader: index, decode everything, mesh every element
//! ifc-reader-spike bench-lite <file.ifc>   ifc-lite: its whole native pipeline (process_geometry)
//! ifc-reader-spike fuzz-own <file>         run our reader on one input (used by the fuzzer)
//! ifc-reader-spike fuzz-lite <file>        run ifc-lite on one input (used by the fuzzer)
//! ```

use std::collections::HashMap;
use std::time::Instant;

use ifc_reader_spike::geom::{Ifc, Mesh};
use ifc_reader_spike::p21::File;
use ifc_reader_spike::{products, run_lite, run_own, void_map};

fn peak_rss_mb() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<f64>().ok())
        })
        .map(|kb| kb / 1024.0)
        .unwrap_or(f64::NAN)
}

/// Primary geometry kind of an element, from the item types it uses.
fn category(kinds: &[String]) -> &'static str {
    let has = |p: &[&str]| kinds.iter().any(|k| p.iter().any(|x| k == x));
    if kinds.is_empty() {
        "none"
    } else if has(&[
        "IFCBOOLEANRESULT",
        "IFCBOOLEANCLIPPINGRESULT",
        "IFCCSGSOLID",
    ]) {
        "boolean"
    } else if has(&["IFCADVANCEDBREP", "IFCADVANCEDBREPWITHVOIDS"]) {
        "advanced-brep"
    } else if has(&["IFCFACETEDBREP", "IFCFACETEDBREPWITHVOIDS"]) {
        "faceted-brep"
    } else if has(&["IFCTRIANGULATEDFACESET", "IFCPOLYGONALFACESET"]) {
        "tessellated"
    } else if has(&["IFCSHELLBASEDSURFACEMODEL", "IFCFACEBASEDSURFACEMODEL"]) {
        "surface-model"
    } else if has(&[
        "IFCSWEPTDISKSOLID",
        "IFCREVOLVEDAREASOLID",
        "IFCSURFACECURVESWEPTAREASOLID",
        "IFCFIXEDREFERENCESWEPTAREASOLID",
        "IFCEXTRUDEDAREASOLIDTAPERED",
        "IFCREVOLVEDAREASOLIDTAPERED",
        "IFCSWEPTDISKSOLIDPOLYGONAL",
        "IFCDIRECTRIXCURVESWEPTAREASOLID",
        "IFCSECTIONEDSOLIDHORIZONTAL",
        "IFCSECTIONEDSPINE",
    ]) {
        "other-swept"
    } else if has(&["IFCEXTRUDEDAREASOLID"]) {
        "extrusion"
    } else {
        "other"
    }
}

fn summary(m: &Mesh) -> String {
    let bb = m
        .bbox()
        .map(|(lo, hi)| {
            format!(
                "[{:.4},{:.4},{:.4}]",
                hi[0] - lo[0],
                hi[1] - lo[1],
                hi[2] - lo[2]
            )
        })
        .unwrap_or("null".into());
    format!(
        "\"tris\":{},\"vol\":{:.6},\"ext\":{}",
        m.tri.len(),
        m.volume().abs(),
        bb
    )
}

fn lite_mesh(m: &ifc_lite_geometry::Mesh) -> Mesh {
    Mesh {
        pos: m
            .positions
            .chunks(3)
            .map(|c| [c[0] as f64, c[1] as f64, c[2] as f64])
            .collect(),
        tri: m
            .indices
            .chunks(3)
            .filter(|t| t.len() == 3)
            .map(|t| [t[0], t[1], t[2]])
            .collect(),
        closed: false,
    }
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn compare(path: &str) {
    let data = std::fs::read(path).expect("read");
    let t0 = Instant::now();
    let f = File::parse(&data).expect("parse");
    let ifc = Ifc::new(&f);
    let voids = void_map(&f);
    let elems = products(&f);
    let t_own_setup = t0.elapsed();

    // ifc-lite per-element, with its own unit handling and voids.
    let mut dec = ifc_lite_core::EntityDecoder::new(&data);
    // Georeferenced models: ifc-lite meshes are f32, so give it the relative-to-centre
    // offset its own pipeline would use (process_geometry does this internally).
    let scale = ifc.length_scale;
    let router = match ifc_lite_core::scan_model_bounds(&data).rtc_offset(scale) {
        Some(ifc_lite_core::RtcVerdict::Large { anchor }) => {
            ifc_lite_geometry::GeometryRouter::with_units_and_rtc(&data, &mut dec, anchor)
        }
        _ => ifc_lite_geometry::GeometryRouter::with_units(&data, &mut dec),
    };
    let lite_voids: rustc_hash::FxHashMap<u32, Vec<u32>> =
        voids.iter().map(|(k, v)| (*k, v.clone())).collect();

    println!(
        "{{\"file\":\"{}\",\"schema\":\"{}\",\"origin\":\"{}\",\"instances\":{},\"elements\":{},\"length_scale\":{}}}",
        esc(path),
        esc(&f.schema().unwrap_or_default()),
        esc(&f.originating_system().unwrap_or_default()),
        f.len(),
        elems.len(),
        ifc.length_scale
    );
    let _ = t_own_setup;
    for id in elems {
        let Ok(p) = f.get(id) else { continue };
        let mut kinds = Vec::new();
        ifc.item_kinds(&p, &mut kinds);
        let cat = category(&kinds);
        kinds.sort();
        kinds.dedup();
        let t = Instant::now();
        let own = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ifc.element(&p, voids.get(&id).map(|v| v.as_slice()).unwrap_or(&[]))
        }));
        let own_ms = t.elapsed().as_secs_f64() * 1e3;
        let own_json = match own {
            Ok(Ok(g)) if !g.mesh.is_empty() => format!(
                "{{\"ok\":true,{},\"open_ok\":{},\"open_fail\":{}}}",
                summary(&g.mesh),
                g.openings_applied,
                g.openings_failed
            ),
            Ok(Ok(_)) => "{\"ok\":false,\"err\":\"empty mesh\"}".into(),
            Ok(Err(e)) => format!("{{\"ok\":false,\"err\":\"{}\"}}", esc(&e.label())),
            Err(_) => "{\"ok\":false,\"err\":\"PANIC\"}".into(),
        };
        let t = Instant::now();
        let lite = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let e = dec.decode_by_id(id)?;
            router.process_element_with_voids(&e, &mut dec, &lite_voids)
        }));
        let lite_ms = t.elapsed().as_secs_f64() * 1e3;
        let lite_json = match lite {
            Ok(Ok(m)) if !m.indices.is_empty() => {
                format!("{{\"ok\":true,{}}}", summary(&lite_mesh(&m)))
            }
            Ok(Ok(_)) => "{\"ok\":false,\"err\":\"empty mesh\"}".into(),
            Ok(Err(e)) => format!(
                "{{\"ok\":false,\"err\":\"{}\"}}",
                esc(&e.to_string().chars().take(120).collect::<String>())
            ),
            Err(_) => "{\"ok\":false,\"err\":\"PANIC\"}".into(),
        };
        println!(
            "{{\"id\":{},\"type\":\"{}\",\"cat\":\"{}\",\"kinds\":[{}],\"own\":{},\"own_ms\":{:.3},\"lite\":{},\"lite_ms\":{:.3}}}",
            id,
            String::from_utf8_lossy(p.ty),
            cat,
            kinds
                .iter()
                .map(|k| format!("\"{k}\""))
                .collect::<Vec<_>>()
                .join(","),
            own_json,
            own_ms,
            lite_json,
            lite_ms
        );
    }
}

fn bench_own(path: &str) {
    let t0 = Instant::now();
    let data = std::fs::read(path).expect("read");
    let t_read = t0.elapsed();
    let t = Instant::now();
    let f = File::parse(&data).expect("parse");
    let t_index = t.elapsed();
    // Decode every instance once (what an importer that walks the whole model does).
    let t = Instant::now();
    let mut attrs = 0usize;
    let mut errors = 0usize;
    for (id, _) in f.iter() {
        match f.get(id) {
            Ok(i) => attrs += i.attrs.len(),
            Err(_) => errors += 1,
        }
    }
    let t_decode = t.elapsed();
    let rss_after_parse = peak_rss_mb();
    let t = Instant::now();
    let ifc = Ifc::new(&f);
    let voids = void_map(&f);
    let elems = products(&f);
    let (mut ok, mut tris) = (0usize, 0usize);
    for &id in &elems {
        let Ok(p) = f.get(id) else { continue };
        if let Ok(g) = ifc.element(&p, voids.get(&id).map(|v| v.as_slice()).unwrap_or(&[])) {
            if !g.mesh.is_empty() {
                ok += 1;
                tris += g.mesh.tri.len();
            }
        }
    }
    let t_geom = t.elapsed();
    println!(
        "{{\"reader\":\"own\",\"file\":\"{}\",\"mb\":{:.1},\"instances\":{},\"decode_errors\":{},\"attrs\":{},\"read_s\":{:.3},\"index_s\":{:.3},\"decode_all_s\":{:.3},\"rss_after_parse_mb\":{:.0},\"elements\":{},\"meshed\":{},\"triangles\":{},\"geometry_s\":{:.3},\"total_s\":{:.3},\"peak_rss_mb\":{:.0},\"threads\":1}}",
        esc(path),
        data.len() as f64 / 1e6,
        f.len(),
        errors,
        attrs,
        t_read.as_secs_f64(),
        t_index.as_secs_f64(),
        t_decode.as_secs_f64(),
        rss_after_parse,
        elems.len(),
        ok,
        tris,
        t_geom.as_secs_f64(),
        t0.elapsed().as_secs_f64(),
        peak_rss_mb()
    );
}

fn bench_lite(path: &str) {
    let t0 = Instant::now();
    let data = std::fs::read(path).expect("read");
    let t_read = t0.elapsed();
    let t = Instant::now();
    let index = ifc_lite_core::build_entity_index(&data);
    let t_index = t.elapsed();
    let n = index.len();
    drop(index);
    let rss_after_parse = peak_rss_mb();
    let t = Instant::now();
    let r = ifc_lite_processing::process_geometry(&data);
    let t_geom = t.elapsed();
    let tris: usize = r.meshes.iter().map(|m| m.indices.len() / 3).sum();
    let mut ids: Vec<u32> = r.meshes.iter().map(|m| m.express_id).collect();
    ids.sort_unstable();
    ids.dedup();
    println!(
        "{{\"reader\":\"ifc-lite\",\"file\":\"{}\",\"mb\":{:.1},\"instances\":{},\"read_s\":{:.3},\"index_s\":{:.3},\"rss_after_index_mb\":{:.0},\"meshes\":{},\"elements_meshed\":{},\"triangles\":{},\"pipeline_s\":{:.3},\"stats_parse_ms\":{},\"stats_geometry_ms\":{},\"total_s\":{:.3},\"peak_rss_mb\":{:.0},\"threads\":{}}}",
        esc(path),
        data.len() as f64 / 1e6,
        n,
        t_read.as_secs_f64(),
        t_index.as_secs_f64(),
        rss_after_parse,
        r.meshes.len(),
        ids.len(),
        tris,
        t_geom.as_secs_f64(),
        r.stats.parse_time_ms,
        r.stats.geometry_time_ms,
        t0.elapsed().as_secs_f64(),
        peak_rss_mb(),
        rayon_threads()
    );
}

fn rayon_threads() -> usize {
    std::env::var("RAYON_NUM_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("");
    let path = args.get(2).map(String::as_str).unwrap_or("");
    match cmd {
        "compare" => compare(path),
        "bench-own" => bench_own(path),
        "bench-lite" => bench_lite(path),
        "fuzz-own" => println!("{}", run_own(&std::fs::read(path).expect("read"))),
        "fuzz-lite" => println!("{}", run_lite(&std::fs::read(path).expect("read"))),
        "lite-pipeline" => {
            // Per-element results from ifc-lite's whole pipeline, to cross-check `compare`.
            let data = std::fs::read(path).expect("read");
            let r = ifc_lite_processing::process_geometry(&data);
            let mut by: HashMap<u32, Mesh> = HashMap::new();
            for m in &r.meshes {
                let e = by.entry(m.express_id).or_default();
                let base = e.pos.len() as u32;
                e.pos.extend(m.positions.chunks(3).map(|c| [c[0] as f64, c[1] as f64, c[2] as f64]));
                e.tri.extend(m.indices.chunks(3).filter(|t| t.len() == 3).map(|t| [t[0] + base, t[1] + base, t[2] + base]));
            }
            for (id, m) in by {
                println!("{{\"id\":{id},{}}}", summary(&m));
            }
        }
        _ => eprintln!(
            "usage: ifc-reader-spike compare|bench-own|bench-lite|fuzz-own|fuzz-lite <file>"
        ),
    }
}
