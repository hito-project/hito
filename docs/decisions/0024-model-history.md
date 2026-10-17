# 0024. Model history: a hash-chained log of change sets, branches as layers, merge first

- **Status:** Accepted
- **Date:** 2026-10-17

## Context

Undo ([#19](https://github.com/hito-project/hito/issues/19)), change sets ([#21](https://github.com/hito-project/hito/issues/21)), saving (E10, [#46](https://github.com/hito-project/hito/issues/46)) and collaboration (E20, [#64](https://github.com/hito-project/hito/issues/64)) all record the model's history. Spike [#156](https://github.com/hito-project/hito/issues/156) designed one history for all of them, "git for objects": from a single edit up to named versions, branches and merges. [ADR 0013](0013-storage-engine-and-file-format.md) already chose SQLite for the `.hito` file and made the application's own change sets the source of truth. [#157](https://github.com/hito-project/hito/pull/157) and [#159](https://github.com/hito-project/hito/pull/159) already make every commit produce a `ChangeSet` of deltas: per field, before and after, the whole element for a creation or deletion, and relationships added or removed.

A prototype ([PR #170](https://github.com/hito-project/hito/pull/170)) measured the design on a generated 100k-element structural model and on a real Revit structural export repeated to 99k elements. Three research reports compared the options. The measurements, the reports and the owner's decisions are in [Discussion #169](https://github.com/hito-project/hito/discussions/169).

| Measured (100k elements, release build) | Result |
|---|---|
| A one-property edit, encoded | 78 B (postcard) |
| Moving a level: commit / undo / redo (5,001 elements) | 23 ms / 2.6 ms / 1.3 ms |
| Commit with an fsync | p50 1.0 ms, p99 2.4 ms |
| Reopen from the stored state / rebuild from a year's log | 0.46 s / 24 s |
| Undo stack of 100 after reopening | under 1 ms |
| Merge two branches of 500 change sets each | 47 ms |
| Show or hide a 1,000-element design option | under 1 ms (copying the store: 167 ms) |

What the research found, in short:

- **Collaboration.** Revit's file-based worksharing needs one authority: Autodesk doesn't support it over Dropbox, Drive or a VPN, and working "at risk" offline can lose changes. Bentley's iTwin puts locks on top of a changeset merge layer. Onshape, Dolt and Figma merge without a server. Every merge tool can produce data that is valid but wrong as engineering, such as two people each moving a column under one beam; they handle it by validating after the merge.
- **Design Options.** Revit tags copies of elements with an option; the copies stop following later edits to the main model, and each view shows one option. Branches would isolate options fully, but as git-style copies they lose side-by-side views and need constant merging.
- **History retention.** Fossil, Dolt and git keep everything and offer a deliberate way to remove history. In Argentina the signed drawings and the calculation report are the legal record (10 years' liability under the Civil Code); no professional council rule asks for model history. Size, privacy and merging decide the policy more than the law does.

## Decision

### Change sets are the events

**A committed transaction's `ChangeSet` is the unit of history.** It is a set of facts, as deltas, and applying it runs no domain logic, so it means the same in any HITO version. The in-memory store produces it; SQLite's session extension isn't used. Derived data, such as generated geometry, isn't recorded.

The core gains three operations, built in #21 and #19:

- `ChangeSet::inverse`: creations become deletions and back, each field goes from after to before, in reverse order.
- `ElementStore::replay`: applies a committed change set without checks, to reopen, redo, receive or rebuild.
- building a `ChangeSet` from stored parts.

Aspects get their own deltas (added, removed, or changed per property) instead of the whole list, so edits to two different aspects of one element don't conflict.

### The record

**The persisted record is the storage adapter's own versioned type, not the core's types with serde.** The core's types will change; a record in a file must read the same for decades. Each record holds:

| Field | Purpose |
|---|---|
| ID: BLAKE3 hash of the canonical encoding and the parent IDs | Content address; a tamper-evident chain, as in git |
| Parent IDs: implicit (the previous record) in a straight-line history, stored for a merge's two parents and a branch's first record | History is a graph, so a merge finds the common ancestor |
| Author, time | Who changed what, and when |
| Schema versions of the domains involved | Replay across schema migrations (#48) |
| Command name and parameters | The undo list and the history shown to users |
| Target: the main model or a branch | Branches and design options |
| The changes, each flagged as made by the user or propagated | Merge reports and conflicts show what someone actually did |

It is encoded with **postcard**, whose wire format is specified and stable since 1.0, and documented in HITO's file format (#49). JSON is rejected: it is the largest, and `serde_json` doesn't round-trip `f64` exactly by default. CBOR works but was about twice the size and slower to decode.

### The file

- **Every commit is one SQLite transaction that appends the record to the log and updates the current state.** A crash loses nothing that was committed (#47).
- **Each commit is fsynced (`synchronous = FULL`).** It costs about 1 ms and protects against power cuts.
- **The file opens from the stored state, not by replaying history.** Replaying a year takes 24 s against 0.46 s. Replay rebuilds the state for recovery and checks it in tests.
- **Undo appends.** Undo and redo commit the inverse change set as a new record; history is never rewritten. The undo stack is rebuilt from the end of the log when a file opens, so **undo survives closing and reopening**, unlike Revit.

### Compression

**Tiered storage, measured on a real Revit structural model.** That model is repeated to 99k elements and given a simulated year of 250,000 realistic edits: 325 MB of history before compression.

| Part | How it's stored | A year on the test model |
|---|---|---|
| **Recent history** (since the last batch) | One row per change set, zstd 3 with a trained dictionary stored in the file. Undo reads only these rows. | about 2 MB |
| **Older history** | Packed in the background, as `git gc` does: about 13 MB of raw records per batch, consecutive in time, cut early at a named version, and sorted by command and property inside the batch, with the order back to time sequence stored. zstd 19. | 25 MB (12.9×) |
| **Current state** | One row per element, zstd 3 with a trained dictionary | 7.4 MB (5.8×) |
| **Record IDs** | A 32-byte ID per record. Parent IDs are stored only where they aren't the previous record (see the record above). | about 8 MB |

The whole file comes to about 47 MB after a year, against 208 MB in the first prototype; that total is an estimate from the measured parts. A fresh file is about 10 MB. Packing a batch takes about 3.5 s of one core, roughly every two weeks at 1,000 edits a day, and never blocks editing. Reading one old batch takes about 7 ms.

What the study ruled out (Discussion #169):
- Compressing each change set harder: zstd 19 alone gives 82 MB, against 48 MB with the dictionary at level 3.
- Other algorithms: brotli is 12% larger, and xz is 7% larger and 10× slower to read.
- A columnar layout with shuffled floats, and storing numbers as XOR deltas: no better than plain packing.
- Batches of about 4 MB: 12% larger.

Content-addressed elements save about 1.5 MB a year and are left for later. zstd is a C library built into the binary, as SQLite is ([ADR 0013](0013-storage-engine-and-file-format.md)). Brotli is the pure-Rust fallback if [ADR 0005](0005-rust-first-stack.md) ever requires one. Batch size and compression level can change later without changing the record format, because a batch is only a way of storing the same records.

These numbers measure compression, not how a HITO file compares with the same project's `.rvt`. **[#174](https://github.com/hito-project/hito/issues/174) checks file size and speed budgets on real projects**, on every PR and weekly, and its full check is an exit criterion for M1.

### Versions, branches and design options

- **A named version is a name for a record ID.** It is what a clean copy, a condense or a merge refers to.
- **A branch is a layer of changes over the record it forked from.** It is shown re-applied on top of the latest main model, so edits to the main model flow into it (a continuous rebase), and conflicts are flagged where both changed the same field.
- **Design options are branches.** Inside a project they are presented as Revit's Design Options: each view chooses which option it shows, so options can sit side by side on a sheet. Versions and collaboration use a branch view with a compare screen that highlights added, removed and changed elements, like comparing branches on GitHub. That compare screen is a departure from Revit's UI ([ADR 0007](0007-ui-familiar-to-current-users.md)), accepted here because Revit has no equivalent.
- **A merge is a three-way merge at field level, applied as a transaction.** Recompute and the domains' rules run on the result, so a merge that is valid data but wrong engineering raises the same errors as an edit. Conflicts are the same field changed to different values on both sides, or an element modified on one side and deleted on the other. They are shown for the user to resolve.

### Collaboration (E20)

**Merge first, borrowing later.** The merge engine works without a server: by email, shared folders or USB. It is presented in Revit's words (*Sincronizar con central*, *Recargar última*). Element borrowing, as in Revit worksharing, can be added on top when a server or an office host exists. It is optional, never required.

### History retention

**Keep everything by default.** Two deliberate operations reduce it:

- **Send a clean copy:** a new file with the current state and no history, written with `VACUUM INTO` so deleted rows can't be recovered from it. It records the version it came from, so changes made to it can merge back.
- **Condense history:** drops history before a chosen point, keeping every named version as a snapshot and an anchor record that keeps the hash chain verifiable. It ends with `VACUUM`.

## Consequences

- #21 and #19 are built on this design: the record type in the storage adapter, `inverse`, `replay`, and the undo stack rebuilt on open.
- #47 implements the log-and-state transaction and the fsync per commit. #100 documents the tables, and #49 documents the record encoding.
- New stories, under E04 and E20: branches as layers; Design Options; the compare screen; merge with conflict review; clean copy; condense; background packing of old history. #174 keeps file size and speed within budgets on real projects.
- Merge results must always be validated. The domains' rules become part of collaboration, not just editing.
- The record format carries fields (parents, target, propagated flags) that M1 doesn't use yet. They cost a few bytes per record and can't be added later without migrating every file.
- Open questions for the founding user, which don't block this design, are listed in Discussion #169: how she shares models in team projects today, whether she uses Design Options, how often several people edit one model, and whether she sends native files or only PDF, DWG or IFC.
