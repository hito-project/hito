//! The writer's output, end to end.

use std::cell::RefCell;
use std::io::{self, Write};
use std::rc::Rc;

use hito_step::{Error, Header, StepWriter, Value, args};

fn header() -> Header<'static> {
    Header {
        description: &["ViewDefinition [ReferenceView_V1.2]"],
        file_name: "Edificio Ñandú.ifc",
        time_stamp: "2026-10-15T12:00:00",
        author: &["Ana Pérez"],
        organization: &["Estudio O'Higgins"],
        preprocessor_version: "HITO 0.0.0",
        originating_system: "HITO",
        schema_identifiers: &["IFC4"],
        ..Header::default()
    }
}

fn write(entities: &[(&str, Vec<Value>)]) -> String {
    let mut w = StepWriter::new(Vec::new(), &header()).unwrap();
    for (name, attributes) in entities {
        w.add(name, attributes).unwrap();
    }
    String::from_utf8(w.finish().unwrap()).unwrap()
}

#[test]
fn whole_file() {
    let file = write(&[
        ("IFCCARTESIANPOINT", args![[0.0, 0.0, 3.5]]),
        (
            "IFCMATERIAL",
            args!["Hormigón H-30", Value::Null, "Hormigón"],
        ),
    ]);
    assert_eq!(
        file,
        "ISO-10303-21;\n\
         HEADER;\n\
         FILE_DESCRIPTION(('ViewDefinition [ReferenceView_V1.2]'),'2;1');\n\
         FILE_NAME('Edificio \\X2\\00D1\\X0\\and\\X2\\00FA\\X0\\.ifc','2026-10-15T12:00:00',\
         ('Ana P\\X2\\00E9\\X0\\rez'),('Estudio O''Higgins'),'HITO 0.0.0','HITO','');\n\
         FILE_SCHEMA(('IFC4'));\n\
         ENDSEC;\n\
         DATA;\n\
         #1=IFCCARTESIANPOINT((0.,0.,3.5));\n\
         #2=IFCMATERIAL('Hormig\\X2\\00F3\\X0\\n H-30',$,'Hormig\\X2\\00F3\\X0\\n');\n\
         ENDSEC;\n\
         END-ISO-10303-21;\n"
    );
    assert!(file.is_ascii());
}

#[test]
fn empty_header_lists_hold_one_empty_string() {
    let header = Header {
        schema_identifiers: &["IFC4X3_ADD2"],
        ..Header::default()
    };
    let file = String::from_utf8(
        StepWriter::new(Vec::new(), &header)
            .unwrap()
            .finish()
            .unwrap(),
    )
    .unwrap();
    assert!(file.contains("FILE_DESCRIPTION((''),'2;1');\n"));
    assert!(file.contains("FILE_NAME('','',(''),(''),'','','');\n"));
    assert!(file.contains("FILE_SCHEMA(('IFC4X3_ADD2'));\n"));
    assert!(file.ends_with("DATA;\nENDSEC;\nEND-ISO-10303-21;\n"));
}

#[test]
fn header_needs_a_schema() {
    let err = StepWriter::new(Vec::new(), &Header::default()).unwrap_err();
    assert!(matches!(err, Error::MissingSchema));
}

#[test]
fn every_kind_of_value() {
    let file = write(&[
        ("NULL_AND_DERIVED", args![Value::Null, Value::Derived]),
        ("INTEGERS", args![0, -7, 42i64, 7u32, i64::MIN, i64::MAX]),
        ("REALS", args![3.0, -0.5, 1e-9, 2.5e20]),
        (
            "STRINGS",
            args!["", "Viga V-101", "a'b\\c", "Sección 30×50"],
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
                hito_step::EntityId::new(1),
                Some(hito_step::EntityId::new(2))
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
            ],
        ),
        ("NO_ATTRIBUTES", args![]),
        ("OPTIONS", args![None::<f64>, Some(1.0)]),
    ]);
    let data: Vec<&str> = file
        .lines()
        .skip_while(|l| *l != "DATA;")
        .skip(1)
        .take_while(|l| *l != "ENDSEC;")
        .collect();
    assert_eq!(
        data,
        [
            "#1=NULL_AND_DERIVED($,*);",
            "#2=INTEGERS(0,-7,42,7,-9223372036854775808,9223372036854775807);",
            "#3=REALS(3.,-0.5,1.E-9,2.5E20);",
            "#4=STRINGS('','Viga V-101','a''b\\\\c','Secci\\X2\\00F3\\X0\\n 30\\X2\\00D7\\X0\\50');",
            "#5=ENUMERATIONS(.NOTDEFINED.,.USERDEFINED.,.U.);",
            "#6=BOOLEANS(.T.,.F.);",
            "#7=REFERENCES(#1,#2);",
            "#8=LISTS((),((1,2),(3)),(1,'a',$));",
            "#9=TYPED(IFCLABEL('Columna C1'),IFCPOSITIVELENGTHMEASURE(0.3),(IFCBOOLEAN(.T.)));",
            "#10=NO_ATTRIBUTES();",
            "#11=OPTIONS($,1.);",
        ]
    );
}

#[test]
fn bad_values_write_nothing_and_keep_the_numbering() {
    let mut w = StepWriter::new(Vec::new(), &header()).unwrap();
    w.add("FIRST", &args![1]).unwrap();
    let bad: [(&str, Vec<Value>); 5] = [
        ("NAN", args![[0.0, f64::NAN]]),
        ("INFINITY", args![Value::typed("X", f64::INFINITY)]),
        ("IfcWall", args![]),
        ("ENUM", args![Value::enumeration("notdefined")]),
        ("TYPED", args![Value::typed("IfcLabel", "x")]),
    ];
    for (name, attributes) in &bad {
        let err = w.add(name, attributes).unwrap_err();
        assert!(
            matches!(err, Error::NonFiniteReal(_) | Error::InvalidKeyword(_)),
            "{err}"
        );
    }
    assert_eq!(w.entity_count(), 1);
    assert_eq!(w.add("SECOND", &args![]).unwrap().get(), 2);
    let file = String::from_utf8(w.finish().unwrap()).unwrap();
    assert!(file.contains("DATA;\n#1=FIRST(1);\n#2=SECOND();\nENDSEC;\n"));
}

/// A sink that records what reaches it and how many writes it took.
#[derive(Clone, Default)]
struct Sink {
    bytes: Rc<RefCell<Vec<u8>>>,
    writes: Rc<RefCell<usize>>,
}

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes.borrow_mut().extend_from_slice(buf);
        *self.writes.borrow_mut() += 1;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn streams_through_a_buffer() {
    let sink = Sink::default();
    let mut w = StepWriter::new(sink.clone(), &header()).unwrap();
    let entities = 20_000;
    for i in 0..entities {
        w.add("IFCCARTESIANPOINT", &args![[f64::from(i), 0.25, -1.5]])
            .unwrap();
    }
    // Most of the file has already reached the sink, before `finish`...
    let streamed = sink.bytes.borrow().len();
    assert!(streamed > 500_000, "only {streamed} bytes streamed");
    // ...in a few large writes rather than one per entity.
    let writes = *sink.writes.borrow();
    assert!(writes < 50, "{writes} writes for {entities} entities");

    w.finish().unwrap();
    let file = String::from_utf8(sink.bytes.borrow().clone()).unwrap();
    assert!(file.contains("#20000=IFCCARTESIANPOINT((19999.,0.25,-1.5));\nENDSEC;\n"));
    assert!(file.ends_with("END-ISO-10303-21;\n"));
}

struct Broken;

impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("disk full"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("disk full"))
    }
}

#[test]
fn io_errors_are_reported() {
    // The header fits in the buffer, so the error shows up on finish.
    let w = StepWriter::new(Broken, &header()).unwrap();
    assert!(matches!(w.finish(), Err(Error::Io(_))));
}
