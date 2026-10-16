//! Everything the writer produces reads back equal: the header, every kind
//! of value, and random entities. A fixed xorshift sequence keeps the random
//! part reproducible without a dependency.

use hito_step::{EntityId, Header, StepReader, StepWriter, Value, args};

fn write(header: &Header<'_>, entities: &[(String, Vec<Value>)]) -> Vec<u8> {
    let mut w = StepWriter::new(Vec::new(), header).unwrap();
    for (name, attributes) in entities {
        w.add(name, attributes).unwrap();
    }
    w.finish().unwrap()
}

fn header() -> Header<'static> {
    Header {
        schema_identifiers: &["IFC4"],
        ..Header::default()
    }
}

/// Reads `file` and checks that its entities are `entities`, numbered from #1.
fn assert_reads_back(file: &[u8], entities: &[(String, Vec<Value>)]) {
    let r = StepReader::new(file).unwrap();
    assert_eq!(r.len(), entities.len());
    for (i, (name, attributes)) in entities.iter().enumerate() {
        let id = EntityId::new(i as u64 + 1);
        let e = r.get(id).unwrap();
        assert_eq!(e.id, id);
        assert_eq!(e.name, name);
        assert!(
            same_values(&e.attributes, attributes),
            "{id}: wrote {attributes:?}, read {:?}",
            e.attributes
        );
        assert!(r.instances_of(name).contains(&id));
    }
}

/// `==`, except that reals must have the same bits, so `-0.` stays `-0.`.
fn same_values(a: &[Value], b: &[Value]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
}

fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Real(x), Value::Real(y)) => x.to_bits() == y.to_bits(),
        (Value::List(x), Value::List(y)) => same_values(x, y),
        (Value::Typed(n, x), Value::Typed(m, y)) => n == m && same(x, y),
        _ => a == b,
    }
}

#[test]
fn every_kind_of_value() {
    let entities: Vec<(String, Vec<Value>)> = [
        ("NULL_AND_DERIVED", args![Value::Null, Value::Derived]),
        ("INTEGERS", args![0, -7, 42i64, 7u32, i64::MIN, i64::MAX]),
        (
            "REALS",
            args![3.0, -0.5, -0.0, 1e-9, 2.5e20, 5e-324, f64::MAX],
        ),
        (
            "STRINGS",
            args!["", "Viga V-101", "a'b\\c", "Sección 30×50", "🏗 ñ\n\t"],
        ),
        (
            "ENUMERATIONS",
            args![
                Value::enumeration("NOTDEFINED"),
                Value::enumeration(String::from("USERDEFINED")),
                Value::enumeration("U"),
            ],
        ),
        ("BOOLEANS", args![true, false]),
        (
            "REFERENCES",
            args![
                EntityId::new(1),
                Some(EntityId::new(2)),
                EntityId::new(u64::MAX)
            ],
        ),
        (
            "LISTS",
            args![
                Vec::<Value>::new(),
                vec![vec![1, 2], vec![3]],
                Value::List(args![1, "a", Value::Null]),
            ],
        ),
        (
            "TYPED",
            args![
                Value::typed("IFCLABEL", "Columna C1"),
                Value::typed("IFCPOSITIVELENGTHMEASURE", 0.3),
                Value::List(vec![Value::typed("IFCBOOLEAN", true)]),
                Value::typed("IFCLIST", Value::List(args![1, 2])),
            ],
        ),
        ("NO_ATTRIBUTES", args![]),
        ("OPTIONS", args![None::<f64>, Some(1.0)]),
    ]
    .into_iter()
    .map(|(name, attributes)| (name.to_owned(), attributes))
    .collect();
    assert_reads_back(&write(&header(), &entities), &entities);
}

#[test]
fn the_header() {
    let header = Header {
        description: &["ViewDefinition [ReferenceView_V1.2]", "Option 'x'"],
        implementation_level: "2;1",
        file_name: "Edificio Ñandú.ifc",
        time_stamp: "2026-10-15T12:00:00",
        author: &["Ana Pérez", "José"],
        organization: &["Estudio O'Higgins"],
        preprocessor_version: "HITO 0.0.0",
        originating_system: "HITO",
        authorization: "a\\b",
        schema_identifiers: &["IFC4X3_ADD2", "X"],
    };
    let file = write(&header, &[]);
    let r = StepReader::new(&file).unwrap();
    let h = r.header();
    assert_eq!(h.description, header.description);
    assert_eq!(h.implementation_level, header.implementation_level);
    assert_eq!(h.file_name, header.file_name);
    assert_eq!(h.time_stamp, header.time_stamp);
    assert_eq!(h.author, header.author);
    assert_eq!(h.organization, header.organization);
    assert_eq!(h.preprocessor_version, header.preprocessor_version);
    assert_eq!(h.originating_system, header.originating_system);
    assert_eq!(h.authorization, header.authorization);
    assert_eq!(h.schema_identifiers, header.schema_identifiers);
    assert!(r.is_empty());
}

#[test]
fn empty_header_lists_read_back_empty() {
    let file = write(&header(), &[]);
    let r = StepReader::new(&file).unwrap();
    let h = r.header();
    assert!(h.description.is_empty());
    assert!(h.author.is_empty());
    assert!(h.organization.is_empty());
    assert_eq!(h.schema_identifiers, ["IFC4"]);
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn name(&mut self) -> String {
        const FIRST: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ_";
        const REST: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789";
        let mut name = String::from(char::from(FIRST[self.below(27) as usize]));
        for _ in 0..self.below(12) {
            name.push(char::from(REST[self.below(37) as usize]));
        }
        name
    }

    fn string(&mut self) -> String {
        let pool: Vec<char> = "aZ09 '\\\"$*#().,;=-_/ñÑáé°²€\n\u{7F}\u{FFFF}🏗"
            .chars()
            .collect();
        (0..self.below(16))
            .map(|_| {
                if self.below(4) == 0 {
                    loop {
                        if let Some(c) = char::from_u32(self.below(0x11_0000) as u32) {
                            break c;
                        }
                    }
                } else {
                    pool[self.below(pool.len() as u64) as usize]
                }
            })
            .collect()
    }

    fn value(&mut self, depth: usize) -> Value {
        let kinds = if depth < 4 { 11 } else { 9 };
        match self.below(kinds) {
            0 => Value::Null,
            1 => Value::Derived,
            2 => Value::Integer(self.next() as i64 >> self.below(64)),
            3 => {
                let x = f64::from_bits(self.next());
                Value::Real(if x.is_finite() { x } else { 0.5 })
            }
            4 => Value::String(self.string()),
            5 => {
                // `.T.` and `.F.` read back as booleans.
                let name = self.name();
                if name == "T" || name == "F" {
                    Value::Boolean(name == "T")
                } else {
                    Value::enumeration(name)
                }
            }
            6 => Value::Boolean(self.below(2) == 0),
            7 => Value::Reference(EntityId::new(1 + self.below(u64::MAX))),
            8 => Value::Integer(self.below(1000) as i64),
            9 => Value::List((0..self.below(5)).map(|_| self.value(depth + 1)).collect()),
            _ => Value::typed(self.name(), self.value(depth + 1)),
        }
    }
}

#[test]
fn random_entities_read_back_equal() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let entities: Vec<(String, Vec<Value>)> = (0..20_000)
        .map(|_| {
            let name = rng.name();
            let attributes = (0..rng.below(8)).map(|_| rng.value(0)).collect();
            (name, attributes)
        })
        .collect();
    assert_reads_back(&write(&header(), &entities), &entities);
}
