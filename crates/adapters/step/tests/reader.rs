//! The reader on hand-written files: what real exporters write, and what a
//! damaged or hostile file can hold.

use hito_step::{EntityId, ReadError, ReadErrorKind, StepReader, Value};

fn id(n: u64) -> EntityId {
    EntityId::new(n)
}

/// Out of order, with comments, a complex instance, and the escapes a Revit
/// or ArchiCAD export uses.
const SAMPLE: &str = "ISO-10303-21;
HEADER;
/* written by hand */
FILE_DESCRIPTION(('ViewDefinition [ReferenceView_V1.2]','Option [x]'),'2;1');
FILE_NAME('Edificio \\X2\\00D1\\X0\\and\\X2\\00FA\\X0\\.ifc','2026-10-15T12:00:00',('Ana P\\X2\\00E9\\X0\\rez'),
  ('Estudio O''Higgins'),'pre 1.0','Autodesk Revit 2025 (ENU)','');
FILE_SCHEMA(('IFC4'));
ENDSEC;
DATA;
#1=IFCCARTESIANPOINT((0.,1.5,-2.E-3));
/* a comment ; with a semicolon and a ' quote */
#3= IFCWALL ('2O2Fr$t4X7Zf8NOew3FLOH',$,'Hormig\\X2\\00F3\\X0\\n ''A''; x',$,$,#2,*,.T.,
  IFCLABEL('x'),.NOTDEFINED.,(1,-2,+3),.U.);
#2=IFCLOCALPLACEMENT($,#1);
#10=(IFCA(1)IFCB(#1,'s'));
#7=IFCCARTESIANPOINT((1.,2.,3.));
ENDSEC;
END-ISO-10303-21;
";

#[test]
fn indexes_every_instance_by_id_and_type() {
    let r = StepReader::new(SAMPLE.as_bytes()).unwrap();
    assert_eq!(r.len(), 5);
    assert!(!r.is_empty());
    assert_eq!(
        r.ids().collect::<Vec<_>>(),
        [id(1), id(2), id(3), id(7), id(10)]
    );
    assert_eq!(r.type_name(id(2)), Some("IFCLOCALPLACEMENT"));
    assert_eq!(r.type_name(id(3)), Some("IFCWALL"));
    assert_eq!(r.type_name(id(10)), Some(""));
    assert_eq!(r.type_name(id(4)), None);
    assert_eq!(r.instances_of("IFCCARTESIANPOINT"), [id(1), id(7)]);
    assert_eq!(r.instances_of("IFCWALL"), [id(3)]);
    assert_eq!(r.instances_of("IFCWINDOW"), []);
    assert_eq!(
        r.type_names().collect::<Vec<_>>(),
        ["", "IFCCARTESIANPOINT", "IFCLOCALPLACEMENT", "IFCWALL"]
    );
}

#[test]
fn reads_the_header() {
    let r = StepReader::new(SAMPLE.as_bytes()).unwrap();
    let h = r.header();
    assert_eq!(
        h.description,
        ["ViewDefinition [ReferenceView_V1.2]", "Option [x]"]
    );
    assert_eq!(h.implementation_level, "2;1");
    assert_eq!(h.file_name, "Edificio Ñandú.ifc");
    assert_eq!(h.time_stamp, "2026-10-15T12:00:00");
    assert_eq!(h.author, ["Ana Pérez"]);
    assert_eq!(h.organization, ["Estudio O'Higgins"]);
    assert_eq!(h.preprocessor_version, "pre 1.0");
    assert_eq!(h.originating_system, "Autodesk Revit 2025 (ENU)");
    assert_eq!(h.authorization, "");
    assert_eq!(h.schema_identifiers, ["IFC4"]);
}

#[test]
fn decodes_attributes_on_demand() {
    let r = StepReader::new(SAMPLE.as_bytes()).unwrap();
    let p = r.get(id(1)).unwrap();
    assert_eq!(p.id, id(1));
    assert_eq!(p.name, "IFCCARTESIANPOINT");
    assert_eq!(
        p.attributes,
        [Value::List(vec![
            Value::Real(0.0),
            Value::Real(1.5),
            Value::Real(-0.002)
        ])]
    );

    let w = r.get(id(3)).unwrap();
    assert_eq!(w.name, "IFCWALL");
    assert_eq!(
        w.attributes,
        [
            Value::from("2O2Fr$t4X7Zf8NOew3FLOH"),
            Value::Null,
            Value::from("Hormigón 'A'; x"),
            Value::Null,
            Value::Null,
            Value::Reference(id(2)),
            Value::Derived,
            Value::Boolean(true),
            Value::typed("IFCLABEL", "x"),
            Value::enumeration("NOTDEFINED"),
            Value::List(vec![
                Value::Integer(1),
                Value::Integer(-2),
                Value::Integer(3)
            ]),
            Value::enumeration("U"),
        ]
    );
}

#[test]
fn complex_instances_keep_their_parts() {
    let r = StepReader::new(SAMPLE.as_bytes()).unwrap();
    let c = r.get(id(10)).unwrap();
    assert_eq!(c.name, "");
    assert_eq!(
        c.attributes,
        [
            Value::typed("IFCA", Value::List(vec![Value::Integer(1)])),
            Value::typed(
                "IFCB",
                Value::List(vec![Value::Reference(id(1)), Value::from("s")])
            ),
        ]
    );
    assert_eq!(r.instances_of(""), [id(10)]);
}

#[test]
fn missing_instances_are_an_error() {
    let r = StepReader::new(SAMPLE.as_bytes()).unwrap();
    let err = r.get(id(4)).unwrap_err();
    assert_eq!(err.kind(), &ReadErrorKind::NotFound);
    assert_eq!(err.entity(), Some(id(4)));
    assert_eq!(err.line(), None);
    assert_eq!(err.to_string(), "#4: no such instance in the file");
}

/// A file with `data` as its data section.
fn file(data: &str) -> String {
    format!(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\n\
         FILE_NAME('','',(''),(''),'','','');\nFILE_SCHEMA(('IFC4'));\nENDSEC;\n\
         DATA;\n{data}ENDSEC;\nEND-ISO-10303-21;\n"
    )
}

/// The first attribute of `#1` in a file whose data section is `#1=X(<value>);`.
fn read_value(value: &str) -> Result<Value, ReadError> {
    let f = file(&format!("#1=X({value});\n"));
    let r = StepReader::new(f.as_bytes())?;
    let mut e = r.get(id(1))?;
    assert_eq!(e.attributes.len(), 1, "{value}");
    Ok(e.attributes.remove(0))
}

#[test]
fn numbers() {
    let read = |s| read_value(s).unwrap();
    assert_eq!(read("0"), Value::Integer(0));
    assert_eq!(read("-7"), Value::Integer(-7));
    assert_eq!(read("+7"), Value::Integer(7));
    assert_eq!(read("-9223372036854775808"), Value::Integer(i64::MIN));
    assert_eq!(read("9223372036854775807"), Value::Integer(i64::MAX));
    assert_eq!(read("3."), Value::Real(3.0));
    assert_eq!(read("-0."), Value::Real(-0.0));
    assert_eq!(read("1.E-5"), Value::Real(1e-5));
    assert_eq!(read("1.5e+3"), Value::Real(1500.0));
    assert_eq!(read("2.5E20"), Value::Real(2.5e20));
    assert_eq!(read("1E3"), Value::Real(1000.0));
    let Value::Real(neg_zero) = read("-0.") else {
        unreachable!()
    };
    assert!(neg_zero.is_sign_negative());

    for (bad, kind) in [
        ("9223372036854775808", ReadErrorKind::NumberOutOfRange),
        ("-9223372036854775809", ReadErrorKind::NumberOutOfRange),
        ("1.E400", ReadErrorKind::NumberOutOfRange),
        ("-1.E400", ReadErrorKind::NumberOutOfRange),
        ("1.2.3", ReadErrorKind::Syntax("a malformed number")),
        ("-", ReadErrorKind::Syntax("a malformed number")),
        ("1.E", ReadErrorKind::Syntax("a malformed number")),
    ] {
        assert_eq!(read_value(bad).unwrap_err().kind(), &kind, "{bad}");
    }
}

#[test]
fn strings_and_their_escapes() {
    let read = |s| match read_value(s).unwrap() {
        Value::String(s) => s,
        other => panic!("{other:?}"),
    };
    assert_eq!(read("''"), "");
    assert_eq!(read("'O''Higgins'"), "O'Higgins");
    assert_eq!(read("'a\\\\b'"), "a\\b");
    assert_eq!(read("'Hormig\\X2\\00F3\\X0\\n'"), "Hormigón");
    assert_eq!(read("'\\X2\\00F100f3\\X0\\'"), "ñó");
    assert_eq!(read("'\\X4\\0001F3D7\\X0\\'"), "🏗");
    // Some exporters write characters beyond the BMP as UTF-16 surrogates.
    assert_eq!(read("'\\X2\\D83CDFD7\\X0\\'"), "🏗");
    assert_eq!(read("'\\X2\\\\X0\\'"), "");
    // 8-bit escapes in ISO 8859-1.
    assert_eq!(read("'Hormig\\X\\F3n'"), "Hormigón");
    assert_eq!(read("'Hormig\\S\\sn'"), "Hormigón");
    assert_eq!(read("'\\S\\''x'"), "§x");
    // Code page switches are accepted; \S\ stays ISO 8859-1.
    assert_eq!(read("'\\PA\\caf\\S\\i'"), "café");
    // Exporters that write UTF-8 directly, and stray backslashes.
    assert_eq!(read("'Hormigón'"), "Hormigón");
    assert_eq!(read("'C:\\Users\\x'"), "C:\\Users\\x");
    assert_eq!(read("'line\nbreak'"), "line\nbreak");

    for bad in [
        "'\\X2\\00F\\X0\\'",
        "'\\X2\\00G3\\X0\\'",
        "'\\X2\\00F3'",
        "'\\X4\\00110000\\X0\\'",
        "'\\X2\\D83C\\X0\\'",
        "'\\X\\G1'",
        "'\\X\\F'",
    ] {
        let err = read_value(bad).unwrap_err();
        assert_eq!(
            err.kind(),
            &ReadErrorKind::Syntax("a malformed string escape"),
            "{bad}"
        );
    }
}

#[test]
fn enumerations_and_booleans() {
    assert_eq!(read_value(".T.").unwrap(), Value::Boolean(true));
    assert_eq!(read_value(".F.").unwrap(), Value::Boolean(false));
    assert_eq!(read_value(".U.").unwrap(), Value::enumeration("U"));
    assert_eq!(
        read_value(".NOTDEFINED.").unwrap(),
        Value::enumeration("NOTDEFINED")
    );
    for bad in [".T", "..", ".A B.", ".T,"] {
        assert!(read_value(bad).is_err(), "{bad}");
    }
}

#[test]
fn whitespace_and_comments_between_tokens() {
    let v = read_value(" ( 1 , /* x */ #2 ,\r\n\t IFCLABEL ( 'a' ) ) ").unwrap();
    assert_eq!(
        v,
        Value::List(vec![
            Value::Integer(1),
            Value::Reference(id(2)),
            Value::typed("IFCLABEL", "a"),
        ])
    );
}

/// What is wrong, on which line, in which entity.
type Where = (ReadErrorKind, usize, Option<u64>);

/// The error of reading `data` as a whole file, with its line and entity.
fn error_of(data: &str) -> Where {
    let e = match StepReader::new(data.as_bytes()) {
        Err(e) => e,
        Ok(r) => r
            .ids()
            .find_map(|i| r.get(i).err())
            .unwrap_or_else(|| panic!("no error in {data:?}")),
    };
    (
        e.kind().clone(),
        e.line().unwrap(),
        e.entity().map(EntityId::get),
    )
}

#[test]
fn errors_give_the_line_and_entity() {
    use ReadErrorKind::*;
    let cases: &[(&str, Where)] = &[
        ("", (Syntax("expected ISO-10303-21;"), 1, None)),
        ("PK\x03\x04", (Syntax("expected ISO-10303-21;"), 1, None)),
        (
            "ISO-10303-21;\nHEADER;\nFILE_NAME('a);\n",
            (Unterminated("string"), 3, None),
        ),
        (
            &file("#1=X(1);\n#2=Y('a',\n'b' 3);\n"),
            (Syntax("expected `,` or `)`"), 10, Some(2)),
        ),
        (
            &file("#1=X(1);\n#2=Y('abc);\n"),
            (Unterminated("string"), 9, Some(2)),
        ),
        (&file("#1=X(1)\n"), (Syntax("expected `;`"), 9, Some(1))),
        (
            &file("#0=X();\n"),
            (Syntax("instance names start at #1"), 8, None),
        ),
        (
            &file("#1=X(#0);\n"),
            (Syntax("instance names start at #1"), 8, Some(1)),
        ),
        (
            &file("#99999999999999999999=X();\n"),
            (NumberOutOfRange, 8, None),
        ),
        (&file("#1 X();\n"), (Syntax("expected `=`"), 8, Some(1))),
        (
            &file("1=X();\n"),
            (Syntax("expected `#` or ENDSEC"), 8, None),
        ),
        (
            &file("#1=X();\n#2=Y();\n#1=Z();\n"),
            (Duplicate { first_line: 8 }, 10, Some(1)),
        ),
        (
            &file("#5=X();\n#2=Y();\n#5=Z();\n"),
            (Duplicate { first_line: 8 }, 10, Some(5)),
        ),
        (
            &file("#1=X(\"0FF\");\n"),
            (Unsupported("binary values"), 8, Some(1)),
        ),
        (
            &file("#1=X(@2);\n"),
            (Syntax("expected a value"), 8, Some(1)),
        ),
        (
            &file("#1=X(IFCLABEL);\n"),
            (Syntax("expected `(`"), 8, Some(1)),
        ),
        (
            &file("#1=X(IFCLABEL(1,2));\n"),
            (Syntax("expected `)`"), 8, Some(1)),
        ),
        (
            &file("#1=X();\n#2=Y(/* open"),
            (Unterminated("comment"), 9, Some(2)),
        ),
        (
            "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=X();\nENDSEC;\n",
            (Syntax("expected DATA or END-ISO-10303-21"), 7, None),
        ),
        (
            "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=X();\n",
            (Syntax("expected `#` or ENDSEC"), 6, None),
        ),
    ];
    for (data, expected) in cases {
        assert_eq!(&error_of(data), expected, "{data:?}");
    }
}

#[test]
fn error_messages_name_the_line_and_entity() {
    let f = file("#1=X(1);\n#2=Y('abc);\n");
    let r = StepReader::new(f.as_bytes());
    let msg = r.err().unwrap().to_string();
    assert_eq!(msg, "line 9, #2: unterminated string");
}

#[test]
fn nesting_is_bounded() {
    let depth = hito_step::MAX_DEPTH;
    let nested = |n: usize| format!("{}1{}", "(".repeat(n), ")".repeat(n));
    // The attribute list itself is one level.
    assert!(read_value(&nested(depth - 1)).is_ok());
    let err = read_value(&nested(depth)).unwrap_err();
    assert_eq!(err.kind(), &ReadErrorKind::TooDeep);
    // Typed values count too, and a huge depth is no different.
    let typed = format!("{}1{}", "A(".repeat(depth), ")".repeat(depth));
    assert_eq!(
        read_value(&typed).unwrap_err().kind(),
        &ReadErrorKind::TooDeep
    );
    let deep = nested(1_000_000);
    assert_eq!(
        read_value(&deep).unwrap_err().kind(),
        &ReadErrorKind::TooDeep
    );
}

#[test]
fn large_numbers_allocate_nothing() {
    // An instance name or a reference near u64::MAX is just a number: nothing
    // is allocated or indexed in proportion to it.
    let f = file("#18446744073709551615=X(#18446744073709551614,(1,2));\n");
    let r = StepReader::new(f.as_bytes()).unwrap();
    let e = r.get(id(u64::MAX)).unwrap();
    assert_eq!(e.attributes[0], Value::Reference(id(u64::MAX - 1)));
}

#[test]
fn later_editions_and_odd_layouts() {
    // DATA with parameters and several DATA sections (Part 21 edition 3),
    // header records on one line, and lowercase names.
    let f = "ISO-10303-21;HEADER;FILE_DESCRIPTION((''),'3;1');FILE_NAME('','',(''),(''),'','','');\
             FILE_SCHEMA(('IFC4'));ENDSEC;DATA('a',('IFC4'));#1=X();ENDSEC;DATA;#2=ifcWall();ENDSEC;\
             END-ISO-10303-21;\ntrailing bytes are ignored";
    let r = StepReader::new(f.as_bytes()).unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!(r.type_name(id(2)), Some("ifcWall"));
    assert_eq!(r.header().implementation_level, "3;1");
}

#[test]
fn a_missing_header_record_reads_as_empty() {
    let f = "ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('IFC2X3'));\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n";
    let r = StepReader::new(f.as_bytes()).unwrap();
    assert!(r.is_empty());
    assert_eq!(r.header().schema_identifiers, ["IFC2X3"]);
    assert_eq!(r.header().file_name, "");
    assert!(r.header().author.is_empty());
}

/// Every prefix of a file, and every single-byte change to it, reads without
/// panicking. The fuzz target goes much further; this keeps the basics in
/// `cargo test`.
#[test]
fn damaged_files_never_panic() {
    let bytes = SAMPLE.as_bytes();
    let read_all = |data: &[u8]| {
        if let Ok(r) = StepReader::new(data) {
            for i in r.ids() {
                let _ = r.get(i);
                let _ = r.type_name(i);
            }
        }
    };
    for end in 0..bytes.len() {
        read_all(&bytes[..end]);
    }
    let mut copy = bytes.to_vec();
    for i in 0..copy.len() {
        let original = copy[i];
        for b in [
            b'\'', b'(', b')', b';', b'#', b'\\', b'/', b'*', b'"', b'.', 0, 0xFF,
        ] {
            copy[i] = b;
            read_all(&copy);
        }
        copy[i] = original;
    }
}
