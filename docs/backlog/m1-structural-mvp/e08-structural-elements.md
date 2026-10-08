---
title: "E08: Structural elements"
labels: type:epic, area:structural
milestone: "M1: Structural BIM MVP"
---

Reinforced-concrete structural elements with parametric types and materials. A full family editor comes in a later milestone. M1 ships built-in parametric types that users can duplicate and edit. Analysis, design checks and rebar are out of scope ([ADR 0011](docs/decisions/0011-mvp-structural-bim.md)).

## Story: Concrete materials
labels: type:story, area:structural

As an Argentine engineer, I want concrete materials named the way my codes name them, so that the model speaks my language.

### Acceptance criteria
- [ ] Material library with concrete grades as a regional pack ([ADR 0008](docs/decisions/0008-argentina-first-international-by-design.md)), starting with the CIRSOC 201 grades (H-20, H-25, H-30…)
- [ ] Materials carry their properties (strength, density, elastic modulus) and display colour
- [ ] Users can create custom materials

## Story: Place columns
labels: type:story, area:structural

As a user, I want to place rectangular or circular concrete columns between levels, so that I can model the vertical structure.

### Acceptance criteria
- [ ] Column types defined by section shape and dimensions, plus material
- [ ] Placed by clicking, snapping to grid intersections, from a base level to a top level, with offsets
- [ ] Columns follow level elevation changes

## Story: Place beams
labels: type:story, area:structural

As a user, I want to draw beams between columns or points, so that I can model the framing.

### Acceptance criteria
- [ ] Beam types defined by rectangular section and material
- [ ] Drawn on a level. Ends attach to columns and follow them when the columns move.
- [ ] Beam ends are trimmed or joined at columns cleanly in the geometry

## Story: Draw slabs
labels: type:story, area:structural

As a user, I want to draw floor slabs by boundary, so that I can model the floors.

### Acceptance criteria
- [ ] Slab types defined by thickness and material
- [ ] The boundary is drawn as lines or picked from beams and walls
- [ ] Openings can be added inside a slab boundary
- [ ] Slabs follow their level

## Story: Place structural walls
labels: type:story, area:structural

As a user, I want to draw concrete walls between levels, so that I can model shear walls and cores.

### Acceptance criteria
- [ ] Wall types defined by thickness and material
- [ ] Drawn as lines on a level, from a base level to a top level, with offsets
- [ ] Wall corners and T-junctions join cleanly

## Story: Place isolated footings
labels: type:story, area:structural

As a user, I want to place footings under columns, so that I can model foundations.

### Acceptance criteria
- [ ] Footing types defined by length, width, thickness and material
- [ ] Placed under a column (and follows it), or freely at a level
