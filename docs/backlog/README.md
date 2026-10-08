# Backlog

Epics and stories, staged here until they are migrated to GitHub issues by [`scripts/migrate-to-github.sh`](../../scripts/migrate-to-github.sh). After the migration, **GitHub is the source of truth** and these files are deleted.

## Work item hierarchy

| Level | What it is | On GitHub |
|---|---|---|
| **Milestone** | A usable release | Milestone |
| **Epic** | A capability area, delivered by several stories | Issue labelled `type:epic` |
| **Story** | A user-visible increment, or a spike (time-boxed research), small enough for one PR series | Sub-issue of its epic, labelled `type:story` or `type:spike` |

Tasks are deliberately **not** tracked. Contributors break stories down however they like, in the PR or in a checklist inside the story.

## Milestones

| Milestone | Goal | Epics |
|---|---|---|
| [M0: Foundations](m0-foundations/README.md) | Settle the technical unknowns and set up the repo, so M1 can start | E01–E02 |
| [M1: Structural BIM MVP](m1-structural-mvp/README.md) | A usable structural modeller ([ADR 0011](../decisions/0011-mvp-structural-bim.md)) | E03–E12 |
| [Future](future/README.md) | Everything after M1. Epics only, with stories written when they're scheduled. | E13+ |

## File format (parsed by the migration script)

One file per epic:

```markdown
---
title: Epic title
labels: type:epic, area:core
milestone: M1: Structural BIM MVP
---

Epic description (becomes the epic issue body).

## Story: Story title
labels: type:story, area:core

As a <role>, I want <capability>, so that <benefit>.

### Acceptance criteria
- [ ] ...
```

Rules:

- The front matter (`title`, `labels`, `milestone`) is required. Leave `milestone` empty for future epics.
- Everything before the first `## Story:` is the epic body.
- A story runs until the next `## Story:` or the end of the file. Its first line must be `labels:`.
- Stories inherit the epic's milestone.
- Links in bodies are **relative to the repository root** (`docs/decisions/...`), not to the file. The script rewrites them to absolute GitHub URLs, because relative links don't resolve inside issues.
