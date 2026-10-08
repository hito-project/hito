# 0004. Unified core with domains and ports/adapters

- **Status:** Accepted (required by [0006](0006-civil-engineering-suite-scope.md))
- **Date:** 2026-10-08

## Context

AEC disciplines are served by separate products (Revit, AutoCAD, Civil 3D, ETABS, Navisworks…) that duplicate the same infrastructure and lose data when exchanging it ([research](https://github.com/hito-project/hito/discussions/77)). Bentley's BIS/iModel shows that one schema-based data core can span disciplines ([prior art](https://github.com/hito-project/hito/discussions/79)).

## Decision

Build a small shared core (element store, schemas, relationships, transactions, coordinates, views). Disciplines plug in as domain schemas, and solvers, importers/exporters and renderers plug in as adapters behind ports. UIs are workspaces. See [architecture/unified-core.md](../architecture/unified-core.md).

## Consequences

- There's no data loss between our own disciplines, and the UX is consistent.
- Revit parity becomes a set of domains and adapters, not special cases.
- Risk of an "everything platform". This is mitigated by delivering usable increments driven by the founding user's needs, and by not generalising an abstraction until a second consumer needs it.
- Existing open-source solvers can be wrapped instead of rewritten.
