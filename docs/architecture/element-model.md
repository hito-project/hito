# Element model

**Status: draft, input to E03.** This is the outcome of spike [#6](https://github.com/hito-project/hito/issues/6). It studies two existing data models and recommends a design for HITO's element store and schema system:

- **Bentley's BIS**, the schema behind iModels and the closest existing design to our [unified core](unified-core.md)
- **IFC 4.3** (ISO 16739-1:2024), our main exchange format

The research notes and sources are in [Discussion #97](https://github.com/hito-project/hito/discussions/97).

It also answers three requirements from the [suite inventories](https://github.com/hito-project/hito/discussions/92):

- **SR-2:** several representations per element
- **SR-4:** pluggable placement, including linear referencing
- **SR-9:** one model of time

## How BIS works

BIS describes everything as **elements** in **models**, organised by **schemas**.

| Concept | What it is in BIS |
|---|---|
| **Element** | Anything being modelled: a beam, a document, a requirement. It has a 64-bit `ElementId` (24 bits of briefcase ID plus a 40-bit sequence), an optional `FederationGuid` for matching against external systems, and an optional **Code**: a human-readable name that is unique within a scope (CodeSpec + CodeScope + CodeValue). |
| **Class hierarchy** | Three branches under `Element`: `GeometricElement` (2D or 3D, including `PhysicalElement` and `SpatialLocationElement`), `InformationContentElement` (documents, definitions, types) and `RoleElement`. |
| **Model** | A container that owns elements. Every element lives in exactly one model. A model has no name of its own: it *models* an element at a finer level of detail (`ModelModelsElement`), so models nest. Model kinds match the **modelling perspectives**: physical, functional, analytical, spatial location, drawing and sheet, definition. |
| **Physical backbone** | The physical perspective is the reference point. Elements in other perspectives point to the physical element they describe, for example `AnalyticalElementSimulatesSpatialElement` and `DrawingGraphicRepresentsElement`. The mapping isn't always one to one. |
| **Parent and child** | An element can have one parent in the same model (`ElementOwnsChildElements`). Deleting the parent deletes its children. This is how assemblies work. |
| **Aspects** | Groups of properties owned by an element, from any schema. `ElementUniqueAspect` allows zero or one per element; `ElementMultiAspect` allows any number. They let one domain add data to another domain's elements. BIS's guidance: aspects are for data that belongs to one element; types are for values shared by many. |
| **Types** | `TypeDefinitionElement`s, such as `PhysicalType`, hold shared values. An instance links to its type (`PhysicalElementIsOfType`), and **an instance property with the same name overrides the type's value**. This is Revit's family type, in database form. |
| **Relationships** | Either **embedding** (the parent controls the child's lifetime) or **referencing**. Stored as a foreign key when one side has at most one target, or in a link table when it's many to many or carries properties. `ElementDrivesElement` marks that one element controls another, which is the dependency graph. |
| **Schemas (domains)** | Five layers: core, common (grids, linear referencing, profiles), discipline-physical, discipline-other (analytical, functional) and application. Versions are **Read.Write.Minor**. Same read number: older software can read the data. Same read and write numbers: it can also write. Minor versions only add things. |
| **Linear referencing** | A common schema. Elements located along a linear element (`ILinearlyLocated` along `ILinearElement`) carry a multi-aspect with `DistanceAlongFromStart` plus lateral and vertical offsets, or a from-to range. |
| **Change sets** | Each local copy (a "briefcase") pushes its changes as a change set. Change sets form one linear history. Element IDs include the briefcase ID, so copies never create clashing IDs. Locks prevent concurrent edits to the same element. |
| **Provenance** | `ExternalSourceAspect` records where an element came from and whether it's in sync with that source. Connectors use it to update elements on re-import without changing their identity. |

**Limits of BIS for HITO:**

- **It's designed for federating data, not for authoring.** Bentley's design programs keep their own data models and convert into BIS ([why products are split](https://github.com/hito-project/hito/discussions/77)).
- **Its element IDs need a server** to hand out briefcase IDs.
- **It has no core concept of time.** Its published domain list has no phasing or scheduling schema.

## How IFC models M1

| HITO (M1) | IFC 4.3 | Notes |
|---|---|---|
| Element ID | `IfcGloballyUniqueId` | 128 bits, encoded in 22 characters |
| Level | `IfcBuildingStorey` | Elements belong to it through `IfcRelContainedInSpatialStructure` |
| Grid | `IfcGrid` with `IfcGridAxis` | Elements can be placed on grid intersections with `IfcGridPlacement` |
| Column | `IfcColumn` + `IfcColumnType` | Body is an extruded profile. The profile and material go in `IfcMaterialProfileSetUsage`. Common properties are in `Pset_ColumnCommon` (`LoadBearing`, `Status`, `Reference`). |
| Beam | `IfcBeam` + `IfcBeamType` | Same profile approach as columns. Connections to columns use `IfcRelConnectsElements`. |
| Slab | `IfcSlab` (`FLOOR`) | Thickness and material come from `IfcMaterialLayerSetUsage`. Openings are `IfcOpeningElement` + `IfcRelVoidsElement`. |
| Structural wall | `IfcWall` | Layers come from `IfcMaterialLayerSetUsage`. Joins use `IfcRelConnectsPathElements`. |
| Isolated footing | `IfcFooting` (`PAD_FOOTING`) | |
| Concrete material | `IfcMaterial` + `Pset_MaterialConcrete` | Includes compressive strength |
| Type parameters | `IfcRelDefinesByType` | Property sets can sit on the type and on the instance. If both have the same property, the instance value wins, as in BIS. |
| Georeference (#93) | `IfcMapConversion` + `IfcProjectedCRS` | |
| Analytical model (later) | `IfcStructuralAnalysisModel` with `IfcStructuralCurveMember` and `IfcStructuralSurfaceMember` | Linked to the physical element through `IfcRelAssignsToProduct`. Loads and results are grouped into load groups and result groups. |

IFC has several representations **inside** one element, each named by a `RepresentationIdentifier`:

- `Body`: the 3D shape
- `Axis`: the single-line representation
- `FootPrint`: the 2D outline
- `Box`, `Annotation`, `Clearance` and others

It also has three placement kinds:

- `IfcLocalPlacement`: relative to another placement, or absolute
- `IfcGridPlacement`: on a grid intersection
- `IfcLinearPlacement`: along a curve, with an optional `CartesianPosition` fallback for software that doesn't support linear placement

Element status over time is a single property, `Status`, with the values `NEW`, `EXISTING`, `DEMOLISH` and `TEMPORARY`. Schedules link tasks to elements through `IfcRelAssignsToProcess`.

## Recommendations for HITO

Each recommendation names its source. M1 builds only what M1 needs, but none of these choices has to be undone later.

### 1. Identity: 128-bit IDs, generated locally

Every element gets a **128-bit UUID** (version 7, which is ordered by time, so new IDs sort together for better storage locality).

- It maps one to one to the IFC `GlobalId` (story #15).
- It needs no server, unlike BIS briefcase IDs, so offline and multi-user work (E20) can't produce clashing IDs.
- Inside a running session, the store can use compact handles for speed. The UUID is what's persisted and exported.

**Codes** (from BIS) are optional, human-readable names that are unique within a scope. Structural marks are the first use: Argentine drawings label columns, beams and slabs as C, V and L (*columna*, *viga*, *losa*), numbered per floor. A code isn't the identity. Renaming C12 doesn't create a new element.

**Provenance** (from BIS `ExternalSourceAspect`): imported and linked elements record their source ID. Re-importing then updates elements instead of replacing them. This is what keeps IDs stable for external schedules and budgets (SR-15).

### 2. Models: one container concept, from the start

Every element lives in exactly one **model**. Models have kinds:

- **physical:** the M1 structure
- **definition:** types, materials and profiles
- **drawing:** 2D content per view or sheet
- **analytical:** later
- **linked:** read-only external content (SR-10)

This is BIS's design, kept small. It's cheap now, and it gives later features an obvious home: the analytical model, 2D drafting, linked DWG and IFC files, and per-model loading for large projects (SR-5). M1 uses a single physical model plus a definition model.

### 3. Classes and schemas: defined in Rust, described at runtime

A small set of core base classes:

- `Element`
- `GeometricElement3d` and `GeometricElement2d`
- `SpatialLocationElement`: levels, grids, and later alignments
- `DefinitionElement`: types, materials, profiles
- `InformationElement`

Domains define their classes in Rust, so they're type-checked. A **runtime schema registry** describes every class, property and unit, so the file can store schema versions. With the registry, an older HITO that opens a newer file can **keep data from domains it doesn't know** instead of dropping it (story #48).

Adopt BIS's **Read.Write.Minor** versioning rule as is (story #14).

### 4. Properties: on the class, in aspects, and on the type

| Where | What goes there | Example |
|---|---|---|
| The element's class | Properties every element of that class has | A column's height |
| A unique aspect | One domain adding data to another domain's element | Structural properties on an architectural wall |
| A multi aspect | Repeating data owned by one element | Linear-referencing locations, IFC property sets imported from other software |
| The type | Values shared by all instances. Instances can override them by name. | A column type's section and concrete grade |

### 5. Relationships: three kinds, plus "drives"

| Kind | Meaning | M1 example |
|---|---|---|
| **Owns** (embedding) | The child's lifetime belongs to the parent. Deleting the parent deletes the child. | A slab owns its openings |
| **Refers to** | A link with no lifetime control. It can carry properties. | A beam frames into a column (with the connection end); an element is of a type |
| **Groups** | Membership in a set | Elements in a selection set or a system |

Any of these can be marked **drives** (BIS `ElementDrivesElement`): a change to the source reruns the target's regeneration. That makes it the dependency graph for story #16:

- A level drives the columns that reference it.
- A column drives the beams that frame into it.

Keeping the dependency graph explicit, as edges in the store, is also what makes **incremental recompute** possible later (SR-7). Only the elements downstream of a change are rebuilt.

### 6. Representations (SR-2): two levels

1. **Inside an element: several named geometry representations** (from IFC): `Body`, `Axis`, `FootPrint`, and a plan symbol. All are generated from the same parameters, and views pick the one they need. A beam has an `Axis` line and a `Body` solid. A plan view at 1:100 can draw a column's `FootPrint` hatched.
2. **Across perspectives: separate elements, linked** (from BIS): an analytical member is its own element in an analytical model. It's linked to its physical beam by a "simulates" relationship, and the link isn't always one to one. The analytical element has its own editable properties (releases, offsets, loads), so editing it doesn't break the physical model. This is how BIS, IFC (`IfcRelAssignsToProduct`) and Revit's analytical model all work.

The rule for choosing: **if it's derived, it's a representation. If someone edits it independently, it's an element.**

### 7. Placement (SR-4): one placement type with several kinds

An element's placement is one of:

| Kind | Defined by | Use |
|---|---|---|
| **Local** | A 64-bit transform, relative to world, a level or a parent element | Most elements; level-relative placement in M1 |
| **Grid** | An intersection of two grid axes, plus offsets | Columns on grid intersections, so they move when the grids move (a Revit behaviour users expect) |
| **Linear** | A curve element, distance along it, and lateral and vertical offsets | Civil work (E24): stations along an alignment |

- **Every kind resolves to a cached world transform.** Rendering, queries and export use the cache. Only the placement's own references (the level, the grid axes, the curve) drive it, through the dependency graph.
- **Export can always fall back to cartesian coordinates**, which is exactly IFC 4.3's `CartesianPosition` fallback.
- **M1 needs local and grid placement.** Linear placement is added with E24, as a new kind, not a redesign.

### 8. Time (SR-9): phases and links, built after M1

Neither source has one model of time:

- **IFC** has a status value and links schedule tasks to elements.
- **BIS** has nothing in its core.
- **Revit** has phases: each element has a "phase created" and a "phase demolished", and views filter by phase.

Recommendation:

- **Phase elements form an ordered sequence.** Each element can record the phase it's created in and the phase it's demolished in. This is Revit's model. It also maps to IFC's status values per phase (`NEW`, `EXISTING`, `DEMOLISH`, `TEMPORARY`).
- **Staged construction** (E26) uses the same mechanism, with its own phase sequence for the analysis.
- **4D schedules** (E30, #95) link tasks to elements through relationships, as IFC's `IfcRelAssignsToProcess` does. An element's state on a date is derived from its tasks.

All of this is **additive**: an aspect plus relationships, added with a minor schema version. M1 builds none of it, and nothing in M1 has to change for it.

### 9. Change sets: one structure for undo, saving and collaboration

A committed transaction produces one change set:

- elements, aspects and relationships created, modified or deleted
- before and after values for each

The same structure drives:

- undo and redo (E04)
- incremental saving (#5)
- later, sharing changes between users (E20)

This is BIS's change-set design. Because IDs are UUIDs, no server is needed to allocate them. Whether to coordinate concurrent edits with locks, as BIS does, or by merging is a decision for E20, not for now.

## Impact on M1 stories

| Story | What this document adds |
|---|---|
| #14 Schemas | Core base classes, Rust-defined domains with a runtime registry, Read.Write.Minor versioning, aspects |
| #15 Stable IDs | UUID v7, codes as scoped names, provenance for re-import |
| #16 Relationships | Owns, refers to and groups, with "drives" edges as the dependency graph |
| #17 Queries | Queries by class need the registry's inheritance; queries by model and by code |
| #19, #21 Undo and change sets | One change-set structure, covering aspects and relationships as well as elements |
| #33, #34 Levels and grids | Spatial location elements; grid placement for columns |
| #37–#41 Structural elements | Representations `Body`, `Axis` and `FootPrint`; types with instance overrides |
| #47–#49 File format | Models, schema versions and unknown-domain data are part of the format |
| #51 IFC export | The mapping table above |
