---
title: "E11: IFC export"
labels: type:epic, area:interop
milestone: "M1: Structural BIM MVP"
---

Export the M1 model to IFC4 so that it works in other BIM tools and in public works that require open standards ([ADR 0002](docs/decisions/0002-native-model-with-early-ifc-export.md)). The implementation approach is chosen in E01.

## Story: Export the structural model to IFC4
labels: type:story, area:interop

As a user, I want to export my model to IFC4, so that consultants and authorities can use it in their software.

### Acceptance criteria
- [ ] Levels (IfcBuildingStorey), grids, columns, beams, slabs, walls and footings are exported with correct geometry and placement
- [ ] Materials and types export as IFC materials and type objects
- [ ] Element parameters export as property sets

## Story: Verified IFC quality
labels: type:story, area:interop

As a user, I want exported IFC files to be valid and lossless, so that I don't get the data loss other tools are known for ([pain point 1](docs/research/industry-pain-points.md)).

### Acceptance criteria
- [ ] Exported files pass schema validation (IfcOpenShell and/or the buildingSMART validation service) in CI
- [ ] Automated tests check that every exported element, property and material is present and correct
- [ ] Exported files open correctly in at least two independent IFC viewers (checked manually before release)
