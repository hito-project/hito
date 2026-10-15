//! A writer for STEP physical files (ISO 10303-21, "Part 21"), the text
//! encoding of IFC files ([ADR 0016](https://github.com/hito-project/hito/blob/main/docs/decisions/0016-ifc-export.md)).
//!
//! The crate knows the encoding and nothing about any schema: entity names
//! and attribute order come from the caller, such as the IFC export adapter.
//! Schema mistakes are caught by validating the output, not here.
//!
//! - [`Value`] is one attribute value, and [`args!`] builds an attribute list.
//! - [`StepWriter`] streams a file to any [`std::io::Write`]: the header first,
//!   then one entity at a time, numbered `#1`, `#2`, ... in creation order.
//! - [`format_real`] and [`encode_string`] are the value encodings on their own.
//! - [`guid`] compresses a UUID into the 22-character GUID that IFC files use.
//!
//! ```
//! use hito_step::{args, Header, StepWriter, Value};
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
//! let file = String::from_utf8(w.finish()?).unwrap();
//! assert!(file.contains("#1=IFCCARTESIANPOINT((0.,0.,0.));\n"));
//! assert!(file.contains("#2=IFCAXIS2PLACEMENT3D(#1,$,$);\n"));
//! # Ok::<(), hito_step::Error>(())
//! ```

mod encode;
pub mod guid;
mod value;
mod writer;

pub use encode::{encode_string, format_real, is_keyword};
pub use value::{EntityId, Value};
pub use writer::{Error, Header, StepWriter};
