# IFC export spike (#3)

Throwaway prototype for [spike #3](https://github.com/hito-project/hito/issues/3). It writes the M1 structural elements to IFC4 and IFC4X3 with our own STEP Part 21 writer, with no dependencies, then checks the files with IfcOpenShell, an IDS file and the buildingSMART Validation Service's rules. The comparison with existing crates is in [Discussion #115](https://github.com/hito-project/hito/discussions/115). The decision is in `docs/decisions/0016-ifc-export.md`.

This code isn't part of HITO and won't be merged.

## What it writes

[`model.rs`](src/model.rs) stands in for the element store: parameters only, as [ADR 0014](../../docs/decisions/0014-geometry-kernel.md) requires. The sample is one bay of a two-level reinforced-concrete frame:

| Element | IFC | Notes |
|---|---|---|
| 2 levels | `IfcBuildingStorey` | Spanish names with accents, to test string encoding |
| Grid A–B / 1–2 | `IfcGrid`, `IfcGridAxis` | `FootPrint` representation |
| 3 square columns, 1 round | `IfcColumn` + `IfcColumnType` | Extruded profile shared with `IfcMaterialProfileSet`. C1 and C2 sit on grid intersections. |
| 2 beams | `IfcBeam` + `IfcBeamType` | `Axis` and `Body`. `IfcRelConnectsElements` to the columns they frame into. |
| 1 slab with an opening | `IfcSlab` + `IfcSlabType` | `IfcMaterialLayerSetUsage`, `IfcOpeningElement` + `IfcRelVoidsElement` |
| 1 wall with a door opening | `IfcWall` + `IfcWallType` | The opening starts at the wall's base (the coplanar case from spike #2) |
| 1 pad footing | `IfcFooting` | |
| Concrete H-30 | `IfcMaterial` + `Pset_MaterialConcrete` | f'c = 30 MPa |
| Georeference | `IfcMapConversion` + `IfcProjectedCRS` | POSGAR 2007 / Argentina 5 (EPSG:5347) |

Each element gets its `Pset_*Common` and `Qto_*BaseQuantities`, with `NetVolume` computed from the parameters.

| File | Lines | What |
|---|---|---|
| [`src/step.rs`](src/step.rs) | 293 (with tests) | Part 21 encoding: entity numbering, reals, string escapes, header |
| [`src/guid.rs`](src/guid.rs) | 77 | `IfcGloballyUniqueId` compression, checked against IfcOpenShell |
| [`src/ifc.rs`](src/ifc.rs) | 769 | The M1 mapping, IFC4 and IFC4X3 |
| [`validate/check.py`](validate/check.py) | 109 | Schema + where-rules, geometry and semantic checks |
| [`validate/m1.ids`](validate/m1.ids) | | IDS 1.0 requirements for M1 elements |
| [`validate/gherkin.py`](validate/gherkin.py) | 41 | Runs the buildingSMART normative rules locally |

## Running

```sh
nix shell nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc -c cargo test
nix shell nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc -c cargo run --release -- results
nix shell nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc -c cargo run --release -- --bench 100 /tmp/bench

# Validation tools (Python). On NixOS the wheels need libstdc++ on LD_LIBRARY_PATH.
python -m venv venv && venv/bin/pip install ifcopenshell ifctester pytest lark
export LD_LIBRARY_PATH=$(nix build nixpkgs#stdenv.cc.cc.lib --no-link --print-out-paths)/lib
venv/bin/python validate/check.py results/m1-ifc4.ifc results/m1-ifc4.expected.json
venv/bin/python -m ifctester validate/m1.ids results/m1-ifc4.ifc -r Console
venv/bin/python validate/schema_diff.py results/m1-ifc4.ifc

# buildingSMART normative rules: download github.com/buildingSMART/ifc-gherkin-rules
# with its ifc_validation_models submodule, then
venv/bin/pip install -r ifc-gherkin-rules/requirements.txt
venv/bin/python validate/gherkin.py ifc-gherkin-rules results/m1-ifc4.ifc
```

## Results

`results/` holds the files and the output of each check. Tools: IfcOpenShell 0.9.0, ifctester from PyPI, ifc-gherkin-rules `main` on 2026-10-08.

| Check | IFC4 | IFC4X3_ADD2 |
|---|---|---|
| IfcOpenShell schema + where-rules | ✅ 0 issues | ✅ 0 issues |
| Geometry (IfcOpenShell builds each element) | ✅ volumes and bounding boxes exact for planar solids | ✅ same |
| Semantics (GUID, class, storey, type, material, properties) | ✅ 9/9 | ✅ 9/9 |
| IDS (`m1.ids`) | ✅ 3/3 specifications | ✅ 3/3 |
| buildingSMART normative rules | ✅ 0 errors | ✅ 0 errors |

What the validators caught on the way (first runs kept in `results/*first-run*`):

- **IfcOpenShell where-rules, 4 errors:** `IfcOwnerHistory` with `ADDED` needs `LastModifiedDate`, and type objects take property sets through `HasPropertySets`, not `IfcRelDefinesByProperties`.
- **buildingSMART rules, 17 errors and 1 warning, on a file IfcOpenShell had passed:** an occurrence must leave `PredefinedType` empty when its type sets it (OJT001), `Qto_` sets need `MethodOfMeasurement = 'BaseQuantities'` (QTY001), and there was no georeference (GRF003).
- **IFC4X3 only, 1 error:** `IfcBuildingStorey.Elevation` is deprecated in 4.3 (IFC102).
- **`IfcGridPlacement`:** both schema validators and the buildingSMART rules accept it, but IfcOpenShell can't build the two grid-placed columns ("Unexpected topology"), and its placement utility doesn't support it. Grid positions are therefore exported as resolved `IfcLocalPlacement`s.
- **Round column:** the parameter volume (πr²h = 0.376991 m³) is exact. IfcOpenShell's mesh gives 0.375767 m³, 0.32% less, which is why `NetVolume` is written from parameters.

Only 2 of the 63 entity types used differ between IFC4 and IFC4X3_ADD2 (`schema_diff.txt`): `IfcGridPlacement` gains `PlacementRelTo`, and `IfcPropertySingleValue.Description` is renamed `Specification`.

### Speed and size (one storey frame, N x N bays, IFC4)

| Bays | Elements | Entities | File | Export |
|---|---|---|---|---|
| 20 x 20 | 1,681 | 43,886 | 2.6 MB | 0.02 s |
| 50 x 50 | 10,201 | 267,086 | 16.6 MB | 0.15 s |
| 100 x 100 | 40,401 | 1,059,086 | 66.8 MB | 0.63 s |
| 200 x 200 | 160,801 | 4,218,086 | 273.7 MB | 2.4 s |

Validating the 20 x 20 file takes 2.9 s (schema), 8.9 s (with where-rules) and 39 s (buildingSMART rules), so CI should validate small fixture models and leave big ones to a nightly job.
