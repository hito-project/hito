# Revit capabilities

## What makes Revit BIM rather than 3D CAD

The founding user describes Revit as "CAD in 3D with metadata embedded, like cables and pipes". That is accurate, but it leaves out the two properties that matter most for our architecture:

1. **Elements are related, not just annotated.** Moving a wall moves the door hosted in it. Changing a level's height updates every wall attached to it. Pipes connect to fittings, and fittings connect to equipment. The core must track relationships and propagate changes, so elements cannot be stored as independent objects.
2. **Drawings are generated, not drawn.** In CAD, the plan and the section are separate drawings kept consistent by hand. In Revit, both are views of one model, and editing the model updates every drawing. This is Revit's main value over CAD.

## Capability inventory

This is the full set of capabilities that Revit parity requires. Revit is one part of the suite's scope ([ADR 0006](../decisions/0006-civil-engineering-suite-scope.md)). Their build order is in [roadmap/phases.md](../roadmap/phases.md).

| Area | Capabilities |
|---|---|
| Core | Element database, parameters, categories, transactions and undo, levels, grids, save/load |
| Architectural modelling | Walls (types, layers, joins), floors, roofs, ceilings, hosted doors and windows, stairs, railings, curtain walls, rooms, areas |
| Families | Family editor, reference planes, constraints, formulas, types and instances, nested families, shared parameters |
| Documentation | Plans, sections, elevations, callouts, drafting and 3D views; view templates; visibility and graphics overrides; dimensions, tags and text; schedules; sheets and title blocks; printing and PDF/DWG export |
| Structural | Columns, beams, braces, foundations, rebar, analytical model, loads and boundary conditions |
| MEP | Ducts, pipes, cable trays, systems, electrical circuits and panels, sizing and flow calculations |
| Project and collaboration | Phasing, design options, worksharing (central model, element borrowing), linked models, revisions |
| Extended | Site and toposolids, massing and conceptual design, rendering, energy analysis |
| Automation | API, visual scripting (Dynamo) |
| Interoperability | IFC, DWG/DXF, RVT/RFA |

## Known weaknesses

- **Site and civil work is basic.** Toposolids arrived in recent versions, and Autodesk sends roads, grading and terrain work to Civil 3D and InfraWorks.
- For performance, platform and versioning problems, see [industry-pain-points.md](industry-pain-points.md).
