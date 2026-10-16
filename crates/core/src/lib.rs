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
//! So far it has:
//!
//! - element identity: [`ElementId`]s, [`Code`]s and [`ExternalSource`]s
//! - the schema system: domains declare a [`Schema`] of classes with typed
//!   properties, and register it in the [`SchemaRegistry`] at startup
//! - the [`ElementStore`], which holds [`Model`]s and [`Element`]s, checks
//!   them against the schemas and resolves type and instance property values
//! - [`Transaction`]s, the only way to change the store: all or nothing,
//!   checked against the domains' validation [`Rule`]s when they commit, and
//!   recorded as a [`ChangeSet`]

mod change;
mod code;
mod element;
mod id;
mod provenance;
mod registry;
mod rule;
mod schema;
mod store;
mod transaction;
mod unit;
mod value;

pub use change::{ChangeSet, Command, ElementChange};
pub use code::{Code, CodeSpec, EmptyCodeError};
pub use element::{Aspect, Element, Model, ModelId, ModelKind, Properties};
pub use id::{ElementId, ParseElementIdError};
pub use provenance::ExternalSource;
pub use registry::{Class, ClassKind, SchemaError, SchemaRegistry};
pub use rule::{Rule, Violation};
pub use schema::{Access, ClassDef, ClassId, PropertyDef, Schema, SchemaVersion};
pub use store::{ElementStore, StoreError};
pub use transaction::Transaction;
pub use unit::Dimension;
pub use value::{DataType, Value};
