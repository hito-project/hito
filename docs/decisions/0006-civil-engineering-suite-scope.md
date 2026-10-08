# 0006. Scope: a complete 2D/3D civil engineering suite

- **Status:** Accepted
- **Date:** 2026-10-08
- **Supersedes:** [0003](0003-full-revit-parity.md)

## Context

[ADR 0003](0003-full-revit-parity.md) set full Revit parity as the minimum scope. Research then showed that civil engineers depend on a whole suite of separate programs (CAD, BIM, civil design, structural analysis, geotechnical and water, coordination), and that most of these share the same core building blocks ([research](../research/why-separate-products.md), [unified core](../architecture/unified-core.md)).

## Decision

The project is a **complete 2D/3D civil engineering suite**. Revit parity becomes one part of the scope rather than all of it. The parity rule from ADR 0003 extends to the whole suite: no capability of the programs the suite replaces is classified as "won't do". Discussions decide order, never whether.

## Consequences

- The unified core with domains and adapters ([ADR 0004](0004-unified-core-ports-and-adapters.md)) becomes the required foundation.
- We need to define which programs make up the suite and inventory their capabilities ([open question](../open-questions/suite-definition.md)).
- The roadmap grows to cover 2D CAD, civil and infrastructure design, analysis, and coordination ([roadmap/phases.md](../roadmap/phases.md)).
- The "everything platform" risk grows. The roadmap must still deliver usable increments, starting with the founding user's needs.
