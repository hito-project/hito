# Documentation

| Folder | Contents |
|---|---|
| [vision/](vision/README.md) | What we are building, for whom, and the principles behind it |
| [research/](research/README.md) | Findings about Revit, the AEC software market and prior art |
| [architecture/](architecture/README.md) | How the system is structured |
| [decisions/](decisions/README.md) | Architecture Decision Records (ADRs) |
| [roadmap/](roadmap/README.md) | Capability phases and how they map to milestones |
| [backlog/](backlog/README.md) | Epics and stories for M0, M1 and beyond (staged for GitHub) |
| [open-questions/](open-questions/README.md) | Unresolved questions and pending research |
| [glossary.md](glossary.md) | AEC and project terminology |

## Migration to GitHub

These docs are a staging area. [`scripts/migrate-to-github.sh`](../scripts/migrate-to-github.sh) moves the work-tracking content to GitHub's own tools. Run it without `--apply` first to preview.

| Here | On GitHub | After migration |
|---|---|---|
| `vision/`, `architecture/`, `decisions/`, `glossary.md`, `roadmap/` | Stay in the repo as Markdown, versioned with the code | Kept |
| `backlog/` milestones | Milestones (M0, M1) | Delete `backlog/` |
| `backlog/` epics and stories | Issues: epics with stories as sub-issues, plus labels | Delete `backlog/` |
| `research/` | Discussions (category: Research) | Keep `research/README.md` linking to the discussions |
| `open-questions/` | Discussions (Ideas, Q&A), or spike issues when actionable | Delete migrated files |
