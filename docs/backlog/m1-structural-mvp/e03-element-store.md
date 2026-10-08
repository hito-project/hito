---
title: "E03: Element store and schemas"
labels: type:epic, area:core
milestone: "M1: Structural BIM MVP"
---

The core's data foundation. Domains define element types in schemas, the store holds elements with stable IDs, typed properties and relationships, and changes propagate to dependent elements. Design input comes from the BIS/IFC spike (E01).

See [architecture/unified-core.md](docs/architecture/unified-core.md).

## Story: Domains define element types through schemas
labels: type:story, area:core

As a domain developer, I want to define element classes, their properties and units in a schema, so that I can add a discipline without changing the core.

### Acceptance criteria
- [ ] A schema declares element classes, inheritance from core base classes, and typed properties with units
- [ ] Schemas are versioned and registered at startup. The core has no knowledge of specific domains.
- [ ] Type and instance parameters are supported (a type's parameters are shared by all its instances)
- [ ] A test domain defines an element type without touching core code

## Story: Elements have stable unique IDs
labels: type:story, area:core

As a user, I want every element to keep its identity across saves, copies between sessions and future collaboration, so that references and exchanges never break.

### Acceptance criteria
- [ ] IDs are globally unique and stable across save and load
- [ ] The ID scheme works for future multi-user editing without conflicts
- [ ] IDs map to IFC GlobalIds for export

## Story: Relationships propagate changes
labels: type:story, area:core

As a user, I want related elements to update when I change one of them, so that the model stays consistent. For example, beams follow a moved column, and elements follow a level's elevation.

### Acceptance criteria
- [ ] Domains declare relationship types (hosted-by, attached-to-level, frames-into…)
- [ ] The core keeps a dependency graph and recomputes dependants after a change, in the correct order
- [ ] Circular dependencies are detected and reported, never looped
- [ ] Deleting an element applies the relationship's declared rule (cascade, detach or block)

## Story: Query elements by type, property and location
labels: type:story, area:core

As a developer, I want to query elements by class, property value and spatial region, so that views, selection and exports can find what they need quickly.

### Acceptance criteria
- [ ] A query API by class (including subclasses) and property filters
- [ ] A spatial index answers region and nearest queries
- [ ] Queries on a 100k-element model return in interactive time (target to be set in the story)
