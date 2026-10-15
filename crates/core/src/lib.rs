//! The HITO core ([ADR 0004](https://github.com/hito-project/hito/blob/main/docs/decisions/0004-unified-core-ports-and-adapters.md)).
//!
//! The core holds what every discipline shares: the element store, the schema
//! system, relationships, transactions, coordinates and views. It also defines
//! the ports (storage, geometry, solvers) that adapters implement.
//!
//! It never depends on domains, adapters or the UI. The design of the element
//! store and schemas is in
//! [element-model.md](https://github.com/hito-project/hito/blob/main/docs/architecture/element-model.md).
//!
//! So far it has element identity: [`ElementId`]s, [`Code`]s and
//! [`ExternalSource`]s.

mod code;
mod id;
mod provenance;

pub use code::{Code, CodeSpec, EmptyCodeError};
pub use id::{ElementId, ParseElementIdError};
pub use provenance::ExternalSource;
