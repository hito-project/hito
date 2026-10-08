---
title: "E01: Research spikes"
labels: type:epic, area:research
milestone: "M0: Foundations"
---

Turn the open technical questions into recorded decisions before M1 design starts. Each spike is time-boxed and ends with an ADR (for a decision) or a design document (for input to a design), linked from the spike.

Context: [ADR 0005](docs/decisions/0005-rust-first-stack.md) (Rust-first dependencies) and [ADR 0009](docs/decisions/0009-license.md) (MIT/Apache-2.0, so no GPL dependencies).

## Story: Spike: geometry kernel selection
labels: type:spike, area:geometry

As a core developer, I want a chosen geometry kernel, so that elements can generate solids with openings reliably.

### Acceptance criteria
- [ ] Candidates evaluated, Rust-native ones first (for example truck, Fornjot, csgrs), plus C++ options (Manifold, OpenCascade) and what justifying them would take
- [ ] A prototype extrudes a wall profile and subtracts an opening with each finalist
- [ ] Evaluated on robustness, performance on 10k elements, license compatibility and maintenance activity
- [ ] ADR records the choice

## Story: Spike: IFC export in Rust
labels: type:spike, area:interop

As a core developer, I want to know how we'll write IFC4, so that IFC export can be designed early.

### Acceptance criteria
- [ ] Existing Rust IFC and STEP crates surveyed for completeness and license
- [ ] Compared against writing our own STEP writer, scoped to the M1 entities
- [ ] Validation approach defined (IfcOpenShell, buildingSMART validation service)
- [ ] ADR records the approach

## Story: Spike: UI toolkit evaluation
labels: type:spike, area:ui

As a UI developer, I want a confirmed UI toolkit, so that the application shell isn't rebuilt later.

### Acceptance criteria
- [ ] egui evaluated against alternatives (for example iced, Slint, Xilem)
- [ ] Criteria cover the needs from [ADR 0007](docs/decisions/0007-ui-familiar-to-current-users.md): ribbon-style toolbars, dockable panels, property grid, command line with autocomplete, embedded wgpu viewport, i18n with proper text shaping, accessibility
- [ ] A small prototype of the riskiest needs is built with the finalist
- [ ] ADR records the choice

## Story: Spike: storage engine and native file format
labels: type:spike, area:file-format

As a core developer, I want a chosen storage approach, so that the native format meets our principles: open, documented, versioned and fast.

### Acceptance criteria
- [ ] Options compared: SQLite (C, would need a justification, used by Bentley's iModel), Rust-native embedded databases, and a custom container format
- [ ] Evaluated for: open documentation, schema versioning and migration, saving to older versions, incremental saves, future worksharing (change sets), and file size
- [ ] ADR records the choice

## Story: Spike: study BIS/iModel and the IFC schema
labels: type:spike, area:core

As a core developer, I want to learn from the BIS and IFC data models, so that our element store and schema system avoid known mistakes.

### Acceptance criteria
- [ ] Design document summarising BisCore concepts (elements, models, aspects, relationships, schemas, change sets) and how IFC models the M1 elements
- [ ] Recommendations for HITO's element store and schema system
- [ ] Linked from [architecture/unified-core.md](docs/architecture/unified-core.md)

## Story: Spike: DWG and DXF feasibility
labels: type:spike, area:interop

As a project maintainer, I want to know how we can read and write DWG/DXF under MIT/Apache, so that the 2D drafting and interop roadmap is realistic.

### Acceptance criteria
- [ ] Options evaluated: LibreDWG (GPL, so can it be used out of process?), Open Design Alliance SDKs (proprietary, membership terms), Rust DXF crates, and a clean-room implementation
- [ ] Legal considerations summarised
- [ ] ADR records the strategy

## Story: Spike: RVT and RFA feasibility
labels: type:spike, area:interop

As a project maintainer, I want to know whether HITO can read Revit files, so that we have a realistic adoption strategy for Revit users.

### Acceptance criteria
- [ ] Surveyed: open-source RVT reverse-engineering efforts, ODA BimRv, and indirect routes (Revit's own IFC export, Speckle connectors, a Revit add-in exporter run under WinApps)
- [ ] Legal considerations of reverse-engineering for interoperability summarised
- [ ] ADR records the strategy and replaces [open-questions/rvt-interop-feasibility.md](docs/open-questions/rvt-interop-feasibility.md)
