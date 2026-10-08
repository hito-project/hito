---
title: "E07: Levels and grids"
labels: type:epic, area:structural
milestone: "M1: Structural BIM MVP"
---

Datum elements: the horizontal levels and vertical grid lines that structural elements attach to. They are the first domain elements, and they prove the schema and relationship system from E03.

## Story: Create and edit levels
labels: type:story, area:structural

As a user, I want to create levels with names and elevations, so that I can organise the building by floors.

### Acceptance criteria
- [ ] Create, rename, delete and change the elevation of levels, from a view or the properties panel
- [ ] Changing a level's elevation moves every element attached to it (one undo step)
- [ ] Each level automatically gets a plan view (see E09)

## Story: Place structural grids
labels: type:story, area:structural

As a user, I want to place labelled grid lines, so that I can lay out columns on a structural grid.

### Acceptance criteria
- [ ] Straight grid lines with automatic labels (A, B, C… and 1, 2, 3…), editable
- [ ] Grids are visible in plan and 3D views, with label bubbles in plans
- [ ] Grid intersections are snap points for placing elements
