//! The storage adapter: reads and writes `.hito` files, which are SQLite
//! databases ([ADR 0013](https://github.com/hito-project/hito/blob/main/docs/decisions/0013-storage-engine-and-file-format.md)).
//!
//! It implements the core's storage port. So far it has the
//! [`ChangeRecord`]: a committed [`ChangeSet`](hito_core::ChangeSet) as the
//! file's history stores it
//! ([ADR 0024](https://github.com/hito-project/hito/blob/main/docs/decisions/0024-model-history.md)).
//! Reading and writing files comes with #47.

mod record;

pub use record::{ChangeRecord, DecodeError, RecordId, Target, Timestamp};
