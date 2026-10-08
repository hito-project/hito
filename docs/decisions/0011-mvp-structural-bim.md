# 0011. MVP: structural BIM

- **Status:** Accepted
- **Date:** 2026-10-08

## Context

The suite is huge ([ADR 0006](0006-civil-engineering-suite-scope.md)), and the first usable milestone has to be small and valuable, and it has to exercise the core. In Argentina, most civil engineers work on reinforced-concrete buildings, and structural modelling is the most common Revit use ([workflows](../research/civil-engineer-workflows.md)).

## Decision

The first usable milestone (**M1**) is a **structural BIM modeller**:

- Levels and structural grids
- Reinforced-concrete columns, beams, slabs, walls and isolated footings, with parametric types and materials
- Elements that relate to each other: beams frame into columns, and elements follow level changes
- Generated plan views per level and a 3D view, with associative dimensions and PDF export
- Native save/load with a versioned, documented format
- IFC4 export
- Spanish and English UI, metric units

It is preceded by **M0, Foundations**: the research spikes plus the repository and CI setup.

## Out of scope for M1 (later milestones)

Analysis, CIRSOC design checks, rebar, sheets and schedules, families editor, architecture, MEP, worksharing, DWG, RVT.

## Consequences

- The MVP slices across roadmap phases 0, 3 and 4 ([phases](../roadmap/phases.md)), so milestones follow releases, not phases.
- The backlog for M0 and M1 is written in full as epics and stories ([backlog](../backlog/README.md)). Later work is listed as epics only.
- Rebar and CIRSOC checks are the natural next milestone after M1, because they build directly on the M1 elements.
