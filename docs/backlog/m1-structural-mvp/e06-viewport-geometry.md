---
title: "E06: 3D viewport and geometry"
labels: type:epic, area:viewport, area:geometry
milestone: "M1: Structural BIM MVP"
---

Element geometry is generated from parameters through the geometry port (kernel chosen in E01). A wgpu viewport displays it and lets users navigate and select.

## Story: Generate element geometry from parameters
labels: type:story, area:geometry

As a domain developer, I want to describe an element's geometry as profiles, extrusions and boolean operations, so that geometry always follows the element's parameters.

### Acceptance criteria
- [ ] The geometry port supports profile extrusion, sweeps along straight paths, and boolean union and difference
- [ ] Geometry regenerates automatically when parameters or related elements change
- [ ] Geometry is cached and only regenerated for affected elements
- [ ] Geometry generation runs in parallel across elements ([principle 5](docs/vision/principles.md))

## Story: Navigate the 3D view
labels: type:story, area:viewport

As a Revit user, I want to orbit, pan and zoom with the mouse conventions I'm used to, so that navigation feels natural.

### Acceptance criteria
- [ ] Orbit, pan, zoom to cursor, and zoom to fit, using Revit-like mouse bindings by default
- [ ] A view cube or equivalent orientation control
- [ ] Perspective and orthographic modes

## Story: Select elements
labels: type:story, area:viewport

As a user, I want to select elements by clicking or drawing a box, so that I can inspect and edit them.

### Acceptance criteria
- [ ] Click to select, Ctrl or Shift to add and remove, Tab to cycle overlapping elements
- [ ] Window and crossing box selection, with the same semantics as Revit and AutoCAD
- [ ] Selection is highlighted, shared across views, and shown in the properties panel

## Story: Responsive with large models
labels: type:story, area:viewport

As a user, I want the viewport to stay smooth on real projects, so that I'm never waiting on the software ([pain point 5](docs/research/industry-pain-points.md)).

### Acceptance criteria
- [ ] A benchmark model (target size set in the story, at least 10k elements) renders at interactive frame rates on mid-range hardware
- [ ] Benchmarks run in CI to catch regressions
