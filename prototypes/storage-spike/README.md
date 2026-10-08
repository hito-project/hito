# Storage spike prototype

Throwaway benchmark for spike [#5](https://github.com/hito-project/hito/issues/5). It is not product code and will not be merged.

It compares four ways to store a HITO project file:

- **SQLite**, through rusqlite with SQLite bundled
- **redb**, a Rust-native B-tree store
- **fjall**, a Rust-native LSM-tree store
- **A ZIP container**: one compressed entry per model, one stored entry per blob chunk

Each runs the same workload:

1. Insert 1,000,000 element records of about 190 bytes. Each key is a model byte plus a UUID v7, so each model is one key range.
2. Reopen the file.
3. Do 100,000 random point reads.
4. Scan one model, which holds 10% of the records.
5. Update 1,000 random elements in one transaction.
6. Write a 256 MB blob owned by one element, in 1 MB chunks (terrain-like, incompressible data).
7. Read 100 random chunks.

The size columns count every file the engine leaves next to the project (journals, WAL, segment files).

## Run

Rust isn't installed system-wide on the spike machine, so it runs through nix:

```sh
nix shell nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc -c cargo build --release
BENCH_DIR=/some/scratch/dir ./target/release/storage-spike 1000000 256   # records, blob MB, [engine]
```

## Results

Machine: 6 cores, 14 GB RAM, NVMe SSD, Linux. The files were just written, so reads come mostly from the OS page cache. Results from one run:

| Engine | Insert (s) | Size (MB) | Files | Open (ms) | 100k point reads (s) | Model scan, 10% (ms) | Update 1k (ms) | Size after update (MB) | Blob write (s) | 1 MB chunk read (ms) | Final size (MB) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| SQLite | 1.62 | 181.7 | 1 | 0.2 | 0.64 | 16 | 29 | 181.7 | 0.55 | 0.18 | 450.4 |
| redb | 1.69 | 539.0 | 1 | 0.5 | 0.21 | 12 | 21 | 539.0 | 0.72 | 0.09 | 851.4 (after `compact()`; 1077.9 before) |
| fjall | 2.10 | 230.1 | 28 | 768.8 | 0.36 | 36 | 7 | 297.5 | 0.61 | 0.25 | 686.7 |
| ZIP container | 4.43 | 46.6 | 1 | 0.0 | 0.54 | 53 | 4211 | 46.6 | 11.41 | 0.21 | 315.1 |

The raw data is about 460 MB: roughly 190 MB of records plus the 256 MB blob. The evaluation is in the Research discussion linked from #5.
