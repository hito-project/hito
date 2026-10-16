# 0023. MPL-2.0 dependencies are allowed

- **Status:** Accepted
- **Date:** 2026-10-16

## Context

HITO is MIT OR Apache-2.0 ([ADR 0009](0009-license.md)), and `deny.toml` only allows permissive licenses. It also left out MPL-2.0 together with GPL, AGPL and LGPL, as a cautious default rather than something ADR 0009 required.

MPL-2.0 is a weak, file-level copyleft:

- Using an MPL crate unmodified only requires keeping its license notice. Its source is already public on crates.io.
- Whoever changes an MPL-licensed file and distributes the program must publish that file's changes under MPL-2.0. The rest of the program keeps its own license, so MPL code can sit in a permissively licensed or a closed program.

The exclusion has already ruled out candidates that were otherwise worth evaluating: persistent collections (`im`, `imbl`, `rpds`) for transactions (#157), and numbat through its `option-ext` dependency (#162). Neither was rejected only for its license, but the next candidate could be.

## Decision

**MPL-2.0 is an allowed dependency license.** `deny.toml` lists it.

- We use MPL crates unmodified. If we ever need to change one, through a fork or a `[patch]`, we publish those files' changes under MPL-2.0. As an open-source project that is no extra burden, but the PR that does it must say so.
- GPL, AGPL and LGPL stay excluded: they would change HITO's own license terms or restrict how it can be linked.
- Licenses of bundled data count too. `cargo deny` only checks a crate's declared license, so a dependency that ships data files (unit databases, fonts, catalogues) needs a look at those files' licenses as well. One example is rink-core, whose MPL code bundles a GPL units database (#162).

## Consequences

- More crates qualify, including persistent collections and parts of the wider Rust ecosystem published under MPL.
- A company embedding HITO that modifies an MPL dependency must publish those files' changes. HITO's own code stays MIT OR Apache-2.0, so ADR 0009's goals still hold.
- Evaluations of existing crates no longer reject MPL candidates outright. Candidates are judged on fit, maturity and dependency weight.
