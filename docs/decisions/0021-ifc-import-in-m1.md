# 0021. M1 includes IFC import, so Revit users can switch

- **Status:** Accepted
- **Date:** 2026-10-15
- **Supersedes:** [0011](0011-mvp-structural-bim.md)

## Context

[ADR 0011](0011-mvp-structural-bim.md) made M1 a structural BIM modeller and left import out of it. The first working software HITO ships has to replace Revit for its founding user, and an M1 without import only works for projects started from scratch in HITO.

In practice, a structural engineer rarely starts from nothing. The structure is modelled on top of the architect's model, which will come from Revit for years, and ongoing projects live in Revit files. [ADR 0020](0020-rvt-rfa-through-ifc.md) settled that Revit projects come in through Revit's own IFC export, so this depends on IFC import (E22, [#66](https://github.com/hito-project/hito/issues/66)), which had no milestone.

## Decision

**M1 is a structural BIM modeller that a Revit user can switch to.** It contains everything in ADR 0011:

- Levels and structural grids
- Reinforced-concrete columns, beams, slabs, walls and isolated footings, with parametric types and materials
- Elements that relate to each other: beams frame into columns, and elements follow level changes
- Generated plan views per level and a 3D view, with associative dimensions and PDF export
- Native save/load with a versioned, documented format
- IFC4 export
- Spanish and English UI, metric units

and adds **IFC import**:

- **Linked IFC models as read-only references** ([#140](https://github.com/hito-project/hito/issues/140)): an architect's model is shown, cut in plan views and snapped to, but not converted. This is how structural engineers already work with the architecture model in Revit.
- **Importing IFC as native elements** ([#141](https://github.com/hito-project/hito/issues/141)), for the element types M1 has. Anything else comes in as reference geometry and is listed in an import report.
- **Revit's IFC exports** ([#135](https://github.com/hito-project/hito/issues/135)) and the **migration guide with a Revit export setup** ([#136](https://github.com/hito-project/hito/issues/136)), as decided in ADR 0020.
- A **spike on the IFC reader** ([#139](https://github.com/hito-project/hito/issues/139)) comes first: ifc-lite or our own Part 21 reader next to the writer.

## Out of scope for M1 (later milestones)

Analysis, CIRSOC design checks, rebar, sheets and schedules, families editor, architecture, MEP, worksharing, DWG, a direct RVT reader, and Revit families beyond fixed geometry per type ([#134](https://github.com/hito-project/hito/issues/134)).

## Consequences

- A Revit user can bring an existing project, or an architect's model, into HITO, and M1 can be used on real work instead of only new projects.
- M1 grows by an epic, and DWG work ([ADR 0019](0019-dwg-and-dxf.md)) waits until M1 ships.
- Import maps IFC onto native element types, so it comes after the element store, levels, grids and structural elements. Linked references only need the viewport, so they can ship earlier.
- The reader choice may need an ADR to allow MPL-2.0 in `deny.toml`, as for the DWG codec.
- The STEP Part 21 crate ([#116](https://github.com/hito-project/hito/issues/116)) may grow a reader, depending on the spike.
- Export and import are tested against each other: a HITO model exported to IFC and imported again must come back unchanged.
- Rebar and CIRSOC checks stay the natural milestone after M1.
