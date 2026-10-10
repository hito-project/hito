# 0016. IFC export: our own STEP writer, validated in CI

- **Status:** Proposed
- **Date:** 2026-10-10

## Context

M1 exports IFC4 ([ADR 0011](0011-mvp-structural-bim.md)), and IFC export comes early ([ADR 0002](0002-native-model-with-early-ifc-export.md)). Spike [#3](https://github.com/hito-project/hito/issues/3) surveyed the Rust IFC and STEP crates and built a prototype writer for the M1 elements. The survey, results and sources are in the IFC export evaluation Research discussion. The prototype is in the spike's draft PR.

What the survey found, checked on 2026-10-08:

| Crate | License | Writes IFC? | Status |
|---|---|---|---|
| **ifc_rs** | MIT | Yes, a hand-written IFC4 subset | Walls, slabs, openings, materials, storeys. No columns, beams, footings, grids or property sets. Alpha, last release 2024-12, no commits in the last year. Depends on bevy_math. |
| **ifc-lite** (`ifc-lite-*`) | MPL-2.0 | Re-writes files it has parsed, with edits | A fast reader and geometry engine. Authoring from scratch lives in its TypeScript packages, not in Rust. 93 releases since January 2026 (now version 20), mostly one author. |
| **openbimrs** (`openbim-ifc`, `ifc-step`, `ifc-author`, ...) | **AGPL-3.0** | Yes, schema-checked authoring for IFC2x3 to IFC4X3 | The most complete Rust design, but AGPL is incompatible with [ADR 0009](0009-license.md). Started 2026-08, one author. |
| **ruststep** / espr | Apache-2.0 | No output specification | "Do not use for product." Its EXPRESS compiler can't read the IFC schemas. No commits since 2025-03. |
| step-p21, step-io, iso-10303, oxideav-ifc, bimifc | various | No IFC writing | Part 21 parsers, CAD-only STEP (AP242), or IFC readers. bimifc has no license file. |

No crate is a usable, permissively licensed IFC writer for M1.

The prototype writer, with no dependencies:

- **Size.** About 300 lines for the Part 21 encoding (entity numbering, reals that round-trip exactly, string escapes for accents, the header) and 80 for GUIDs. The M1 mapping is about 770 lines.
- **Coverage.** It writes levels, a grid, square and round columns, beams, a slab with an opening, a wall with a door, a footing, a concrete material with f'c, types, property sets, base quantities and a georeference (POSGAR 2007), in IFC4 and IFC4X3_ADD2.
- **Correctness.** Both files pass IfcOpenShell's schema and where-rule validation, an IDS file and the buildingSMART Validation Service's normative rules (run locally). IfcOpenShell rebuilds every element's geometry with exact volumes and positions.
- **Speed.** 40,000 elements (67 MB) in 0.6 s.
- **IFC4 versus 4.3.** Only 2 of the 63 entity types used differ in their attributes, and one attribute is deprecated in 4.3.

The validators caught real mistakes: 4 from IfcOpenShell's where-rules, then 17 more from the buildingSMART rules on a file IfcOpenShell had passed. Readers also differ from the schema: `IfcGridPlacement` passes every validator, but IfcOpenShell can't place the elements that use it.

## Decision

### 1. Our own writer, in two layers

- **A Part 21 writer crate** that knows the encoding and nothing about IFC: values, entity numbering, streaming output, the header. Pure Rust with no dependencies.
- **An IFC export adapter** behind the exporter port ([ADR 0004](0004-unified-core-ports-and-adapters.md)) that maps HITO elements to IFC entities, following the mapping in the [element model](../architecture/element-model.md). The entity set is the M1 subset, with typed builder functions per entity. A code generator from the EXPRESS schemas is deferred until the entity count makes hand-writing a burden, which is likely with IFC import (E22) or IFC 4.3 infrastructure (E24).

### 2. IFC4 ADD2 TC1 for M1, with 4.3 as a second target

M1 writes **IFC4** (Reference View), the version that most receiving software reads. The adapter takes the schema version as a parameter. **IFC4X3_ADD2** stays a tested second output, because civil work (E24) will need it.

### 3. Export rules from the spike

- **Geometry comes from parameters**, as [ADR 0014](0014-geometry-kernel.md) requires: `IfcExtrudedAreaSolid` with `IfcOpeningElement` and `IfcRelVoidsElement`, not meshes. Quantities (`Qto_*BaseQuantities`) are computed from parameters too. A round column's mesh is 0.3% short of πr²h.
- **Placements are always resolved `IfcLocalPlacement`s.** Grid-hosted elements aren't written with `IfcGridPlacement`, because readers don't support it. This is the cartesian fallback in the element model, section 7.
- **GUIDs come from element UUIDs**, so the same element keeps its `GlobalId` across exports.
- **Type values live on the type.** Type property sets go in `HasPropertySets`. An occurrence leaves `PredefinedType` empty when its type sets it.

### 4. Validation in CI, in four layers

Each pull request that touches export runs these on a set of small fixture models:

1. **Schema:** IfcOpenShell's validator with EXPRESS where-rules.
2. **Normative rules:** the buildingSMART Validation Service's own rules (`ifc-gherkin-rules`, MIT), run locally at a pinned commit.
3. **Round trip:** IfcOpenShell reopens the file, finds every element, property, material and type again, and rebuilds the geometry. Volumes and bounding boxes must match the parameters.
4. **Requirements:** an IDS file states what HITO promises about an export, checked with ifctester.

These are Python tools used only in CI. They aren't shipped or linked, so they don't affect the license or the Rust-first rule. Large models are validated nightly, not per pull request. Before each release, files are uploaded to the online Validation Service and opened in at least two independent viewers (#52).

## Consequences

- HITO has no IFC dependency and controls the export completely. The Part 21 crate can later serve IFC import (E22) and other STEP exchanges, such as detailing STEP export (E25).
- We own an IFC mapping layer, as ADR 0002 expected. The validators keep it honest. The spike showed that each one catches mistakes the others miss.
- CI needs Python with IfcOpenShell (LGPL, run as a tool), ifctester and the gherkin rules. The buildingSMART rules take about 40 s on a 1,700-element model, so per-PR fixtures stay small.
- `IfcGridPlacement` is out until readers support it. The grid relationship can be added later with `IfcRelPositions` in IFC 4.3.
- openbimrs is worth checking again if its license changes. ifc-lite is a candidate for IFC import (E22), as a reader.
- New work: the Part 21 writer crate, the IFC export adapter (#51), the CI validation pipeline (#52), and an IDS file for M1.
