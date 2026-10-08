# Suite definition

**Status: not yet researched.**

## Question

Which existing programs does the suite replace, and what is each one's full capability inventory?

## Why it matters

[ADR 0006](../decisions/0006-civil-engineering-suite-scope.md) makes parity the floor for the whole suite. "Complete" has to be defined before it can be planned. Today only Revit has a capability inventory ([research/revit-capabilities.md](../research/revit-capabilities.md)).

## Candidates

The candidates come from [research/aec-software-landscape.md](../research/aec-software-landscape.md):

| Area | Programs to inventory |
|---|---|
| 2D/3D drafting | AutoCAD |
| BIM | Revit (done), ArchiCAD |
| Steel and concrete detailing | Tekla Structures, Advance Steel |
| Civil and infrastructure | Civil 3D, InfraWorks, OpenRoads |
| Structural analysis | CYPECAD (priority for Argentina), ETABS, SAP2000, SAFE, STAAD.Pro, Robot |
| Geotechnical and water | PLAXIS, HEC-RAS/HMS, EPANET, SWMM, WaterGEMS |
| Coordination and reality capture | Navisworks, ReCap |
| Visualization | 3ds Max (rendering only) |
| Project management | Scheduling and cost (4D/5D BIM) |

## To decide

- Which programs are in scope, and which are out (project management, for example).
- For specialist solvers: wrap them (adapter) or replace them. See [why-separate-products.md](../research/why-separate-products.md) on trust and liability.
