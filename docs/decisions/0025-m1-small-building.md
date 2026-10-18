# 0025. M1 models a small building the way the founding user does: architecture, basic concrete structure and drawings

- **Status:** Accepted
- **Date:** 2026-10-17
- **Supersedes:** [0021](0021-ifc-import-in-m1.md)

## Context

[ADR 0011](0011-mvp-structural-bim.md) made M1 a structural BIM modeller, assuming structural modelling is the most common Revit use. [ADR 0021](0021-ifc-import-in-m1.md) kept that and added IFC import. Neither was checked against the founding user.

On 2026-10-17 she went through 396 Revit features ([Discussion #191](https://github.com/hito-project/hito/discussions/191)):

- **Her top three:** architecture, walls and columns.
- **What she uses a lot:**
  - walls, doors, windows, stairs and railings
  - structural columns, structural floors and isolated footings
  - levels, plans, sections, 3D and the section box
  - dimensions and text
  - sheets with title blocks she designs herself, printed or exported to PDF
- **What she never uses:** MEP, clash detection, worksharing, rooms, schedules, rebar or analysis.
- **AutoCAD is part of her work.** She links DWGs often and does her 2D drawing in AutoCAD.
- **Her complaints:** too much information on screen all the time, no orbiting with the mouse wheel, and walls that don't join on their own. She'd also like a nicer look, and her computer is too weak for heavy work.

The structural half of M1 fits her. The architectural half, which ADR 0011 put out of scope, is what she uses most.

## Decision

**M1 is a modeller for a small building, the way the founding user models it today:** architecture, basic reinforced-concrete structure, and the drawings to deliver it. The milestone is renamed **M1: Building BIM MVP**.

### Kept from ADR 0021

- Levels and grids
- Reinforced-concrete columns, beams, slabs and isolated footings, with parametric types and materials
- Elements that follow each other: beams frame into columns, and elements follow level changes
- Generated plan views per level, a 3D view, associative dimensions and PDF export
- Native save and open with a versioned, documented format
- IFC4 export and IFC import (linked references and native elements)
- Spanish and English UI, metric units

### Added

- **Architectural walls.** They join automatically at corners and T-junctions, their top and base attach to slabs and roofs, and they can have rectangular openings. Structural walls (#40) become a wall with its structural flag set, as in Revit.
- **Doors and windows** hosted in walls. They cut their opening, and can be flipped. Types have fixed geometry per type, loaded from a library, as ADR 0020 already plans for Revit families.
- **Sections and elevations**, plus a section box in 3D.
- **Sheets with a title block**, and PDF export of sheets. Title blocks are edited as a sheet template, not in a family editor.
- **Linking a DWG as a read-only underlay** in plan views. It uses the read path of ADR 0019's codec. Writing DWG still waits.

### Interface rules for M1

These apply to every M1 story. Departures from Revit's look go through ADR 0007, and these are recorded here.

- **A calm interface.** By default it shows only what the current task needs. The properties panel shows common parameters first and the rest on demand, and panels open where Revit puts them but start collapsed when unused.
- **Theming:** light and dark, plus an accent colour.
- **Orbiting with the mouse wheel** in 3D. Holding the wheel orbits, and Shift with the wheel pans. That's the reverse of Revit's default, so it's a setting with Revit's behaviour one click away.
- **Modest hardware:** M1's speed budgets (#174) are measured on a low-end laptop with integrated graphics, not on a workstation.

### M1-alpha, the first version she tries

Levels, grids, walls (auto-joined and attached), doors and windows, columns, beams, structural slabs and isolated footings. A 3D view with mouse-wheel orbit, a plan view per level, a section with a section box, selection, the properties panel, undo and redo, and save and open. Dimensions, sheets, PDF, DWG linking and IFC follow in M1 after the alpha.

### Out of scope for M1 (later milestones)

- Stairs, railings, ramps and roofs. She uses them, but each is a large element, so they're the first candidates for M2.
- MEP and coordination (E19, E27), worksharing, rooms and areas, schedules, rebar, analysis, rendering, a family editor, writing DWG, and a direct RVT reader.

## Consequences

- M1 matches what its first user does, so her feedback on the alpha is about real work.
- M1 grows: walls and openings, doors and windows, sections and elevations, sheets and DWG linking. The structural element stories stay, so nothing already built is lost.
- The CIRSOC concrete table (#36) stays, updated to the 2025 editions and marked "not reviewed" until an engineer reviews it.
- Hosted elements and openings (doors and windows cutting walls) become a core need in M1. The element model's relationships and the geometry cache's dependency tracking already cover it, but it needs its own stories.
- DWG reading moves earlier than ADR 0021 planned. The codec (ADR 0019) becomes an M1 lane, read-only.
- Stairs, railings and roofs are named as M2 candidates, so M2 isn't automatically "rebar and CIRSOC checks" as ADR 0011 said.
- The interface rules make some Revit habits settings rather than defaults, for example the mouse-wheel orbit. ADR 0007 still holds: everything else stays as familiar as Revit.
