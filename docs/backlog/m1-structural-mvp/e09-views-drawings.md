---
title: "E09: Views and drawings"
labels: type:epic, area:drawings
milestone: "M1: Structural BIM MVP"
---

Drawings are generated from the model, never drawn separately. This is Revit's main value over CAD ([research](docs/research/revit-capabilities.md)). M1 delivers plan views, a 3D view, associative dimensions and PDF export. Sheets, sections, schedules and annotation come in later milestones.

## Story: Generated plan views
labels: type:story, area:drawings

As a user, I want a plan view per level that is generated from the model, so that my plans always match the model.

### Acceptance criteria
- [ ] The plan cuts the model at a configurable height above the level. Cut elements are drawn with heavy lines and elements below with light lines.
- [ ] Line weights and patterns follow a drawing-standards pack, starting with IRAM ([ADR 0008](docs/decisions/0008-argentina-first-international-by-design.md))
- [ ] The plan updates immediately when the model changes
- [ ] Elements can be placed and edited directly in the plan view

## Story: Associative dimensions
labels: type:story, area:drawings

As a user, I want to dimension elements in plans, so that dimensions stay correct when the model changes.

### Acceptance criteria
- [ ] Aligned linear dimensions between element faces, centrelines and grids
- [ ] Dimension values update when the referenced elements move
- [ ] Dimension styles come from the standards pack

## Story: Export views to PDF
labels: type:story, area:drawings

As a user, I want to export a plan view to PDF at a chosen scale, so that I can share and print drawings.

### Acceptance criteria
- [ ] Export at a standard scale (1:50, 1:100…) on a chosen paper size
- [ ] Vector output with correct line weights
- [ ] Batch export of several views
