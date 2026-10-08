# Goal

Build a free, open-source **2D/3D civil engineering suite**. It will replace the separate programs civil engineers use today: drafting, BIM, civil and infrastructure design, analysis and coordination. These all run on one unified core ([ADR 0006](../decisions/0006-civil-engineering-suite-scope.md)).

The ambition is a **silver bullet for civil engineering software**: one free tool that removes the need to juggle separate, expensive, Windows-only programs and lose data between them.

## Scope rule: parity is the floor

- Every capability of the programs the suite replaces is in scope. Nothing is ever classified as "won't do".
- Roadmap discussions decide **order**, never **whether**.
- Exceeding those programs is welcome, especially by removing the data loss between them.
- Which programs the suite replaces is still being defined ([open question](../open-questions/suite-definition.md)).

## Non-negotiables

- **Cross-platform, Linux first.** The programs it replaces are largely Windows-only, and that's the reason the project started. See [target-user.md](target-user.md).
- **Interoperability with existing files.** Users can only switch if they can still exchange RVT, DWG and IFC with everyone else. See [ADR 0002](../decisions/0002-native-model-with-early-ifc-export.md) and [open-questions/rvt-interop-feasibility.md](../open-questions/rvt-interop-feasibility.md).
