# Unified core

**Status: accepted.** See [ADR 0004](../decisions/0004-unified-core-ports-and-adapters.md). This is the foundation of the suite ([ADR 0006](../decisions/0006-civil-engineering-suite-scope.md)).

## Idea

Most AEC programs share the same building blocks and differ mainly in their solvers and UI:

| Building block | CAD | Revit | Civil 3D | Tekla | Navisworks | ETABS/SAP | PLAXIS | EPANET/SWMM | 3ds Max |
|---|---|---|---|---|---|---|---|---|---|
| Element store (IDs, properties, relationships) | simple | ✅ | ✅ | ✅ | read-only | ✅ | ✅ | ✅ | ✅ |
| Transactions, undo, versioning | ✅ | ✅ | ✅ | ✅ | – | ✅ | ✅ | ✅ | ✅ |
| Geometry engine | 2D/3D | B-rep | TIN + swept solids | B-rep | meshes | lines, shells | 3D FE meshes | lines and points | meshes |
| Parametric relationships and recompute | blocks | ✅ | ✅ (corridors) | ✅ | – | – | – | – | modifiers |
| Views and 2D drawings | ✅ | ✅ | ✅ | ✅ | – | partial | – | – | – |
| Georeferencing / large coordinates | partial | partial | ✅ | – | ✅ | – | – | ✅ | – |
| **Specialised solver** | – | – | grading, hydraulics | – | clashes | **FEA** | **geotechnical FE** | **network hydraulics** | renderer |
| Discipline UI | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

So we build the shared rows once, as a core, and add each discipline as a domain, its solvers as adapters, and its UI as a workspace.

## Structure

```
                ┌────────────── CORE ──────────────┐
 UI workspaces  │ Element store (IDs, typed props)  │  Importers/exporters
 (BIM, CAD,     │ Schema system (domains plug in)   │  (IFC, DWG, LandXML,
  Civil, …)     │ Relationships + dependency graph  │   RVT, solver formats)
      ▲         │ Transactions / undo / history     │
      │         │ Coordinates (f64 + georeference)  │  Solvers (ports)
      └─────────│ Views → 2D drawing generation     │  ├─ structural FE
                │ Query + spatial index             │  ├─ hydraulics
                └──────────────┬────────────────────┘  ├─ clash detection
                               │                       └─ renderer
                     Geometry port (B-rep, mesh, TIN, curves)
```

- **Core:** element store with stable IDs, schema system, relationships and dependency propagation, transactions, coordinates, view generation, spatial queries, the operator framework. Kept as small as possible. The element store and schema design is in [element-model.md](element-model.md).
- **Domains:** plug-in schemas that add element types (wall, beam, pipe, alignment, bolt) on shared base classes. This follows the BisCore and domain pattern in Bentley BIS.
- **Adapters:** solvers, importers and exporters, and renderers behind ports. Existing open-source solvers are wrapped, not rewritten, under the solver rules in [ADR 0012](../decisions/0012-suite-definition.md): for example EPANET, SWMM, MYSTRAN, Kratos, and Code_Aster as a separate program.
- **Workspaces:** discipline-specific UI layouts, similar to Revit's discipline tabs.
- **Regional packs:** design codes, units, language and drawing standards, as adapters ([ADR 0008](../decisions/0008-argentina-first-international-by-design.md)).

## Constraints

- **Scope.** Every discipline is in scope, and the core must be designed for all of them, including future epics. Delivery still comes in usable increments, starting with the founding user's needs.
- **No speculative abstractions.** Design the ports up front, but generalise an abstraction only once a second domain needs it.
- **Scale.** Fabrication-level models (millions of bolts) must be feasible, so the element store is designed for that from the start.
- **Coordinates.** Use 64-bit coordinates with a project base point, so building and site scales coexist.
