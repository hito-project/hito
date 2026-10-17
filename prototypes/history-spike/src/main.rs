//! Spike #156: measures change-record size, log appends, undo latency,
//! snapshot cost and reopening, on a generated structural model.
//!
//! Run with `cargo run --release -- [levels]` (default 20 levels, about
//! 100k elements).

mod compress;
mod domain;
mod ifc;
mod merge;
mod record;

use std::path::Path;
use std::time::{Duration, Instant};

use hito_core::{
    ChangeSet, Command, ElementChange, ElementStore, Relationship, RelationshipChange,
};
use rusqlite::{Connection, params};

use domain::{Project, Rng, class};
use record::{ChangeRecord, Encoding};

fn ms(d: Duration) -> String {
    format!("{:.2} ms", d.as_secs_f64() * 1e3)
}

fn kb(bytes: usize) -> String {
    if bytes < 10_000 {
        format!("{bytes} B")
    } else {
        format!("{:.1} KB", bytes as f64 / 1e3)
    }
}

fn time<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let value = f();
    (value, start.elapsed())
}

/// One user edit, as the app would run it: a command in one transaction.
fn edit(project: &mut Project, rng: &mut Rng, kind: usize) -> ChangeSet {
    let level = rng.below(project.levels.len());
    let members = &project.members[level];
    let target = members[rng.below(members.len())];
    match kind {
        // Change one property of one element (most edits).
        0 => {
            let mut tx = project
                .store
                .transaction_for(Command::new("EditProperty").with_parameter("Name", "Comments"));
            tx.set_property(target, "Comments", format!("revisado {}", rng.next() % 100))
                .unwrap();
            tx.commit().unwrap()
        }
        // Move 20 elements.
        1 => {
            let mut tx = project.store.transaction_for(Command::new("Move"));
            for _ in 0..20 {
                let id = members[rng.below(members.len())];
                if tx.view().get(id).is_none() {
                    continue;
                }
                let x = rng.real(120.0);
                tx.set_property(id, "X", x).unwrap();
            }
            tx.commit().unwrap()
        }
        // Place 10 beams.
        2 => {
            let level_id = project.levels[level];
            let beams: Vec<_> = (0..10)
                .map(|_| domain::member(project, rng, "Beam", level))
                .collect();
            let mut tx = project.store.transaction_for(Command::new("PlaceBeam"));
            let mut ids = Vec::new();
            for beam in beams {
                let id = tx.insert(beam).unwrap();
                tx.relate(Relationship::new(class("OnLevel"), id, level_id))
                    .unwrap();
                ids.push(id);
            }
            let changes = tx.commit().unwrap();
            project.members[level].extend(ids);
            changes
        }
        // Delete a selection of 10 elements, so the model keeps its size
        // over a year: placing and deleting balance out.
        _ => {
            let mut tx = project.store.transaction_for(Command::new("Delete"));
            for _ in 0..10 {
                let members = &mut project.members[level];
                let id = members.swap_remove(rng.below(members.len()));
                tx.delete(id).unwrap();
            }
            tx.commit().unwrap()
        }
    }
}

/// The edit mix of a working day: mostly single-property edits.
fn day_kind(rng: &mut Rng) -> usize {
    match rng.below(100) {
        0..70 => 0,
        70..90 => 1,
        90..95 => 2,
        _ => 3,
    }
}

const SCHEMA_SQL: &str = "
    PRAGMA journal_mode = WAL;
    PRAGMA synchronous = NORMAL;
    CREATE TABLE change (
        seq INTEGER PRIMARY KEY,
        hash BLOB NOT NULL UNIQUE,
        parent BLOB,
        author TEXT NOT NULL,
        time INTEGER NOT NULL,
        command TEXT,
        payload BLOB NOT NULL
    );
    CREATE TABLE model (id BLOB PRIMARY KEY, kind INTEGER NOT NULL);
    CREATE TABLE element (id BLOB PRIMARY KEY, record BLOB NOT NULL) WITHOUT ROWID;
    CREATE TABLE relationship (
        source BLOB NOT NULL, target BLOB NOT NULL, class TEXT NOT NULL,
        PRIMARY KEY (source, target, class)
    ) WITHOUT ROWID;
    CREATE TABLE meta (key TEXT PRIMARY KEY, value BLOB);
";

/// A `.hito` file: the change log plus the current state, both written in
/// the same SQLite transaction on every commit.
struct HitoFile {
    db: Connection,
    head: Option<[u8; 32]>,
    encoding: Encoding,
}

impl HitoFile {
    fn create(path: &Path, encoding: Encoding) -> Self {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path.with_extension("hito-wal"));
        let db = Connection::open(path).unwrap();
        db.execute_batch(SCHEMA_SQL).unwrap();
        Self {
            db,
            head: None,
            encoding,
        }
    }

    fn open(path: &Path, encoding: Encoding) -> Self {
        let db = Connection::open(path).unwrap();
        let head = db
            .query_row(
                "SELECT hash FROM change ORDER BY seq DESC LIMIT 1",
                [],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .ok()
            .map(|h| h.try_into().unwrap());
        Self { db, head, encoding }
    }

    /// Appends a change set to the log and writes its effect into the state
    /// tables, atomically. Returns the payload size.
    fn commit(&mut self, changes: &ChangeSet) -> usize {
        let record = ChangeRecord::from_change_set(changes);
        let payload = self.encoding.encode(&record);
        // The hash covers the parent, so it names the whole history up to
        // here, as a git commit does.
        let mut hasher = blake3::Hasher::new();
        if let Some(parent) = &self.head {
            hasher.update(parent);
        }
        hasher.update(&postcard::to_stdvec(&record).unwrap());
        let hash: [u8; 32] = *hasher.finalize().as_bytes();
        let tx = self.db.transaction().unwrap();
        tx.execute(
            "INSERT INTO change (hash, parent, author, time, command, payload) VALUES (?1, ?2, 'spike', 0, ?3, ?4)",
            params![&hash[..], self.head.as_ref().map(|h| &h[..]), changes.command().map(|c| c.name()), &payload],
        )
        .unwrap();
        write_state(&tx, changes);
        tx.commit().unwrap();
        self.head = Some(hash);
        payload.len()
    }

    /// Loads the current state into a store, without replaying history.
    fn load(&self) -> ElementStore {
        let mut store = ElementStore::new(domain::registry());
        let mut models = Vec::new();
        let mut stmt = self.db.prepare("SELECT id, kind FROM model").unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, u8>(1)?)))
            .unwrap();
        for row in rows {
            let (id, kind) = row.unwrap();
            models.push((id.try_into().unwrap(), kind));
        }
        let mut stmt = self.db.prepare("SELECT record FROM element").unwrap();
        let elements = stmt
            .query_map([], |r| r.get::<_, Vec<u8>>(0))
            .unwrap()
            .map(|r| record::ElementChangeRec::Created(postcard::from_bytes(&r.unwrap()).unwrap()))
            .collect();
        let mut stmt = self
            .db
            .prepare("SELECT class, source, target FROM relationship")
            .unwrap();
        let relationships = stmt
            .query_map([], |r| {
                let (c, s, t): (String, Vec<u8>, Vec<u8>) = (r.get(0)?, r.get(1)?, r.get(2)?);
                let (schema, name) = c.split_once('.').unwrap();
                Ok((
                    true,
                    record::RelationshipRec {
                        class: record::ClassRec(schema.into(), name.into()),
                        source: s.try_into().unwrap(),
                        target: t.try_into().unwrap(),
                    },
                ))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let snapshot = ChangeRecord {
            command: None,
            models,
            elements,
            relationships,
        };
        store.replay(&snapshot.to_change_set());
        store
    }

    /// Rebuilds the store by replaying the whole log from the start.
    fn rebuild(&self) -> ElementStore {
        let mut store = ElementStore::new(domain::registry());
        let mut stmt = self
            .db
            .prepare("SELECT payload FROM change ORDER BY seq")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0)).unwrap();
        for row in rows {
            store.replay(&self.encoding.decode(&row.unwrap()).to_change_set());
        }
        store
    }

    /// The last `n` change sets, newest first: the undo stack after reopening.
    fn tail(&self, n: usize) -> Vec<ChangeSet> {
        let mut stmt = self
            .db
            .prepare("SELECT payload FROM change ORDER BY seq DESC LIMIT ?1")
            .unwrap();
        stmt.query_map([n as i64], |r| r.get::<_, Vec<u8>>(0))
            .unwrap()
            .map(|r| self.encoding.decode(&r.unwrap()).to_change_set())
            .collect()
    }
}

fn class_key(c: &hito_core::ClassId) -> String {
    format!("{}.{}", c.schema(), c.name())
}

fn write_state(tx: &rusqlite::Transaction, changes: &ChangeSet) {
    for model in changes.models() {
        let kind = ChangeRecord::from_change_set(&ChangeSet::from_parts(
            None,
            vec![model.clone()],
            vec![],
            vec![],
        ))
        .models[0]
            .1;
        tx.execute(
            "INSERT INTO model VALUES (?1, ?2)",
            params![&model.id().as_u128().to_be_bytes()[..], kind],
        )
        .unwrap();
    }
    let mut upsert = tx
        .prepare_cached("INSERT OR REPLACE INTO element VALUES (?1, ?2)")
        .unwrap();
    let mut delete = tx
        .prepare_cached("DELETE FROM element WHERE id = ?1")
        .unwrap();
    let mut select = tx
        .prepare_cached("SELECT record FROM element WHERE id = ?1")
        .unwrap();
    for change in changes.elements() {
        let id = change.id().as_u128().to_be_bytes();
        match change {
            ElementChange::Created(e) => {
                upsert
                    .execute(params![
                        &id[..],
                        postcard::to_stdvec(&record::element(e)).unwrap()
                    ])
                    .unwrap();
            }
            ElementChange::Deleted(_) => {
                delete.execute(params![&id[..]]).unwrap();
            }
            ElementChange::Modified { .. } => {
                // Read, patch and write the element's row.
                let bytes: Vec<u8> = select.query_row(params![&id[..]], |r| r.get(0)).unwrap();
                let mut element: record::ElementRec = postcard::from_bytes(&bytes).unwrap();
                let record::ElementChangeRec::Modified(_, fields) = &ChangeRecord::from_change_set(
                    &ChangeSet::from_parts(None, vec![], vec![change.clone()], vec![]),
                )
                .elements[0] else {
                    unreachable!()
                };
                fields.iter().for_each(|f| element.patch(f));
                upsert
                    .execute(params![&id[..], postcard::to_stdvec(&element).unwrap()])
                    .unwrap();
            }
        }
    }
    let mut add = tx
        .prepare_cached("INSERT INTO relationship VALUES (?1, ?2, ?3)")
        .unwrap();
    let mut remove = tx
        .prepare_cached("DELETE FROM relationship WHERE source = ?1 AND target = ?2 AND class = ?3")
        .unwrap();
    for change in changes.relationships() {
        let (r, stmt) = match change {
            RelationshipChange::Added(r) => (r, &mut add),
            RelationshipChange::Removed(r) => (r, &mut remove),
        };
        stmt.execute(params![
            &r.source().as_u128().to_be_bytes()[..],
            &r.target().as_u128().to_be_bytes()[..],
            class_key(r.class())
        ])
        .unwrap();
    }
}

fn file_size(path: &Path) -> u64 {
    let wal = path.with_extension("hito-wal");
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
        + std::fs::metadata(wal).map(|m| m.len()).unwrap_or(0)
}

fn percentile(times: &mut [Duration], p: f64) -> Duration {
    times.sort();
    times[((times.len() - 1) as f64 * p) as usize]
}

/// A year of history: `days` working days of `edits` edits, appended to one
/// file, then packed (old change sets compressed together in batches, as git
/// packfiles do) and condensed (history dropped, state kept).
fn year(levels: usize, edits: usize, days: usize) {
    let dir = std::env::var_os("HISTORY_SPIKE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("history-spike"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("year.hito");
    let mut rng = Rng::new(0x5eed_1234_abcd_ef01);
    let (mut project, build) = domain::build(levels, &mut rng);
    let mut file = HitoFile::create(&path, Encoding::PostcardZstd);
    for changes in &build {
        file.commit(changes);
    }
    println!("# A year of history ({days} days × {edits} edits, {levels} levels)\n");
    println!("| after day | file | change sets |\n|---|---|---|");
    for day in 1..=days {
        for _ in 0..edits {
            let kind = day_kind(&mut rng);
            let changes = edit(&mut project, &mut rng, kind);
            file.commit(&changes);
        }
        if day.is_power_of_two() || day == days {
            file.db
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
                .unwrap();
            let n: i64 = file
                .db
                .query_row("SELECT count(*) FROM change", [], |r| r.get(0))
                .unwrap();
            println!("| {day} | {:.1} MB | {n} |", file_size(&path) as f64 / 1e6);
        }
    }
    drop(file);
    let file = HitoFile::open(&path, Encoding::PostcardZstd);
    let (_, t_open) = time(|| file.load());
    let (rebuilt, t_rebuild) = time(|| file.rebuild());
    assert_eq!(rebuilt.elements().count(), project.store.elements().count());
    println!(
        "\nReopen from state: {}. Rebuild from the whole log: {}.\n",
        ms(t_open),
        ms(t_rebuild)
    );

    // Packing: decode every change set and recompress them in batches of
    // 1,000, so zstd sees the repetition between them.
    let payloads: Vec<Vec<u8>> = {
        let mut stmt = file
            .db
            .prepare("SELECT payload FROM change ORDER BY seq")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let individual: usize = payloads.iter().map(Vec::len).sum();
    let raw: Vec<Vec<u8>> = payloads
        .iter()
        .map(|p| zstd::decode_all(&p[..]).unwrap())
        .collect();
    let (packed, t_pack) = time(|| {
        raw.chunks(1000)
            .map(|chunk| {
                let mut batch = Vec::new();
                for r in chunk {
                    batch.extend_from_slice(&(r.len() as u32).to_le_bytes());
                    batch.extend_from_slice(r);
                }
                zstd::encode_all(&batch[..], 19).unwrap().len()
            })
            .sum::<usize>()
    });
    println!("| History | size |\n|---|---|");
    println!(
        "| uncompressed change sets | {:.1} MB |",
        raw.iter().map(Vec::len).sum::<usize>() as f64 / 1e6
    );
    println!(
        "| each change set compressed alone (zstd 3) | {:.1} MB |",
        individual as f64 / 1e6
    );
    println!(
        "| packed in batches of 1,000 (zstd 19, {}) | {:.1} MB |",
        ms(t_pack),
        packed as f64 / 1e6
    );

    // Condensing: drop the history, keep the state, then vacuum.
    let condensed = dir.join("condensed.hito");
    let _ = std::fs::remove_file(&condensed);
    std::fs::copy(&path, &condensed).unwrap();
    let db = Connection::open(&condensed).unwrap();
    let (_, t) = time(|| db.execute_batch("DELETE FROM change; VACUUM;").unwrap());
    println!(
        "\nCondensed (state only, vacuumed in {}): {:.1} MB.",
        ms(t),
        file_size(&condensed) as f64 / 1e6
    );

    // Durability: commit latency when every commit is fsynced.
    let mut file = HitoFile::open(&path, Encoding::PostcardZstd);
    file.db.execute_batch("PRAGMA synchronous = FULL").unwrap();
    let mut times: Vec<_> = (0..200)
        .map(|_| {
            let changes = edit(&mut project, &mut rng, 0);
            time(|| file.commit(&changes)).1
        })
        .collect();
    println!(
        "Commit with synchronous = FULL (fsync each commit): p50 {}, p99 {}.",
        ms(percentile(&mut times, 0.5)),
        ms(percentile(&mut times, 0.99))
    );
}

/// Two branches from one ancestor, merged at field level.
fn merge_demo(levels: usize, edits: usize) {
    let mut rng = Rng::new(0x5eed_1234_abcd_ef01);
    let (base, _) = domain::build(levels, &mut rng);
    let level = base.levels[1];
    let column = base.members[1][0];
    let beam = base.members[1][COLUMN_BEAM];
    let mut ours = Project {
        store: base.store.clone(),
        members: base.members.clone(),
        ..base.clone_ids()
    };
    let mut theirs = Project {
        store: base.store.clone(),
        members: base.members.clone(),
        ..base.clone_ids()
    };
    let mut our_log = Vec::new();
    let mut their_log = Vec::new();
    let (mut rng_a, mut rng_b) = (Rng::new(1), Rng::new(2));
    // Ours: raise level 1, edit the column's comment, edit the beam, plus
    // a day's ordinary edits.
    let mut tx = ours
        .store
        .transaction_for(Command::new("ChangeLevelElevation"));
    tx.set_property(level, "Elevation", 3.5).unwrap();
    our_log.push(tx.commit().unwrap());
    let mut tx = ours.store.transaction_for(Command::new("EditProperty"));
    tx.set_property(column, "Comments", "ours").unwrap();
    tx.set_property(beam, "Comments", "checked").unwrap();
    our_log.push(tx.commit().unwrap());
    for _ in 0..edits {
        let kind = [0, 0, 1, 2][rng_a.below(4)];
        our_log.push(edit(&mut ours, &mut rng_a, kind));
    }
    // Theirs: the same column comment differently, delete the beam, place
    // beams on level 1, plus a day's ordinary edits.
    let mut tx = theirs.store.transaction_for(Command::new("EditProperty"));
    tx.set_property(column, "Comments", "theirs").unwrap();
    their_log.push(tx.commit().unwrap());
    let mut tx = theirs.store.transaction_for(Command::new("Delete"));
    tx.delete(beam).unwrap();
    their_log.push(tx.commit().unwrap());
    theirs.members[1].retain(|&id| id != beam);
    let beams: Vec<_> = (0..10)
        .map(|_| domain::member(&theirs, &mut rng_b, "Beam", 1))
        .collect();
    let mut tx = theirs.store.transaction_for(Command::new("PlaceBeam"));
    let mut new_beams = Vec::new();
    for b in beams {
        let id = tx.insert(b).unwrap();
        tx.relate(Relationship::new(class("OnLevel"), id, level))
            .unwrap();
        new_beams.push(id);
    }
    their_log.push(tx.commit().unwrap());
    for _ in 0..edits {
        let kind = [0, 0, 1, 2][rng_b.below(4)];
        their_log.push(edit(&mut theirs, &mut rng_b, kind));
    }

    let (merged, t) = time(|| merge::merge(&mut ours.store, &our_log, &their_log).unwrap());
    println!("# Merge\n");
    println!(
        "Two branches of {} and {} change sets on a {}-element model merged in {}: {} element changes, {} conflicts, {} modify/delete.\n",
        our_log.len(),
        their_log.len(),
        base.store.elements().count(),
        ms(t),
        merged.changes.elements().len(),
        merged.conflicts.len(),
        merged
            .conflicts
            .iter()
            .filter(|c| matches!(c, merge::Conflict::ModifyDelete { .. }))
            .count()
    );
    for c in merged.conflicts.iter().take(5) {
        println!("- {c:?}");
    }
    let elevation = ours
        .store
        .view()
        .property(new_beams[0], "BaseElevation")
        .cloned();
    let stale = theirs
        .store
        .view()
        .property(new_beams[0], "BaseElevation")
        .cloned();
    println!(
        "\nTheir new beam on level 1: BaseElevation {stale:?} on their branch, {elevation:?} after the merge (our level moved to 3.5 m)."
    );
    assert_eq!(elevation, Some(3.5.into()));
}

/// A design option as an overlay: a lasting set of changes on top of the
/// main model, applied to show the option and removed again.
fn overlay(levels: usize, size: usize) {
    let mut rng = Rng::new(0x5eed_1234_abcd_ef01);
    let (mut project, _) = domain::build(levels, &mut rng);
    let level = project.levels[2];
    let mut tx = project.store.transaction_for(Command::new("Option"));
    for &id in project.members[2].iter().take(size) {
        tx.set_property(id, "BaseOffset", 0.25).unwrap();
        tx.set_property(id, "Comments", "option B").unwrap();
    }
    let option = tx.commit().unwrap();
    let _ = level;
    let ((), show) = time(|| project.store.replay(&option.inverse()));
    let ((), hide) = time(|| project.store.replay(&option));
    let ((), clone) = time(|| drop(project.store.clone()));
    println!("# Design option as an overlay\n");
    println!(
        "Option touching {} elements ({} with recompute) on {} elements: toggle off {}, on {}. Cloning the whole store instead: {}.",
        size,
        option.elements().len(),
        project.store.elements().count(),
        ms(show),
        ms(hide),
        ms(clone)
    );
}

const COLUMN_BEAM: usize = domain::COLUMNS;

/// The compression study on a real model: an IFC file repeated on
/// `floors` floors, then `days` × `edits` realistic edits.
fn real(ifc: &str, floors: usize, edits: usize, days: usize) {
    let dir = std::env::var_os("HISTORY_SPIKE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("history-spike"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("real.hito");
    let ((mut model, build, per_floor), t) = time(|| ifc::build(Path::new(ifc), floors));
    println!(
        "# A real model\n\n{per_floor} elements per floor from `{ifc}`, {floors} floors: {} elements, imported in {}.\n",
        model.store.elements().count(),
        ms(t)
    );
    let mut file = HitoFile::create(&path, Encoding::PostcardZstd);
    for changes in &build {
        file.commit(changes);
    }
    file.db
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    println!(
        "| after day | file | elements |\n|---|---|---|\n| 0 | {:.1} MB | {} |",
        file_size(&path) as f64 / 1e6,
        model.store.elements().count()
    );
    let mut rng = Rng::new(0x5eed_1234_abcd_ef01);
    for day in 1..=days {
        for _ in 0..edits {
            let changes = model.edit(&mut rng);
            file.commit(&changes);
        }
        if day.is_power_of_two() || day == days {
            file.db
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
                .unwrap();
            println!(
                "| {day} | {:.1} MB | {} |",
                file_size(&path) as f64 / 1e6,
                model.store.elements().count()
            );
        }
    }
    drop(file);
    println!();
    compress::study(&path);
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("batching") {
        let path = std::env::args().nth(2).expect("a year file");
        return compress::batching(Path::new(&path));
    }
    if std::env::args().nth(1).as_deref() == Some("real") {
        let args: Vec<String> = std::env::args().collect();
        let n = |i: usize, d| args.get(i).and_then(|a| a.parse().ok()).unwrap_or(d);
        return real(&args[2], n(3, 24), n(4, 1000), n(5, 250));
    }
    if std::env::args().nth(1).as_deref() == Some("compress") {
        let path = std::env::args().nth(2).expect("a year file");
        return compress::study(std::path::Path::new(&path));
    }
    if std::env::args().nth(1).as_deref() == Some("merge") {
        return merge_demo(20, 500);
    }
    if std::env::args().nth(1).as_deref() == Some("overlay") {
        return overlay(20, 1000);
    }
    if std::env::args().nth(1).as_deref() == Some("year") {
        let arg = |n: usize, default| {
            std::env::args()
                .nth(n)
                .and_then(|a| a.parse().ok())
                .unwrap_or(default)
        };
        return year(arg(2, 20), arg(3, 2000), arg(4, 250));
    }
    let levels: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(20);
    let edits: usize = std::env::args()
        .nth(2)
        .and_then(|a| a.parse().ok())
        .unwrap_or(2000);
    let dir = std::env::var_os("HISTORY_SPIKE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("history-spike"));
    std::fs::create_dir_all(&dir).unwrap();
    let mut rng = Rng::new(0x5eed_1234_abcd_ef01);

    let ((mut project, build), t) = time(|| domain::build(levels, &mut rng));
    let elements = project.store.elements().count();
    println!(
        "# Model\n\n{levels} levels, {elements} elements, built in {}\n",
        ms(t)
    );

    // Record sizes for typical change sets, per encoding.
    let single = edit(&mut project, &mut rng, 0);
    let moved = edit(&mut project, &mut rng, 1);
    let placed = edit(&mut project, &mut rng, 2);
    let deleted = edit(&mut project, &mut rng, 3);
    let level = project.levels[levels / 2];
    let mut tx = project
        .store
        .transaction_for(Command::new("ChangeLevelElevation"));
    tx.set_property(level, "Elevation", 31.5).unwrap();
    let (level_move, commit_level) = time(|| tx.commit().unwrap());
    let cases = [
        ("1 property", &single),
        ("move 20", &moved),
        ("place 10 beams", &placed),
        ("delete 1", &deleted),
        ("move a level", &level_move),
        ("create a level's members", &build[1]),
    ];
    println!("# Record size by encoding\n");
    print!("| Change set | elements |");
    for e in Encoding::ALL {
        print!(" {e:?} |");
    }
    println!("\n|---|---|{}", "---|".repeat(Encoding::ALL.len()));
    for (name, changes) in cases {
        let record = ChangeRecord::from_change_set(changes);
        assert_eq!(
            &record.to_change_set().elements(),
            &changes.elements(),
            "lossless"
        );
        print!("| {name} | {} |", changes.elements().len());
        for e in Encoding::ALL {
            let bytes = e.encode(&record);
            assert_eq!(e.decode(&bytes), record, "{e:?} round-trips");
            print!(" {} |", kb(bytes.len()));
        }
        println!();
    }

    // Undo and redo: replay the inverse, then the change set again.
    println!("\n# Undo latency (in memory)\n");
    println!("| Change set | commit | invert + undo | redo |\n|---|---|---|---|");
    let before = project.store.view().property(level, "Elevation").cloned();
    let (inverse, t_undo) = time(|| {
        let inverse = level_move.inverse();
        project.store.replay(&inverse);
        inverse
    });
    assert_eq!(
        project.store.view().property(level, "Elevation").cloned(),
        Some(3.0 * (levels / 2) as f64).map(Into::into)
    );
    let ((), t_redo) = time(|| project.store.replay(&level_move));
    assert_eq!(
        project.store.view().property(level, "Elevation").cloned(),
        before
    );
    let _ = inverse;
    println!(
        "| move a level ({} elements) | {} | {} | {} |",
        level_move.elements().len(),
        ms(commit_level),
        ms(t_undo),
        ms(t_redo)
    );
    let (_, t_undo1) = time(|| project.store.replay(&single.inverse()));
    project.store.replay(&single);
    println!("| 1 property | – | {} | – |", ms(t_undo1));

    // A working day on disk, per encoding: snapshot the state, then append
    // every edit's change set to the log with its state update.
    println!("\n# A working day on disk ({edits} edits)\n");
    println!(
        "| Encoding | initial file | after the day | log bytes | commit p50 | commit p99 | reopen (state) | undo stack (100) | rebuild from log |"
    );
    println!("|---|---|---|---|---|---|---|---|---|");
    for encoding in [
        Encoding::Postcard,
        Encoding::PostcardZstd,
        Encoding::PostcardLz4,
        Encoding::CborZstd,
    ] {
        let path = dir.join(format!("{encoding:?}.hito"));
        let mut rng = Rng::new(0x5eed_1234_abcd_ef01);
        let (mut project, build) = domain::build(levels, &mut rng);
        let mut file = HitoFile::create(&path, encoding);
        let mut log = 0;
        for changes in &build {
            log += file.commit(changes);
        }
        file.db
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .unwrap();
        let initial = file_size(&path);
        let mut times = Vec::new();
        for _ in 0..edits {
            let kind = day_kind(&mut rng);
            let changes = edit(&mut project, &mut rng, kind);
            let (bytes, t) = time(|| file.commit(&changes));
            log += bytes;
            times.push(t);
        }
        file.db
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .unwrap();
        let after = file_size(&path);
        drop(file);
        let file = HitoFile::open(&path, encoding);
        let (loaded, t_open) = time(|| file.load());
        assert_eq!(loaded.elements().count(), project.store.elements().count());
        let (_, t_tail) = time(|| file.tail(100));
        let (rebuilt, t_rebuild) = time(|| file.rebuild());
        assert_eq!(rebuilt.elements().count(), project.store.elements().count());
        for e in project.store.elements() {
            assert_eq!(
                rebuilt.get(e.id()),
                Some(e),
                "replaying the log reproduces the model"
            );
        }
        println!(
            "| {encoding:?} | {:.1} MB | {:.1} MB | {:.1} MB | {} | {} | {} | {} | {} |",
            initial as f64 / 1e6,
            after as f64 / 1e6,
            log as f64 / 1e6,
            ms(percentile(&mut times, 0.5)),
            ms(percentile(&mut times, 0.99)),
            ms(t_open),
            ms(t_tail),
            ms(t_rebuild),
        );
    }
}
