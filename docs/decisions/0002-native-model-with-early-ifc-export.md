# 0002. Own native model, IFC export early

- **Status:** Accepted
- **Date:** 2026-10-08

## Context

IFC is the open BIM exchange standard, but it's awkward as a live editing model. Revit's lossy IFC mapping is a well-known pain point ([research](https://github.com/hito-project/hito/discussions/78)). Users can't switch tools unless they can exchange files with others.

## Decision

Use our own native data model internally, and support IFC export from the earliest milestones.

## Consequences

- The internal model can be designed for editing and performance.
- We maintain an IFC mapping layer, verified by automated round-trip tests.
- IFC import, DWG and RVT come later as further adapters. RVT feasibility is an open question ([rvt-interop-feasibility.md](https://github.com/hito-project/hito/issues/8)).
