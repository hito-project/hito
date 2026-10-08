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
| [0010](0010-project-name-hito.md) | Project name: HITO (HITO Is Totally Open) | Accepted |
| [0011](0011-mvp-structural-bim.md) | MVP: structural BIM (M1), preceded by foundations (M0) | Accepted |
| [0012](0012-suite-definition.md) | Suite definition: which programs the suite replaces, and the rules for wrapping solvers | Accepted |
| [0013](0013-storage-engine-and-file-format.md) | Storage engine and native file format: SQLite, behind a storage port | Accepted |
| [0014](0014-geometry-kernel.md) | Geometry kernel: Manifold behind a multi-representation geometry port, boolmesh as a second adapter | Accepted |

Use [template.md](template.md) for new ADRs.
