# Tech stack

**Status: language accepted, libraries under review.** See [ADR 0005](../decisions/0005-rust-first-stack.md). Prefer Rust-native dependencies. Any non-Rust dependency needs a recorded justification.

| Layer | Choice | Rust-native | Notes |
|---|---|---|---|
| Language | Rust | ✅ | Accepted |
| Rendering | wgpu | ✅ | Vulkan, Metal and DX12 from one API. First-class on Linux. |
| UI | egui | ✅ | Custom-drawn, so we control the UX fully ([ADR 0007](../decisions/0007-ui-familiar-to-current-users.md)). Maturity to be reviewed. |
| Geometry | Manifold (mesh booleans) behind a multi-representation geometry port, with boolmesh (Rust) as a second adapter. Proposed in [ADR 0014](../decisions/0014-geometry-kernel.md). | ❌ (C++, justified in ADR 0014) | Parameters are the source of truth; the kernel evaluates them into meshes. Curves, sweeps, TINs and point clouds are separate modules. |
| IFC | Our own IFC4 exporter, unless a Rust library proves sufficient | ✅ | IFC STEP files are plain text, so export is tractable |
| DWG | To be decided | ? | LibreDWG is GPL, which conflicts with the [license](../decisions/0009-license.md) |
| Storage | SQLite through rusqlite (bundled) | ❌ (justified) | The `.hito` file is a SQLite database ([ADR 0013](../decisions/0013-storage-engine-and-file-format.md)). turso, a Rust rewrite with the same file format, is the path to pure Rust later. |

## Interaction model

These features shape the architecture and must exist from the start:

- A command system, where every action is a command with built-in undo.
- Conventions familiar to current users ([ADR 0007](../decisions/0007-ui-familiar-to-current-users.md)): Revit-style two-letter shortcuts and an AutoCAD-style command line.
- Discipline workspaces over the shared core.
- All user-facing text localised from day one ([ADR 0008](../decisions/0008-argentina-first-international-by-design.md)).
