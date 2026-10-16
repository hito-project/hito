//! Reading a large real model: the performance criterion of #148.
//!
//! The model isn't committed (spike #139's corpus has no stated licence), so
//! this test is ignored unless asked for. The spike's 343 MB ArchiCAD model,
//! `O-S1-BWK-BIM architectural - BIM bouwkundig.ifc`, is listed with its
//! download URL and SHA-256 in `corpus/corpus.tsv` at tag `spike/139-ifc-reader`.
//!
//! ```sh
//! HITO_STEP_LARGE_FILE=path/to/model.ifc \
//!     cargo test --release -p hito-step --test large_file -- --ignored --nocapture
//! ```

use std::time::Instant;

#[test]
#[ignore = "needs a large model in HITO_STEP_LARGE_FILE; run with --release"]
fn reads_a_large_model_quickly_in_little_memory() {
    let path = std::env::var_os("HITO_STEP_LARGE_FILE")
        .expect("set HITO_STEP_LARGE_FILE to the path of a large Part 21 file");

    let start = Instant::now();
    let data = std::fs::read(&path).unwrap();
    let read = start.elapsed();
    let r = hito_step::StepReader::new(&data).unwrap();
    let indexed = start.elapsed();
    let mut attributes = 0;
    for id in r.ids() {
        attributes += r.get(id).unwrap().attributes.len();
    }
    let total = start.elapsed();

    let mb = data.len() as f64 / 1e6;
    println!(
        "{mb:.1} MB, {} instances of {} types, {attributes} attributes",
        r.len(),
        r.type_names().len()
    );
    println!(
        "read {:.2} s, index {:.2} s, decode all {:.2} s, total {:.2} s",
        read.as_secs_f64(),
        (indexed - read).as_secs_f64(),
        (total - indexed).as_secs_f64(),
        total.as_secs_f64()
    );
    // "A few seconds", with room for a slower machine than the spike's.
    if !cfg!(debug_assertions) {
        assert!(total.as_secs_f64() < 15.0, "took {total:?}");
    }
    // The file itself, plus an index well under its size, plus what any test
    // process takes, which only matters for small files.
    if let Some(peak) = peak_memory_bytes() {
        let ratio = peak as f64 / data.len() as f64;
        println!(
            "peak memory {:.0} MB, {ratio:.2} × the file",
            peak as f64 / 1e6
        );
        let allowed = 1.6 * data.len() as f64 + 32e6;
        assert!(
            (peak as f64) < allowed,
            "peak memory {ratio:.2} × the file size"
        );
    }
}

/// The process's peak resident memory, where the platform says (Linux).
fn peak_memory_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}
