//! Arbitrary bytes as a Part 21 file: index it, read the header, decode
//! every instance. Nothing may panic, hang or run out of memory.
//!
//! Whatever decodes is then written back with the writer and read again, and
//! must come back equal: the reader and the writer agree on every value the
//! fuzzer can make them read.

#![no_main]

use hito_step::{Header, StepReader, StepWriter};

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Ok(r) = StepReader::new(data) else {
        return;
    };
    let mut entities = Vec::new();
    for id in r.ids() {
        let name = r.type_name(id);
        assert!(name.is_some(), "{id} is indexed but has no type name");
        if let Ok(e) = r.get(id) {
            assert_eq!(e.id, id);
            assert_eq!(Some(e.name), name);
            entities.push(e);
        }
    }
    let mut indexed = 0;
    for name in r.type_names() {
        let ids = r.instances_of(name);
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
        indexed += ids.len();
    }
    assert_eq!(indexed, r.len());

    // Write back what decoded, renumbered from #1. The writer refuses names
    // it wouldn't write, such as lowercase ones; those are left out.
    let h = r.header();
    fn strs(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }
    let (description, author, organization) =
        (strs(&h.description), strs(&h.author), strs(&h.organization));
    let mut schema = strs(&h.schema_identifiers);
    if schema.is_empty() {
        schema.push("X");
    }
    let header = Header {
        description: &description,
        implementation_level: &h.implementation_level,
        file_name: &h.file_name,
        time_stamp: &h.time_stamp,
        author: &author,
        organization: &organization,
        preprocessor_version: &h.preprocessor_version,
        originating_system: &h.originating_system,
        authorization: &h.authorization,
        schema_identifiers: &schema,
    };
    let mut w = StepWriter::new(Vec::new(), &header).unwrap();
    let mut written = Vec::new();
    for e in &entities {
        if let Ok(id) = w.add(e.name, &e.attributes) {
            written.push((id, e));
        }
    }
    let file = w.finish().unwrap();

    let again = StepReader::new(&file).unwrap();
    let mut expected = h.clone();
    if expected.schema_identifiers.is_empty() {
        expected.schema_identifiers.push("X".into());
    }
    assert_eq!(again.header(), &expected);
    assert_eq!(again.len(), written.len());
    for (id, e) in written {
        let back = again.get(id).unwrap();
        assert_eq!(back.name, e.name);
        assert_eq!(back.attributes, e.attributes, "{id}, was {}", e.id);
    }
});
