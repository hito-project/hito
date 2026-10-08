# 0009. License

- **Status:** Accepted
- **Date:** 2026-10-08

## Context

The project owner wants to:

- not gatekeep the software,
- offer it to companies, and
- sell customisation and consulting services.

## Options

| License | Effect | Fit |
|---|---|---|
| **MIT OR Apache-2.0 (dual)** | Permissive. Anyone, including companies, can use, modify and embed it, even in closed products. Apache-2.0 adds an explicit patent grant. | Standard in the Rust ecosystem. Maximises company adoption, and consulting revenue doesn't depend on restricting the code. |
| MIT only | Permissive, with no patent grant | Like the above, with weaker patent protection |
| GPL-3.0 | Derivatives must stay open source | Deters companies from embedding it |
| AGPL-3.0 | Like GPL, and also covers software offered as a network service | Protects against closed SaaS forks, but deters companies most |

## Decision

**MIT OR Apache-2.0**, the Rust ecosystem convention.

## Consequences

- Companies can adopt it freely, which supports the services business model.
- A company could release a closed fork. This is accepted.
- **Dependencies must be compatible.** GPL libraries can't be linked without changing our license. One example is LibreDWG, which affects DWG support ([DWG spike](https://github.com/hito-project/hito/issues/7)).
