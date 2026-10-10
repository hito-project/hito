# 0018. HITO is a working name until the first public release

- **Status:** Accepted
- **Date:** 2026-10-10
- **Supersedes:** [0010](0010-project-name-hito.md)

## Context

[ADR 0010](0010-project-name-hito.md) named the project HITO and left the trademark checks pending. Spike [#83](https://github.com/hito-project/hito/issues/83) did them on 2026-10-10. The findings are in [Discussion #120](https://github.com/hito-project/hito/discussions/120).

HITO is registered for goods and services like ours:

- **Argentina:** an architect in Córdoba holds two HITO marks in class 42. One covers software design, civil engineering design and computer-aided design for architecture. He uses HITO® as the brand of a unit of his architecture studio.
- **United States:** the same owner holds HITO in class 42 for architectural and engineering services.
- **European Union:** another company holds HITO as a word mark in class 9 for "computer software" without restriction, and for SaaS in class 42.
- **Argentina:** HITOS was registered in 2026 for downloadable software and SaaS.

Argentina is our first market ([ADR 0008](0008-argentina-first-international-by-design.md)), so the spike recommended a rename. The alternatives in ADR 0010 and about thirty more names were screened, and none was both clear of conflicts and liked by the project owner. The owner wants the name of a well-known tool that changed how the profession works, the way the abacus did.

Nothing has been released or announced. The risk comes from using the name in public for software, so a rename costs about the same now as it will just before the first public release.

## Decision

**HITO stays as the project's working name. It will be replaced before the first public release.**

The repository, crates, license line and documentation keep using HITO until a new ADR picks the public name. Issue [#121](https://github.com/hito-project/hito/issues/121) tracks the choice.

## Consequences

- Work isn't blocked on choosing a name.
- Until the rename, the name stays internal. Nothing is published under the name HITO: no crates on crates.io, no website, no announcements and no public release.
- Rolling development builds ([ADR 0017](0017-development-builds.md)) may keep the name, because they're for testers, not a release.
- The rename will touch the GitHub organisation and repository, crate names, the license line ("The HITO contributors"), the documentation and the build artifacts. The longer we wait, the more there is to change.
- The new name must be screened the same way as in Discussion #120 (crates.io, INPI, USPTO, EUIPO and WIPO, classes 9 and 42), and ideally reviewed by a trademark lawyer before it's announced.
