# 0014. Geometry kernel: Manifold behind a multi-representation geometry port

- **Status:** Proposed
- **Date:** 2026-10-08

## Context

HITO needs solids with openings that are reliable. Spike [#2](https://github.com/hito-project/hito/issues/2) compared the candidate kernels and built the same prototypes with each. The evaluation, benchmarks and sources are in [Discussion #104](https://github.com/hito-project/hito/discussions/104). The prototype code is in [PR #103](https://github.com/hito-project/hito/pull/103).

The suite inventories add three constraints:

- **SR-1** ([#92](https://github.com/hito-project/hito/discussions/92)): most civil and drafting geometry isn't B-rep (TINs, clothoid alignments, sweeps along 3D curves, 2D primitives). The geometry port has to be multi-representation, and the kernel is one adapter behind it, not the whole port.
- **SR-15:** quantities (volumes and areas net of openings) must be exact.
- **Detailing** ([#87](https://github.com/hito-project/hito/discussions/87)): booleans have to stay robust with many small holes and cuts.

What the spike found:

| Kernel | Type | License | Robustness | Speed | Maintenance |
|---|---|---|---|---|---|
| **Manifold** (C++, via `manifold-csg`) | Mesh booleans | Apache-2.0 | Passed every case, including the coplanar door and 200 holes | Fastest: 200 holes in 0.1 s, 10,000 walls in 1.0 s | 22 contributors in the last year. It's the default boolean engine in Blender 4.5 and OpenSCAD. |
| **boolmesh** (Rust) | Mesh booleans, inspired by Manifold | MPL-2.0 | Passed every case | Same speed as Manifold on walls. 8–60 times slower on 200 holes (0.8 s batched, 6.1 s one by one). | One maintainer, first release November 2025 |
| **csgrs** (Rust) | Mesh booleans on exact arithmetic | MIT | Passed every case | Far too slow on many features: 50 holes took 5 minutes | One maintainer. The published 0.20.1 no longer builds (it depends on a yanked crate). |
| **truck** (Rust) | NURBS B-rep | Apache-2.0 | **Failed** the coplanar door and a plate with 10 holes | Slow: one hole took 0.4 s, 1,000 walls 4.0 s | Effectively one organisation. No release since 2024-09. |
| **monstertruck** (Rust, fork of truck) | NURBS B-rep | Apache-2.0 | **Failed** the same two cases | Faster than truck (1,000 walls in 2.3 s), same failures | Two people, started 2026 |
| **Fornjot** (Rust) | B-rep | 0BSD | Not tested | — | **Archived:** "No longer in development" |
| **OpenCascade** (C++, via `opencascade-rs`) | Exact B-rep | LGPL-2.1 with exception | Passed every case | 200 holes in 5.1 s, 1,000 walls in 4.1 s | OCCT itself is mature, but the Rust binding targets OCCT 7.8 and doesn't compile against 7.9. Building it means compiling OCCT from source. |

## Decision

### 1. The kernel is Manifold, as one adapter behind the geometry port

The **solid-boolean adapter** uses Manifold through the `manifold-csg` bindings.

- **Non-Rust justification (ADR 0005).** No Rust kernel is good enough yet:
  - The Rust B-rep kernels fail basic BIM booleans: a door flush with the wall's base, and a plate with 10 holes.
  - boolmesh passes, but it's younger, has one maintainer and is much slower on many features.
  - OpenCascade passes too, but it's 40–50 times slower than Manifold here, its LGPL license complicates static linking, and its Rust binding lags behind OCCT releases.

  Manifold is robust in production (Blender, OpenSCAD), fast, permissively licensed, and has a small, stable C API.
- **Path to replacing it.** boolmesh uses the same algorithm family and passes the same cases. It's kept as a second adapter and run in CI on the same inputs (differential testing). If it matures, it can replace Manifold without any change above the port.
- **License.** Apache-2.0, with dependencies under BSL-1.0 (Clipper2) and Apache-2.0 (oneTBB, optional). All compatible with [ADR 0009](0009-license.md).

### 2. The parameters are the truth; the kernel evaluates them

An element's geometry is stored as **parameters**: a profile, an extrusion, openings as voids, placement. It is never stored as a mesh. The kernel turns parameters into the meshes of the named representations ([element model](../architecture/element-model.md), section 6). Meshes are a cache, rebuilt when the parameters change.

As a result:

- **Exact quantities (SR-15) come from the parameters.** A slab's volume is its area times its thickness, net of its openings, computed analytically. Mesh volumes are a cross-check, and they're exact anyway for planar solids. Circular sections are where meshes fall short: a 32-segment circle has about 0.64% less area than the true circle. A circular column's volume must therefore come from πr²h, not from its mesh.
- **IFC export (#3) writes the parameters**, as `IfcExtrudedAreaSolid` with `IfcOpeningElement`, not as meshes.
- **Fabrication output later (E25)**, such as hole positions in DSTV files, also comes from the parameters, so a mesh kernel doesn't limit CNC accuracy.

### 3. What sits behind the geometry port

| Representation | Handled by | Status |
|---|---|---|
| Solid booleans: openings, cuts, unions, clash intersections | **Manifold** adapter (boolmesh as the second adapter) | M1 |
| Profiles and extrusions (rectangles, circles, steel sections) | Our own Rust code, producing meshes for the kernel | M1 |
| 2D regions and offsets (plan cut faces, hatches) | Manifold's `CrossSection` (Clipper2) for now; a pure-Rust 2D library to evaluate in E21 | M1 for plan views |
| Curves: lines, arcs, clothoid spirals, alignments | Our own Rust module, with exact analytic evaluation. No kernel handles clothoids. | E24 |
| Sweeps along 3D curves (corridors) | Our own code generates the swept mesh; the kernel does the booleans. The prototype does this for a 200 m clothoid. | E24 |
| TIN surfaces | A separate module (Delaunay triangulation), not the kernel | E24 |
| Point clouds | A separate out-of-core module ([#89](https://github.com/hito-project/hito/discussions/89)), not the kernel | E27 |
| Exact B-rep: NURBS, fillets, STEP exchange | Not needed for M1. If a later epic needs it (detailing STEP export, IFC import of advanced B-rep), add it as an optional adapter. OpenCascade would have to be dynamically linked or run out of process because of the LGPL. Re-check monstertruck's maturity first. | Later, by its own ADR |

## Consequences

- M1 has a kernel that handles the cases that broke the Rust B-rep kernels, and is fast enough. 10,000 walls with openings take about 1 second, before any caching or parallelism.
- HITO has one C++ dependency in its core. It builds from source with CMake, or links a system library (nixpkgs packages it).
- Geometry is mesh-based in M1. Curved shapes are faceted in the viewport, at a tolerance we choose. Quantities and exports aren't affected, because they come from the parameters.
- The geometry port has to be designed so that kernel adapters only see meshes and boolean programs. That's what keeps Manifold replaceable.
- [tech-stack.md](../architecture/tech-stack.md) records the choice.
