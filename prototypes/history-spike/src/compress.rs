//! Compression techniques for the history and the state, compared on a
//! year's file: per-row zstd, trained dictionaries, packing, long-range
//! windows, a columnar layout with shuffled floats, and content-addressed
//! element blobs.

use std::collections::HashMap;
use std::time::Instant;

use rusqlite::Connection;

use crate::record::{ChangeRecord, ElementChangeRec, ElementRec, FieldRec, ValueRec};

fn mb(bytes: usize) -> String {
    format!("{:.1} MB", bytes as f64 / 1e6)
}

struct Row {
    name: String,
    bytes: usize,
    decode_ms: f64,
    note: &'static str,
}

/// Records split into streams of similar data, each compressed on its own:
/// structure, IDs, names, floats (bytes shuffled so exponents and high
/// mantissa bytes line up, as Blosc does), texts and whole elements.
#[derive(Default)]
struct Columns {
    structure: Vec<u8>,
    ids: Vec<u8>,
    names: Vec<u8>,
    floats: Vec<f64>,
    texts: Vec<u8>,
    elements: Vec<u8>,
    name_index: HashMap<String, u32>,
    /// Store a changed number as its XOR with the value before (as the
    /// Gorilla time-series format does): small edits leave mostly zeros.
    xor: bool,
}

impl Columns {
    fn varint(out: &mut Vec<u8>, mut v: u64) {
        while v >= 0x80 {
            out.push(v as u8 | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
    }

    fn name(&mut self, name: &str) {
        let next = self.name_index.len() as u32;
        let index = *self.name_index.entry(name.to_owned()).or_insert_with(|| {
            self.names.extend_from_slice(name.as_bytes());
            self.names.push(0);
            next
        });
        Self::varint(&mut self.structure, index as u64);
    }

    fn value(&mut self, value: &Option<ValueRec>) {
        match value {
            None => self.structure.push(0),
            Some(ValueRec::Boolean(b)) => self.structure.push(1 + *b as u8),
            Some(ValueRec::Integer(i)) => {
                self.structure.push(3);
                Self::varint(&mut self.structure, *i as u64);
            }
            Some(ValueRec::Real(x)) => {
                self.structure.push(4);
                self.floats.push(*x);
            }
            Some(ValueRec::Text(s)) => {
                self.structure.push(5);
                self.texts.extend_from_slice(s.as_bytes());
                self.texts.push(0);
            }
        }
    }

    fn element(&mut self, e: &ElementRec, blobs: Option<&mut HashMap<blake3::Hash, u32>>) {
        let bytes = postcard::to_stdvec(e).unwrap();
        if let Some(blobs) = blobs {
            let hash = blake3::hash(&bytes);
            let next = blobs.len() as u32;
            let mut new = false;
            let index = *blobs.entry(hash).or_insert_with(|| {
                new = true;
                next
            });
            if !new {
                // Seen before: refer to it by number.
                self.structure.push(1);
                Self::varint(&mut self.structure, index as u64);
                return;
            }
            self.structure.push(0);
        }
        self.elements.extend_from_slice(&bytes);
    }

    fn push(&mut self, record: &ChangeRecord, mut blobs: Option<&mut HashMap<blake3::Hash, u32>>) {
        Self::varint(&mut self.structure, record.elements.len() as u64);
        for change in &record.elements {
            match change {
                ElementChangeRec::Created(e) => {
                    self.structure.push(0);
                    self.element(e, blobs.as_deref_mut());
                }
                ElementChangeRec::Deleted(e) => {
                    self.structure.push(1);
                    self.element(e, blobs.as_deref_mut());
                }
                ElementChangeRec::Modified(id, fields) => {
                    self.structure.push(2);
                    self.ids.extend_from_slice(id);
                    Self::varint(&mut self.structure, fields.len() as u64);
                    for field in fields {
                        match field {
                            FieldRec::Property(name, before, after) => {
                                self.name(name);
                                self.value(before);
                                match (before, after) {
                                    (Some(ValueRec::Real(b)), Some(ValueRec::Real(a)))
                                        if self.xor =>
                                    {
                                        self.structure.push(6);
                                        self.floats.push(f64::from_bits(a.to_bits() ^ b.to_bits()));
                                    }
                                    _ => self.value(after),
                                }
                            }
                            other => {
                                // Rare: type, code, source and aspects.
                                self.structure.push(0xff);
                                self.elements.extend(postcard::to_stdvec(other).unwrap());
                            }
                        }
                    }
                }
            }
        }
        let rest = (&record.command, &record.models, &record.relationships);
        self.elements.extend(postcard::to_stdvec(&rest).unwrap());
    }

    /// Each stream compressed alone; returns the total.
    fn compressed(&self, level: i32) -> usize {
        let mut shuffled = vec![0u8; self.floats.len() * 8];
        for (i, x) in self.floats.iter().enumerate() {
            for (b, byte) in x.to_le_bytes().iter().enumerate() {
                shuffled[b * self.floats.len() + i] = *byte;
            }
        }
        [
            &self.structure,
            &self.ids,
            &self.names,
            &shuffled,
            &self.texts,
            &self.elements,
        ]
        .iter()
        .map(|s| zstd::encode_all(&s[..], level).unwrap().len())
        .sum()
    }
}

fn zstd_long(data: &[u8], level: i32) -> Vec<u8> {
    let mut encoder = zstd::stream::Encoder::new(Vec::new(), level).unwrap();
    encoder.long_distance_matching(true).unwrap();
    encoder.window_log(27).unwrap();
    std::io::Write::write_all(&mut encoder, data).unwrap();
    encoder.finish().unwrap()
}

pub fn study(path: &std::path::Path) {
    let db = Connection::open(path).unwrap();
    let raw: Vec<Vec<u8>> = {
        let mut stmt = db
            .prepare("SELECT payload FROM change ORDER BY seq")
            .unwrap();
        stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))
            .unwrap()
            .map(|p| zstd::decode_all(&p.unwrap()[..]).unwrap())
            .collect()
    };
    let state: Vec<Vec<u8>> = {
        let mut stmt = db.prepare("SELECT record FROM element").unwrap();
        stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let records: Vec<ChangeRecord> = raw
        .iter()
        .map(|r| postcard::from_bytes(r).unwrap())
        .collect();
    let raw_total: usize = raw.iter().map(Vec::len).sum();
    let cap = raw.iter().map(Vec::len).max().unwrap_or(0);
    let mut rows = Vec::new();

    let mut measure =
        |name: String, note, encode: &dyn Fn() -> Vec<Vec<u8>>, decode: &dyn Fn(&[Vec<u8>])| {
            let blobs = encode();
            let start = Instant::now();
            decode(&blobs);
            rows.push(Row {
                name,
                bytes: blobs.iter().map(Vec::len).sum(),
                decode_ms: start.elapsed().as_secs_f64() * 1e3,
                note,
            });
        };

    measure("uncompressed postcard".into(), "", &|| raw.clone(), &|_| {});
    for level in [3, 19] {
        measure(
            format!("each change set alone, zstd {level}"),
            "what a log row holds today",
            &|| {
                raw.iter()
                    .map(|r| zstd::encode_all(&r[..], level).unwrap())
                    .collect()
            },
            &|b| {
                b.iter()
                    .for_each(|x| drop(zstd::decode_all(&x[..]).unwrap()))
            },
        );
    }
    let sample: Vec<&[u8]> = raw
        .iter()
        .step_by(raw.len().max(1) / 20_000 + 1)
        .map(|r| &r[..])
        .collect();
    let dict = zstd::dict::from_samples(&sample, 112 * 1024).unwrap();
    for level in [3, 19] {
        let dict = &dict;
        measure(
            format!("each alone, zstd {level} + trained dictionary"),
            "dictionary (112 KB) stored once in the file",
            &|| {
                let mut c = zstd::bulk::Compressor::with_dictionary(level, dict).unwrap();
                raw.iter().map(|r| c.compress(r).unwrap()).collect()
            },
            &|b| {
                let mut d = zstd::bulk::Decompressor::with_dictionary(dict).unwrap();
                b.iter().for_each(|x| drop(d.decompress(x, cap).unwrap()));
            },
        );
    }
    let pack = |chunk: &[Vec<u8>]| {
        let mut batch = Vec::new();
        for r in chunk {
            batch.extend_from_slice(&(r.len() as u32).to_le_bytes());
            batch.extend_from_slice(r);
        }
        batch
    };
    for (size, level, long) in [
        (1000, 19, false),
        (10_000, 19, false),
        (10_000, 19, true),
        (100_000, 22, true),
    ] {
        let name = format!(
            "packed by {size}, zstd {level}{}",
            if long { " + long window" } else { "" }
        );
        measure(
            name,
            "old history only; recent rows stay single",
            &|| {
                raw.chunks(size)
                    .map(|c| {
                        if long {
                            zstd_long(&pack(c), level)
                        } else {
                            zstd::encode_all(&pack(c)[..], level).unwrap()
                        }
                    })
                    .collect()
            },
            &|b| {
                b.iter().for_each(|x| {
                    let mut d = zstd::stream::Decoder::new(&x[..]).unwrap();
                    d.window_log_max(27).unwrap();
                    std::io::copy(&mut d, &mut std::io::sink()).unwrap();
                })
            },
        );
    }
    // Other compressors on the same packs.
    for size in [10_000, 100_000] {
        measure(
            format!("packed by {size}, brotli 11 (window 2^24)"),
            "pure Rust (brotli crate)",
            &|| {
                raw.chunks(size)
                    .map(|c| {
                        let mut out = Vec::new();
                        let mut w = brotli::CompressorWriter::new(&mut out, 1 << 16, 11, 24);
                        std::io::Write::write_all(&mut w, &pack(c)).unwrap();
                        drop(w);
                        out
                    })
                    .collect()
            },
            &|b| {
                b.iter().for_each(|x| {
                    let mut d = brotli::Decompressor::new(&x[..], 1 << 16);
                    std::io::copy(&mut d, &mut std::io::sink()).unwrap();
                })
            },
        );
        measure(
            format!("packed by {size}, xz 9 extreme"),
            "liblzma (C)",
            &|| {
                raw.chunks(size)
                    .map(|c| {
                        let mut w = xz2::write::XzEncoder::new(Vec::new(), 9 | 0x8000_0000);
                        std::io::Write::write_all(&mut w, &pack(c)).unwrap();
                        w.finish().unwrap()
                    })
                    .collect()
            },
            &|b| {
                b.iter().for_each(|x| {
                    let mut d = xz2::read::XzDecoder::new(&x[..]);
                    std::io::copy(&mut d, &mut std::io::sink()).unwrap();
                })
            },
        );
    }
    for (blobs, xor) in [(false, false), (true, false), (true, true)] {
        let name = format!(
            "columnar by 10,000, shuffled floats, zstd 19{}{}",
            if blobs {
                " + content-addressed elements"
            } else {
                ""
            },
            if xor {
                " + numbers as XOR with before"
            } else {
                ""
            }
        );
        let note = if blobs {
            "a deleted element that existed before is a reference"
        } else {
            ""
        };
        let start = Instant::now();
        let mut seen = HashMap::new();
        let bytes: usize = records
            .chunks(10_000)
            .map(|chunk| {
                let mut columns = Columns {
                    xor,
                    ..Columns::default()
                };
                for r in chunk {
                    columns.push(r, blobs.then_some(&mut seen));
                }
                columns.compressed(19)
            })
            .sum();
        let _ = start;
        rows.push(Row {
            name,
            bytes,
            decode_ms: f64::NAN,
            note,
        });
    }

    println!("# Compression study\n");
    println!(
        "{} change sets, {} uncompressed. State: {} elements, {}.\n",
        raw.len(),
        mb(raw_total),
        state.len(),
        mb(state.iter().map(Vec::len).sum())
    );
    println!("| History stored as | size | vs raw | decode all | note |\n|---|---|---|---|---|");
    for row in &rows {
        let decode = if row.decode_ms.is_nan() {
            "–".into()
        } else {
            format!("{:.0} ms", row.decode_ms)
        };
        println!(
            "| {} | {} | {:.1}× | {} | {} |",
            row.name,
            mb(row.bytes),
            raw_total as f64 / row.bytes as f64,
            decode,
            row.note
        );
    }

    // The state: one row per element.
    let state_raw: usize = state.iter().map(Vec::len).sum();
    let sample: Vec<&[u8]> = state
        .iter()
        .step_by(state.len() / 20_000 + 1)
        .map(|r| &r[..])
        .collect();
    let dict = zstd::dict::from_samples(&sample, 64 * 1024).unwrap();
    let mut c = zstd::bulk::Compressor::with_dictionary(3, &dict).unwrap();
    let start = Instant::now();
    let with_dict: Vec<_> = state.iter().map(|r| c.compress(r).unwrap()).collect();
    let t_c = start.elapsed();
    let mut d = zstd::bulk::Decompressor::with_dictionary(&dict).unwrap();
    let start = Instant::now();
    with_dict
        .iter()
        .for_each(|x| drop(d.decompress(x, 1 << 20).unwrap()));
    let t_d = start.elapsed();
    let plain: usize = state
        .iter()
        .map(|r| zstd::encode_all(&r[..], 3).unwrap().len())
        .sum();
    println!("\n| State stored as | size | vs raw |\n|---|---|---|");
    println!(
        "| one postcard row per element | {} | 1.0× |",
        mb(state_raw)
    );
    println!(
        "| each row zstd 3 | {} | {:.1}× |",
        mb(plain),
        state_raw as f64 / plain as f64
    );
    let dict_total: usize = with_dict.iter().map(Vec::len).sum::<usize>() + dict.len();
    println!(
        "| each row zstd 3 + trained dictionary (compress {:.0} ms, decompress {:.0} ms) | {} | {:.1}× |",
        t_c.as_secs_f64() * 1e3,
        t_d.as_secs_f64() * 1e3,
        mb(dict_total),
        state_raw as f64 / dict_total as f64
    );
    let mut all = Vec::new();
    state.iter().for_each(|r| all.extend_from_slice(r));
    println!(
        "| all rows as one zstd 19 stream (reference: a zipped file) | {} | {:.1}× |",
        mb(zstd::encode_all(&all[..], 19).unwrap().len()),
        state_raw as f64 / zstd::encode_all(&all[..], 19).unwrap().len() as f64
    );
}
