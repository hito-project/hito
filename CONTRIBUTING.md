# Contributing to HITO

Thanks for your interest! HITO is in its early stages. Read [docs/vision](docs/vision/README.md) first to understand what we are building and why.

## How work flows

```
Discussion  →  ADR (if a decision is needed)  →  Epic  →  Stories  →  Pull requests
```

1. **Ideas and questions** start in Discussions. Describe the problem and the workflow, not just a solution.
2. **Decisions** that shape the architecture or product are recorded as ADRs in [docs/decisions](docs/decisions/README.md). Accepted ADRs are never edited. A new ADR supersedes the old one.
3. **Epics** (`type:epic`) are capability areas, grouped into **milestones** (usable releases).
4. **Stories** (`type:story`) are user-visible increments, attached to their epic as sub-issues. **Spikes** (`type:spike`) are time-boxed research stories that end in an ADR or a design document.
5. **Tasks are not tracked as issues.** Break a story down however you like, in your PR or in a checklist inside the story.

## Picking up work

- Look at the current milestone and pick an unassigned story or spike. Comment to claim it.
- `good first issue` and `help wanted` mark good entry points.
- For anything not covered by an existing story, open a Discussion first.

## Ground rules

- **Rust first.** Prefer dependencies written in Rust. A non-Rust dependency needs a recorded justification ([ADR 0005](docs/decisions/0005-rust-first-stack.md)).
- **License compatibility.** Dependencies must be compatible with MIT/Apache-2.0, so no GPL ([ADR 0009](docs/decisions/0009-license.md)).
- **Nothing region-specific in the core.** No hardcoded strings, units or design-code rules ([ADR 0008](docs/decisions/0008-argentina-first-international-by-design.md)).
- **Familiar UX.** Follow the conventions of Revit, AutoCAD and CYPE. Departures need an ADR ([ADR 0007](docs/decisions/0007-ui-familiar-to-current-users.md)).

## Labels

| Label | Meaning |
|---|---|
| `type:epic` / `type:story` / `type:spike` / `type:bug` | Work item type |
| `area:*` | The part of the system (core, ui, geometry, structural, interop…) |

## License

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in HITO, as defined in the Apache-2.0 license, shall be dual licensed under MIT OR Apache-2.0, without any additional terms or conditions.

## Code of conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md).
