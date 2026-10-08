# Why AEC software is split into separate products

## Why vendors ship separate products

1. **History.** The products were built or acquired separately. AutoCAD dates from 1982 and is drafting-centric. Revit was a startup product that Autodesk acquired in 2002, with its own data model and geometry engine. Civil 3D is built on AutoCAD, not Revit. Merging programs with different data models and decades of file compatibility is extremely expensive.
2. **Business.** Separate products can be priced and sold separately. The AEC Collection is a bundle of separate apps, not one program.
3. **Scale and precision.** A building is about 100 m across and needs millimetre precision. Infrastructure spans tens of kilometres in georeferenced coordinates. A core only handles both if it's designed for both.
4. **Different ways of modelling.** CAD uses drawing primitives. BIM uses semantic objects. Civil design uses terrain surfaces and solids swept along alignments.

## Why users haven't moved to a unified solution

Bentley's iTwin unifies the **data layer**, but its design programs (OpenRoads, STAAD, PLAXIS…) are still separate desktop applications. Connectors convert their output, and Revit's, into a shared iModel. Nobody has shipped a unified *design* program across disciplines.

Even if one existed, these forces would remain:

1. **Network effects.** Clients, architects and contractors exchange `.rvt` and `.dwg`. Firms use what their partners use.
2. **Disciplines are different companies.** The architect, structural engineer, MEP engineer and contractor are usually separate firms, each choosing its own tools.
3. **Trust and liability.** Structural results carry legal responsibility. ETABS and SAP2000 have decades of validation, and a new solver needs years to earn the same trust.
4. **Depth.** Each specialist tool contains decades of domain logic: design codes, fabrication rules, hydraulic methods.
5. **Vendor incentives.** Separate products are profitable.

## What this means for the project

- The barriers are about **adoption**, not technology. A unified core is still sound engineering: less duplicated infrastructure, no data loss inside our tool, and one consistent UX. See [ADR 0004](../decisions/0004-unified-core-ports-and-adapters.md).
- **Interoperability, not unification, decides adoption.** See [vision/principles.md](../vision/principles.md) principle 8.
- The project adopted the unified suite as its scope ([ADR 0006](../decisions/0006-civil-engineering-suite-scope.md)).
- Wrap trusted solvers and export to ETABS/SAP before considering our own solvers.

## Sources

- [iTwin.js: iModel overview](https://www.itwinjs.org/learning/imodels/)
- [iTwin.js: BIS](https://www.itwinjs.org/bis)
