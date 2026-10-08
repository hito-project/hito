//! Throwaway benchmark for spike #5: storage engine and native file format.
//!
//! Compares SQLite (rusqlite), redb, fjall and a ZIP container on the
//! workloads HITO's file format must handle: about a million small element
//! records, range scans per model, small incremental updates, and large
//! chunked blobs owned by elements (TIN surfaces, analysis results).
//!
//! Run: `cargo run --release -- [records] [blob_mb]` (defaults: 1000000 256).

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

const MODELS: u8 = 10;
const CHUNK: usize = 1 << 20;
const POINT_READS: usize = 100_000;
const UPDATES: usize = 1_000;
const CHUNK_READS: usize = 100;

/// Key = model kind byte + 128-bit UUID v7, so one model is one key range.
type Key = [u8; 17];

struct Data {
    keys: Vec<Key>,
    blob: Vec<u8>,
}

fn key(model: u8, id: uuid::Uuid) -> Key {
    let mut k = [0u8; 17];
    k[0] = model;
    k[1..].copy_from_slice(id.as_bytes());
    k
}

/// A plausible element record of about 180 bytes: class, type ID, a dozen
/// parameters and a short property string.
fn record(i: u64, version: u8) -> Vec<u8> {
    let mut v = Vec::with_capacity(192);
    v.extend_from_slice(&((i % 7) as u16).to_le_bytes());
    v.extend_from_slice(&[version; 16]);
    for p in 0..12u64 {
        v.extend_from_slice(&(((i * 31 + p) % 10_000) as f64 * 0.01).to_le_bytes());
    }
    v.extend_from_slice(format!("Hormigon H-30 C{} nivel {} v{version}", i % 400, i % 12).as_bytes());
    v
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[derive(Default)]
struct Row {
    insert_s: f64,
    size_mb: f64,
    files: usize,
    open_ms: f64,
    point_reads_s: f64,
    model_scan_ms: f64,
    update_ms: f64,
    size_after_update_mb: f64,
    blob_write_s: f64,
    chunk_read_ms: f64,
    size_final_mb: f64,
}

fn secs(t: Instant) -> f64 {
    t.elapsed().as_secs_f64()
}

fn disk_usage(path: &Path) -> (f64, usize) {
    fn walk(p: &Path, total: &mut u64, files: &mut usize) {
        if p.is_dir() {
            for e in fs::read_dir(p).unwrap() {
                walk(&e.unwrap().path(), total, files);
            }
        } else if let Ok(m) = fs::metadata(p) {
            *total += m.len();
            *files += 1;
        }
    }
    let (mut total, mut files) = (0, 0);
    let parent = path.parent().unwrap();
    let stem = path.file_name().unwrap().to_string_lossy().to_string();
    for e in fs::read_dir(parent).unwrap() {
        let p = e.unwrap().path();
        // Count side files too (journals, WAL), which matter for "one file".
        if p.file_name().unwrap().to_string_lossy().starts_with(&stem) {
            walk(&p, &mut total, &mut files);
        }
    }
    (total as f64 / 1e6, files)
}

fn update_set(data: &Data) -> Vec<(Key, u64)> {
    let mut rng = Rng(0x5eed);
    (0..UPDATES)
        .map(|_| {
            let i = rng.below(data.keys.len());
            (data.keys[i], i as u64)
        })
        .collect()
}

// ---------------------------------------------------------------- SQLite

fn bench_sqlite(data: &Data, dir: &Path) -> Row {
    use rusqlite::{Connection, MAIN_DB, params};
    let path = dir.join("project.sqlite");
    let mut row = Row::default();

    let t = Instant::now();
    {
        let mut db = Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE element(key BLOB PRIMARY KEY, data BLOB NOT NULL) WITHOUT ROWID;
             CREATE TABLE chunk(id INTEGER PRIMARY KEY, element BLOB NOT NULL, idx INTEGER NOT NULL,
                                data BLOB NOT NULL, UNIQUE(element, idx));",
        )
        .unwrap();
        let tx = db.transaction().unwrap();
        {
            let mut st = tx.prepare("INSERT INTO element VALUES (?1, ?2)").unwrap();
            for (i, k) in data.keys.iter().enumerate() {
                st.execute(params![&k[..], record(i as u64, 0)]).unwrap();
            }
        }
        tx.commit().unwrap();
    }
    row.insert_s = secs(t);
    (row.size_mb, row.files) = disk_usage(&path);

    let t = Instant::now();
    let mut db = Connection::open(&path).unwrap();
    db.query_row("SELECT count(*) FROM sqlite_schema", [], |r| r.get::<_, i64>(0)).unwrap();
    row.open_ms = secs(t) * 1e3;

    let mut rng = Rng(42);
    let t = Instant::now();
    {
        let mut st = db.prepare_cached("SELECT data FROM element WHERE key = ?1").unwrap();
        for _ in 0..POINT_READS {
            let k = &data.keys[rng.below(data.keys.len())];
            let v: Vec<u8> = st.query_row([&k[..]], |r| r.get(0)).unwrap();
            assert!(!v.is_empty());
        }
    }
    row.point_reads_s = secs(t);

    let t = Instant::now();
    {
        let mut st = db.prepare("SELECT data FROM element WHERE key >= ?1 AND key < ?2").unwrap();
        let mut n = 0usize;
        let mut rows = st.query(params![&[3u8][..], &[4u8][..]]).unwrap();
        while let Some(r) = rows.next().unwrap() {
            let v: Vec<u8> = r.get(0).unwrap();
            n += v.len().min(1);
        }
        assert!(n > 0);
    }
    row.model_scan_ms = secs(t) * 1e3;

    let t = Instant::now();
    {
        let tx = db.transaction().unwrap();
        {
            let mut st = tx.prepare("UPDATE element SET data = ?2 WHERE key = ?1").unwrap();
            for (k, i) in update_set(data) {
                st.execute(params![&k[..], record(i, 1)]).unwrap();
            }
        }
        tx.commit().unwrap();
    }
    row.update_ms = secs(t) * 1e3;
    row.size_after_update_mb = disk_usage(&path).0;

    let t = Instant::now();
    {
        let tx = db.transaction().unwrap();
        {
            let mut st = tx.prepare("INSERT INTO chunk(element, idx, data) VALUES (?1, ?2, ?3)").unwrap();
            for (idx, c) in data.blob.chunks(CHUNK).enumerate() {
                st.execute(params![&data.keys[0][..], idx as i64, c]).unwrap();
            }
        }
        tx.commit().unwrap();
    }
    row.blob_write_s = secs(t);

    let n_chunks = data.blob.len() / CHUNK;
    let mut rng = Rng(7);
    let t = Instant::now();
    for _ in 0..CHUNK_READS {
        let idx = rng.below(n_chunks) as i64;
        let id: i64 = db
            .query_row(
                "SELECT id FROM chunk WHERE element = ?1 AND idx = ?2",
                params![&data.keys[0][..], idx],
                |r| r.get(0),
            )
            .unwrap();
        // Incremental BLOB I/O: read without materialising the whole row.
        let mut b = db.blob_open(MAIN_DB, c"chunk", c"data", id, true).unwrap();
        let mut buf = vec![0u8; CHUNK];
        b.read_exact(&mut buf).unwrap();
    }
    row.chunk_read_ms = secs(t) * 1e3 / CHUNK_READS as f64;
    drop(db);
    row.size_final_mb = disk_usage(&path).0;
    row
}

// ---------------------------------------------------------------- redb

fn bench_redb(data: &Data, dir: &Path) -> Row {
    use redb::{Database, ReadableDatabase, TableDefinition};
    const ELEMENTS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("element");
    const CHUNKS: TableDefinition<(&[u8], u32), &[u8]> = TableDefinition::new("chunk");
    let path = dir.join("project.redb");
    let mut row = Row::default();

    let t = Instant::now();
    {
        let db = Database::create(&path).unwrap();
        let tx = db.begin_write().unwrap();
        {
            let mut table = tx.open_table(ELEMENTS).unwrap();
            for (i, k) in data.keys.iter().enumerate() {
                table.insert(&k[..], &record(i as u64, 0)[..]).unwrap();
            }
        }
        tx.commit().unwrap();
    }
    row.insert_s = secs(t);
    (row.size_mb, row.files) = disk_usage(&path);

    let t = Instant::now();
    let db = Database::open(&path).unwrap();
    row.open_ms = secs(t) * 1e3;

    let mut rng = Rng(42);
    let t = Instant::now();
    {
        let tx = db.begin_read().unwrap();
        let table = tx.open_table(ELEMENTS).unwrap();
        for _ in 0..POINT_READS {
            let k = &data.keys[rng.below(data.keys.len())];
            let v = table.get(&k[..]).unwrap().unwrap();
            assert!(!v.value().is_empty());
        }
    }
    row.point_reads_s = secs(t);

    let t = Instant::now();
    {
        let tx = db.begin_read().unwrap();
        let table = tx.open_table(ELEMENTS).unwrap();
        let mut n = 0usize;
        for e in table.range(&[3u8][..]..&[4u8][..]).unwrap() {
            let (_, v) = e.unwrap();
            n += v.value().len().min(1);
        }
        assert!(n > 0);
    }
    row.model_scan_ms = secs(t) * 1e3;

    let t = Instant::now();
    {
        let tx = db.begin_write().unwrap();
        {
            let mut table = tx.open_table(ELEMENTS).unwrap();
            for (k, i) in update_set(data) {
                table.insert(&k[..], &record(i, 1)[..]).unwrap();
            }
        }
        tx.commit().unwrap();
    }
    row.update_ms = secs(t) * 1e3;
    row.size_after_update_mb = disk_usage(&path).0;

    let t = Instant::now();
    {
        let tx = db.begin_write().unwrap();
        {
            let mut table = tx.open_table(CHUNKS).unwrap();
            for (idx, c) in data.blob.chunks(CHUNK).enumerate() {
                table.insert((&data.keys[0][..], idx as u32), c).unwrap();
            }
        }
        tx.commit().unwrap();
    }
    row.blob_write_s = secs(t);

    let n_chunks = data.blob.len() / CHUNK;
    let mut rng = Rng(7);
    let t = Instant::now();
    {
        let tx = db.begin_read().unwrap();
        let table = tx.open_table(CHUNKS).unwrap();
        for _ in 0..CHUNK_READS {
            let idx = rng.below(n_chunks) as u32;
            // Copy out, as the other engines do, instead of borrowing the mmap.
            let v = table.get((&data.keys[0][..], idx)).unwrap().unwrap().value().to_vec();
            assert_eq!(v.len(), CHUNK);
        }
    }
    row.chunk_read_ms = secs(t) * 1e3 / CHUNK_READS as f64;
    let before = disk_usage(&path).0;
    let mut db = db;
    let t = Instant::now();
    db.compact().unwrap();
    drop(db);
    row.size_final_mb = disk_usage(&path).0;
    eprintln!("redb: {before:.1} MB before compact(), {:.1} MB after, compact took {:.2} s", row.size_final_mb, secs(t));
    row
}

// ---------------------------------------------------------------- fjall

fn bench_fjall(data: &Data, dir: &Path) -> Row {
    use fjall::{Database, KeyspaceCreateOptions, PersistMode};
    let path = dir.join("project.fjall");
    let mut row = Row::default();

    let t = Instant::now();
    {
        let db = Database::builder(&path).open().unwrap();
        let items = db.keyspace("element", KeyspaceCreateOptions::default).unwrap();
        for (i, k) in data.keys.iter().enumerate() {
            items.insert(&k[..], record(i as u64, 0)).unwrap();
        }
        db.persist(PersistMode::SyncAll).unwrap();
    }
    row.insert_s = secs(t);
    (row.size_mb, row.files) = disk_usage(&path);

    let t = Instant::now();
    let db = Database::builder(&path).open().unwrap();
    let items = db.keyspace("element", KeyspaceCreateOptions::default).unwrap();
    row.open_ms = secs(t) * 1e3;

    let mut rng = Rng(42);
    let t = Instant::now();
    for _ in 0..POINT_READS {
        let k = &data.keys[rng.below(data.keys.len())];
        assert!(!items.get(&k[..]).unwrap().unwrap().is_empty());
    }
    row.point_reads_s = secs(t);

    let t = Instant::now();
    let mut n = 0usize;
    for kv in items.prefix([3u8]) {
        n += kv.value().unwrap().len().min(1);
    }
    assert!(n > 0);
    row.model_scan_ms = secs(t) * 1e3;

    let t = Instant::now();
    for (k, i) in update_set(data) {
        items.insert(&k[..], record(i, 1)).unwrap();
    }
    db.persist(PersistMode::SyncAll).unwrap();
    row.update_ms = secs(t) * 1e3;
    row.size_after_update_mb = disk_usage(&path).0;

    let chunks = db.keyspace("chunk", KeyspaceCreateOptions::default).unwrap();
    let t = Instant::now();
    for (idx, c) in data.blob.chunks(CHUNK).enumerate() {
        let mut k = data.keys[0].to_vec();
        k.extend_from_slice(&(idx as u32).to_be_bytes());
        chunks.insert(k, c).unwrap();
    }
    db.persist(PersistMode::SyncAll).unwrap();
    row.blob_write_s = secs(t);

    let n_chunks = data.blob.len() / CHUNK;
    let mut rng = Rng(7);
    let t = Instant::now();
    for _ in 0..CHUNK_READS {
        let mut k = data.keys[0].to_vec();
        k.extend_from_slice(&(rng.below(n_chunks) as u32).to_be_bytes());
        assert_eq!(chunks.get(k).unwrap().unwrap().len(), CHUNK);
    }
    row.chunk_read_ms = secs(t) * 1e3 / CHUNK_READS as f64;
    drop(items);
    drop(chunks);
    drop(db);
    (row.size_final_mb, row.files) = disk_usage(&path);
    row
}

// ---------------------------------------------------------------- ZIP container

/// One compressed entry per model, one stored entry per blob chunk. This is
/// the "pile of files in a ZIP" design (like ODF or OOXML).
fn write_zip(path: &Path, models: &BTreeMap<u8, Vec<(Key, Vec<u8>)>>, blob: Option<&[u8]>, first: &Key) {
    use zip::write::SimpleFileOptions;
    let mut z = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let deflate = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (m, recs) in models {
        z.start_file(format!("models/{m}.bin"), deflate).unwrap();
        for (k, v) in recs {
            z.write_all(k).unwrap();
            z.write_all(&(v.len() as u16).to_le_bytes()).unwrap();
            z.write_all(v).unwrap();
        }
    }
    if let Some(blob) = blob {
        let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let id = uuid::Uuid::from_slice(&first[1..]).unwrap();
        for (idx, c) in blob.chunks(CHUNK).enumerate() {
            z.start_file(format!("blobs/{id}/{idx}"), stored).unwrap();
            z.write_all(c).unwrap();
        }
    }
    z.finish().unwrap();
}

fn parse_model(bytes: &[u8]) -> BTreeMap<Key, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut p = 0;
    while p < bytes.len() {
        let k: Key = bytes[p..p + 17].try_into().unwrap();
        let len = u16::from_le_bytes([bytes[p + 17], bytes[p + 18]]) as usize;
        out.insert(k, bytes[p + 19..p + 19 + len].to_vec());
        p += 19 + len;
    }
    out
}

fn bench_zip(data: &Data, dir: &Path) -> Row {
    let path = dir.join("project.zip");
    let mut row = Row::default();
    let mut models: BTreeMap<u8, Vec<(Key, Vec<u8>)>> = BTreeMap::new();

    let t = Instant::now();
    for (i, k) in data.keys.iter().enumerate() {
        models.entry(k[0]).or_default().push((*k, record(i as u64, 0)));
    }
    write_zip(&path, &models, None, &data.keys[0]);
    row.insert_s = secs(t);
    (row.size_mb, row.files) = disk_usage(&path);

    let t = Instant::now();
    let mut z = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
    row.open_ms = secs(t) * 1e3;

    // Lazy loading is per entry: a point read has to inflate its whole model.
    let mut rng = Rng(42);
    let t = Instant::now();
    let mut cache: BTreeMap<u8, BTreeMap<Key, Vec<u8>>> = BTreeMap::new();
    for _ in 0..POINT_READS {
        let k = data.keys[rng.below(data.keys.len())];
        let m = cache.entry(k[0]).or_insert_with(|| {
            let mut buf = Vec::new();
            z.by_name(&format!("models/{}.bin", k[0])).unwrap().read_to_end(&mut buf).unwrap();
            parse_model(&buf)
        });
        assert!(!m[&k].is_empty());
    }
    row.point_reads_s = secs(t);

    let t = Instant::now();
    let mut buf = Vec::new();
    z.by_name("models/3.bin").unwrap().read_to_end(&mut buf).unwrap();
    assert!(!parse_model(&buf).is_empty());
    row.model_scan_ms = secs(t) * 1e3;

    // An update rewrites the file: changed entries are recompressed, the
    // rest copied raw. This is the best case for a ZIP container.
    let t = Instant::now();
    let updates = update_set(data);
    let changed: std::collections::BTreeSet<u8> = updates.iter().map(|(k, _)| k[0]).collect();
    for (k, i) in &updates {
        cache.get_mut(&k[0]).unwrap().insert(*k, record(*i, 1));
    }
    let tmp = dir.join("project.zip.tmp");
    {
        use zip::write::SimpleFileOptions;
        let mut out = zip::ZipWriter::new(fs::File::create(&tmp).unwrap());
        let deflate = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for m in 0..MODELS {
            let name = format!("models/{m}.bin");
            if changed.contains(&m) {
                out.start_file(name, deflate).unwrap();
                for (k, v) in &cache[&m] {
                    out.write_all(k).unwrap();
                    out.write_all(&(v.len() as u16).to_le_bytes()).unwrap();
                    out.write_all(v).unwrap();
                }
            } else {
                out.raw_copy_file(z.by_name(&name).unwrap()).unwrap();
            }
        }
        out.finish().unwrap();
    }
    fs::rename(&tmp, &path).unwrap();
    row.update_ms = secs(t) * 1e3;
    row.size_after_update_mb = disk_usage(&path).0;

    // Adding a blob also rewrites everything.
    let t = Instant::now();
    let models: BTreeMap<u8, Vec<(Key, Vec<u8>)>> =
        cache.into_iter().map(|(m, recs)| (m, recs.into_iter().collect())).collect();
    write_zip(&path, &models, Some(&data.blob), &data.keys[0]);
    row.blob_write_s = secs(t);

    let mut z = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
    let id = uuid::Uuid::from_slice(&data.keys[0][1..]).unwrap();
    let n_chunks = data.blob.len() / CHUNK;
    let mut rng = Rng(7);
    let t = Instant::now();
    for _ in 0..CHUNK_READS {
        let mut buf = Vec::with_capacity(CHUNK);
        z.by_name(&format!("blobs/{id}/{}", rng.below(n_chunks))).unwrap().read_to_end(&mut buf).unwrap();
        assert_eq!(buf.len(), CHUNK);
    }
    row.chunk_read_ms = secs(t) * 1e3 / CHUNK_READS as f64;
    row.size_final_mb = disk_usage(&path).0;
    row
}

// ---------------------------------------------------------------- main

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).map_or(1_000_000, |s| s.parse().unwrap());
    let blob_mb: usize = args.get(2).map_or(256, |s| s.parse().unwrap());
    let only: Option<&str> = args.get(3).map(|s| s.as_str());
    let dir = PathBuf::from(std::env::var("BENCH_DIR").unwrap_or_else(|_| "bench-data".into()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let mut rng = Rng(1);
    let keys = (0..n).map(|i| key((i % MODELS as usize) as u8, uuid::Uuid::now_v7())).collect();
    // Terrain-like data: mostly incompressible floats.
    let blob = (0..blob_mb * CHUNK).map(|_| rng.next() as u8).collect();
    let data = Data { keys, blob };

    println!("records: {n}, blob: {blob_mb} MB in 1 MB chunks, updates: {UPDATES}, point reads: {POINT_READS}\n");
    println!("| Engine | Insert (s) | Size (MB) | Files | Open (ms) | 100k point reads (s) | Model scan, 10% (ms) | Update 1k (ms) | Size after update (MB) | Blob write (s) | 1 MB chunk read (ms) | Final size (MB) |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
    let engines: [(&str, fn(&Data, &Path) -> Row); 4] = [
        ("SQLite", bench_sqlite),
        ("redb", bench_redb),
        ("fjall", bench_fjall),
        ("ZIP container", bench_zip),
    ];
    for (name, f) in engines {
        if only.is_some_and(|o| !name.to_lowercase().starts_with(o)) {
            continue;
        }
        let r = f(&data, &dir);
        println!(
            "| {name} | {:.2} | {:.1} | {} | {:.1} | {:.2} | {:.0} | {:.0} | {:.1} | {:.2} | {:.2} | {:.1} |",
            r.insert_s, r.size_mb, r.files, r.open_ms, r.point_reads_s, r.model_scan_ms, r.update_ms,
            r.size_after_update_mb, r.blob_write_s, r.chunk_read_ms, r.size_final_mb
        );
    }
}
