# 0013. Storage engine and native file format: SQLite

- **Status:** Proposed
- **Date:** 2026-10-08

## Context

The native `.hito` file holds a project for decades. Spike [#5](https://github.com/hito-project/hito/issues/5) evaluated storage options against these requirements:

- an open, documented, versioned format
- incremental saves
- change sets for undo and future worksharing
- 10⁶–10⁷ elements loaded lazily (SR-5)
- chunked bulk data owned by elements (SR-6)
- linked read-only models (SR-10)
- crash safety
- **one file per project**, so users can email and copy it

The requirements come from the [suite inventories](https://github.com/hito-project/hito/discussions/92) and the [element model](../architecture/element-model.md). The evaluation and benchmark are in [Discussion #98](https://github.com/hito-project/hito/discussions/98).

| Option | Result |
|---|---|
| **SQLite** (rusqlite, SQLite bundled) | One file the size of its data (450 MB for 460 MB of data in the benchmark). Incremental, crash-safe writes. Format documented and unchanged since 2004. `ATTACH` for linked models, a session extension for change sets, incremental BLOB I/O. Written in C. |
| **redb** (Rust) | Fast and crash-safe, one file. But the file is about 1.9× the data even after compaction. The on-disk format has changed three times, with only "a reasonable effort" promised for upgrades. No cross-file queries. |
| **fjall** (Rust) | A directory of files (28 in the benchmark), not one file. Not durable by default. About 0.8 s to open. |
| **ZIP container** (our own content) | Smallest file, because it compresses. But every save rewrites the whole file: 4.2 s to save 1,000 changed elements and 11.4 s to add 256 MB, growing with the project. |

All three databases were fast enough. Speed didn't decide this.

## Decision

**A `.hito` file is a SQLite database with a documented HITO schema.** HITO reads and writes it through **rusqlite**, with SQLite compiled in (the `bundled` feature), so users never need SQLite installed.

All storage goes through a **storage port** ([ADR 0004](0004-unified-core-ports-and-adapters.md)). The element store, domains and UI never use SQL directly.

**Justification for a non-Rust dependency**, as [ADR 0005](0005-rust-first-stack.md) requires:

- **Why no Rust option is good enough.** None of them combines one file, a size close to the data, a stable documented format with a decades-long compatibility promise, and crash-safe incremental writes. The file format outlives every other technical choice. SQLite's format has been unchanged since 2004 and is promised backwards compatible through 2050.
- **Whether it can be replaced later.** Yes, without changing any project file. turso is a SQLite-compatible rewrite in Rust (MIT, pre-1.0 today). Once it is stable, the storage adapter can switch to it. The storage port keeps the switch inside one adapter.
- **License.** SQLite is public domain and rusqlite is MIT, both compatible with [ADR 0009](0009-license.md).

## Consequences

- Project files are open in practice. Any SQLite tool or script can read them, which also makes recovery and third-party tools easy.
- **Linked models (SR-10)** can use `ATTACH ... mode=ro`, up to 125 files per connection.
- **Chunked bulk data (SR-6)** goes in chunk rows read with incremental BLOB I/O.
- **Change sets** can use SQLite's session extension later if useful. The application's own change sets ([element model §9](../architecture/element-model.md)) stay the source of truth.
- The build compiles C. That's one more toolchain requirement for contributors and CI (E02).
- SQLite allows one writer per file. That's fine for a desktop app. Worksharing (E20) will exchange change sets rather than share one file, as iModel does.
- The file format epic (E10, [#46](https://github.com/hito-project/hito/issues/46)) still decides:
  - the table layout, including whether domain properties are columns or an encoded, versioned blob
  - value compression (records compressed about 4× in the ZIP test)
  - the journal mode (with WAL, checkpoint on close so the file is self-contained)
  - the downgrade transform for saving to older versions
- turso is worth reviewing again when it reaches 1.0.
