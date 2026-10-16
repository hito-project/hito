//! A writer and a reader for STEP physical files (ISO 10303-21, "Part 21"),
//! the text encoding of IFC files ([ADR 0016](https://github.com/hito-project/hito/blob/main/docs/decisions/0016-ifc-export.md),
//! [ADR 0022](https://github.com/hito-project/hito/blob/main/docs/decisions/0022-ifc-reader.md)).
//!
//! The crate knows the encoding and nothing about any schema: entity names
//! and attribute order come from the caller, such as the IFC export adapter.
//! Schema mistakes are caught by validating the output, not here.
//!
//! - [`Value`] is one attribute value, and [`args!`] builds an attribute list.
//! - [`StepWriter`] streams a file to any [`std::io::Write`]: the header first,
//!   then one entity at a time, numbered `#1`, `#2`, ... in creation order.
//! - [`StepReader`] indexes a file in one pass, then decodes entities into
//!   the same [`Value`]s on demand. It's written for untrusted input.
//! - [`format_real`] and [`encode_string`] are the value encodings on their own.
//! - [`guid`] compresses a UUID into the 22-character GUID that IFC files use.
//!
//! ```
//! use hito_step::{args, Header, StepReader, StepWriter, Value};
//!
//! let header = Header {
//!     file_name: "obra.ifc",
//!     time_stamp: "2026-10-15T12:00:00",
//!     schema_identifiers: &["IFC4"],
//!     ..Header::default()
//! };
//! let mut w = StepWriter::new(Vec::new(), &header)?;
//! let origin = w.add("IFCCARTESIANPOINT", &args![[0.0, 0.0, 0.0]])?;
//! w.add("IFCAXIS2PLACEMENT3D", &args![origin, Value::Null, Value::Null])?;
//! let file = w.finish()?;
//! let text = std::str::from_utf8(&file).unwrap();
//! assert!(text.contains("#1=IFCCARTESIANPOINT((0.,0.,0.));\n"));
//! assert!(text.contains("#2=IFCAXIS2PLACEMENT3D(#1,$,$);\n"));
//!
//! let r = StepReader::new(&file)?;
//! assert_eq!(r.header().schema_identifiers, ["IFC4"]);
//! let [placement] = r.instances_of("IFCAXIS2PLACEMENT3D") else { panic!() };
//! let e = r.get(*placement)?;
//! assert_eq!(e.attributes, args![origin, Value::Null, Value::Null]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod decode;
mod encode;
pub mod guid;
mod reader;
mod value;
mod writer;

pub use decode::MAX_DEPTH;
pub use encode::{encode_string, format_real, is_keyword};
pub use reader::{Entity, FileHeader, ReadError, ReadErrorKind, StepReader};
pub use value::{EntityId, Value};
pub use writer::{Error, Header, StepWriter};
