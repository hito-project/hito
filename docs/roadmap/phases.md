# Phases

Capabilities are grouped by dependency. Per [ADR 0006](../decisions/0006-civil-engineering-suite-scope.md), every phase is in scope, and only the order is open for discussion. Phase numbers show dependencies, not a final sequence.

## Core

| Phase | Name | Capabilities |
|---|---|---|
| 0 | Core | Element store with stable IDs, parameters, categories; transactions and undo (worksharing-ready); operator system; workspaces; 64-bit coordinates with georeferencing; levels and grids; 3D viewport; save/load; IFC export |

## BIM (Revit parity)

The full Revit inventory is in [research/revit-capabilities.md](../research/revit-capabilities.md).

| Phase | Name | Capabilities |
|---|---|---|
| 1 | Architectural modelling | Walls (types, layers, joins); floors, roofs, ceilings; hosted doors and windows; stairs, railings; curtain walls; rooms, areas |
| 2 | Families | Family editor, reference planes, constraints, formulas, types and instances, nested families, shared parameters |
| 3 | Documentation | Plans, sections, elevations, callouts, drafting and 3D views; view templates; visibility and graphics; dimensions, tags, text; schedules; sheets, title blocks; PDF/DWG export |
| 4 | Structural modelling | Columns, beams, braces, foundations; rebar; analytical model, loads, boundary conditions |
| 5 | MEP | Ducts, pipes, cable trays, systems; electrical circuits and panels; sizing and flow |
| 6 | Project and collaboration | Phasing, design options, worksharing, linked models, revisions |
| 7 | Extended BIM | Massing, rendering, energy analysis, Python API, visual node scripting |

## Rest of the suite

These capability inventories are still pending ([suite-definition.md](../open-questions/suite-definition.md)).

| Phase | Name | Replaces (candidates) | Capabilities (preliminary) |
|---|---|---|---|
| 8 | 2D drafting | AutoCAD | Drawing primitives, blocks, layers, dimensions, DWG-native workflows |
| 9 | Civil and infrastructure | Civil 3D, InfraWorks, OpenRoads | Terrain (TIN) from survey data, alignments and profiles, corridors, grading and earthworks, pipe networks |
| 10 | Structural analysis | ETABS, SAP2000, SAFE, STAAD.Pro, Robot | Solver adapters (OpenSees, Code_Aster), design-code checks, export to commercial solvers |
| 11 | Detailing and fabrication | Tekla, Advance Steel | Steel connections, shop drawings, fabrication-level detail |
| 12 | Geotechnical and water | PLAXIS, HEC-RAS/HMS, EPANET, SWMM | Solver adapters (EPANET, SWMM), soil models, hydraulics |
| 13 | Coordination and reality capture | Navisworks, ReCap | Model federation, clash detection, point clouds |
| 14 | Interoperability | — | IFC import, DWG/DXF import and export, RVT/RFA import, LandXML, solver formats |

## Notes

- Phase 0 design decisions must account for **every later phase**.
- Interoperability (phase 14) is numbered last for dependency reasons only. It decides adoption ([principle 8](../vision/principles.md)), so parts of it will move earlier, depending on [RVT feasibility](../open-questions/rvt-interop-feasibility.md).
- The founding user's workflow decides which phases come first ([assumed workflow](../research/civil-engineer-workflows.md)).
- **Argentina context:** AutoCAD and CYPECAD dominate there, and cross-company BIM exchange is rare. So DWG interop, 2D drafting and CIRSOC design checks may matter more for the first users than RVT import ([ADR 0008](../decisions/0008-argentina-first-international-by-design.md)).
