# IFC reader spike (#139)

Throwaway prototype for [spike #139](https://github.com/hito-project/hito/issues/139). It reads the same IFC files with two candidate readers and with IfcOpenShell as the reference, and compares what each can turn into a mesh, how fast, with how much memory, and how they behave under fuzzing. The results are in the Discussion linked from the issue. The decision is in `docs/decisions/0022-ifc-reader.md`.

This code isn't part of HITO and won't be merged.

## The candidates

| | Code | What it is |
|---|---|---|
| **A: ifc-lite** | `ifc-lite-core`, `-geometry`, `-processing` 20.4.0 (MPL-2.0) | Its scanner and decoder, and its geometry router per element (`compare`) or its whole native pipeline (`bench-lite`) |
| **B: our own** | [`src/p21.rs`](src/p21.rs) | A STEP Part 21 reader with no dependencies: one pass to index every instance (16 bytes each), then decoding on demand. Bounded nesting, no panics on bad input. |
| | [`src/geom.rs`](src/geom.rs) | The IFC side HITO would own: units, placements, profiles (parametric, arbitrary, derived, composite, steel fillets), extrusions, faceted B-reps, surface models, tessellated sets, mapped items, swept disks, revolutions, CSG primitives. Booleans, half-spaces and openings go to Manifold, standing in for the geometry port (ADR 0014). Polygons are triangulated with earcutr (ISC). |

Not implemented in B: advanced B-rep (NURBS), sectioned and tapered solids, surface-curve sweeps, IFC 4.3 linear placement, curves other than lines, polylines, circles and indexed polycurves.

## Files

| File | What |
|---|---|
| `src/main.rs` | `compare`, `bench-own`, `bench-lite` |
| `src/lib.rs` | Element selection, the void map, and the end-to-end runs used by the fuzzer |
| `reference/ios.py` | IfcOpenShell 0.9: per-element meshes for a `compare` run, and the benchmark |
| `reference/run_corpus.py`, `reference/aggregate.py` | Run all three on the corpus and print the tables |
| `fuzz/` | cargo-fuzz targets `own` and `lite`, each end to end (scan, decode every instance, mesh every element) |
| `corpus/corpus.tsv` | The corpus: path, SHA-256, schema, view, originating system, source, licence, download URL |
| `results/` | Raw outputs of the runs reported in the Discussion |

The IFC files aren't committed: most have no stated licence. `corpus/corpus.tsv` lists where to download each one and its hash.

## Running

```sh
# Manifold from nixpkgs (see prototypes/geometry-spike in spike/2-geometry)
export MANIFOLD_CSG_LIB_DIR=$(nix build nixpkgs#manifold --no-link --print-out-paths)/lib
export LD_LIBRARY_PATH=$MANIFOLD_CSG_LIB_DIR
nix shell nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc -c cargo test --release
nix shell nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc -c cargo build --release

# Reference (on NixOS the wheels need libstdc++ on LD_LIBRARY_PATH)
python -m venv venv && venv/bin/pip install ifcopenshell numpy
venv/bin/python reference/run_corpus.py <corpus dir> out target/release/ifc-reader-spike 80
venv/bin/python reference/aggregate.py out

# Large models
/usr/bin/time -v target/release/ifc-reader-spike bench-own big.ifc
/usr/bin/time -v target/release/ifc-reader-spike bench-lite big.ifc
/usr/bin/time -v venv/bin/python reference/ios.py bench big.ifc 6

# Fuzzing (nightly)
cargo +nightly fuzz build -s none -O
fuzz/target/x86_64-unknown-linux-gnu/release/own corpus-dir -fork=2 -ignore_crashes=1 \
  -ignore_timeouts=1 -ignore_ooms=1 -timeout=10 -rss_limit_mb=2048 -malloc_limit_mb=2048 \
  -max_len=262144 -max_total_time=7200
```
