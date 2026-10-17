# Architecture Decision Records

Each ADR records one decision, its context and its consequences. ADRs are never edited after acceptance. To change a decision, write a new ADR that supersedes the old one.

**Statuses:** Proposed → Accepted → (Superseded by NNNN)

| # | Decision | Status |
|---|---|---|
| [0001](0001-standalone-desktop-app.md) | Standalone desktop app with a Blender-like UI | Superseded by 0007 |
| [0002](0002-native-model-with-early-ifc-export.md) | Own native model, IFC export early | Accepted |
| [0003](0003-full-revit-parity.md) | Full Revit parity is the minimum scope | Superseded by 0006 |
| [0004](0004-unified-core-ports-and-adapters.md) | Unified core with domains and ports/adapters | Accepted |
| [0005](0005-rust-first-stack.md) | Rust-first stack with Rust-native dependencies | Accepted |
| [0006](0006-civil-engineering-suite-scope.md) | Scope: a complete 2D/3D civil engineering suite | Accepted |
| [0007](0007-ui-familiar-to-current-users.md) | Standalone desktop app with a UI familiar to current users. Other UX patterns (Blender included) only via ADR. | Accepted |
| [0008](0008-argentina-first-international-by-design.md) | Argentina first, international by design | Accepted |
| [0009](0009-license.md) | License: MIT OR Apache-2.0 | Accepted |
| [0010](0010-project-name-hito.md) | Project name: HITO (HITO Is Totally Open) | Superseded by 0018 |
| [0011](0011-mvp-structural-bim.md) | MVP: structural BIM (M1), preceded by foundations (M0) | Superseded by 0021 |
| [0012](0012-suite-definition.md) | Suite definition: which programs the suite replaces, and the rules for wrapping solvers | Accepted |
| [0013](0013-storage-engine-and-file-format.md) | Storage engine and native file format: SQLite, behind a storage port | Accepted |
| [0014](0014-geometry-kernel.md) | Geometry kernel: Manifold behind a multi-representation geometry port, boolmesh as a second adapter | Accepted |
| [0015](0015-ui-toolkit.md) | UI toolkit: egui on wgpu, behind HITO's own shell layer; Fluent for i18n; RTL languages wait for bidi support | Accepted |
| [0016](0016-ifc-export.md) | IFC export: our own STEP Part 21 writer, IFC4 for M1, validated in CI with IfcOpenShell and the buildingSMART rules | Accepted |
| [0017](0017-development-builds.md) | Development builds: an AppImage for Linux, zips for Windows and macOS, published as a rolling `dev` prerelease on every push to main | Accepted |
| [0018](0018-hito-as-working-name.md) | HITO is a working name until the first public release; trademark conflicts in Argentina, the US and the EU | Accepted |
| [0019](0019-dwg-and-dxf.md) | DWG and DXF: our own pure-Rust codec for the AutoCAD drawing database, DXF first, then DWG read and write; built from published specifications and permissive code only; no LibreDWG or ODA dependency | Accepted |
| [0020](0020-rvt-rfa-through-ifc.md) | RVT and RFA: through Revit's IFC export, with a Revit add-in later for families; no direct RVT reader for now | Accepted |
| [0021](0021-ifc-import-in-m1.md) | M1 includes IFC import: linked read-only references, native import of M1 elements, and Revit's IFC exports, so Revit users can switch | Accepted |
| [0022](0022-ifc-reader.md) | IFC reader: our own STEP Part 21 reader next to the writer, IFC geometry in HITO through the geometry port; ifc-lite as a CI reference, not a dependency | Accepted |
| [0023](0023-mpl-dependencies.md) | MPL-2.0 dependencies are allowed, used unmodified; bundled data files' licenses are checked too | Accepted |
| [0024](0024-model-history.md) | Model history: a hash-chained log of change sets in the .hito file, undo that survives reopening, tiered compression, branches as layers that also serve as Design Options, merge first, full history kept | Accepted |

Use [template.md](template.md) for new ADRs.
