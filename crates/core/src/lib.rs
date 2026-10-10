//! The HITO core ([ADR 0004](https://github.com/hito-project/hito/blob/main/docs/decisions/0004-unified-core-ports-and-adapters.md)).
//!
//! The core holds what every discipline shares: the element store, the schema
//! system, relationships, transactions, coordinates and views. It also defines
//! the ports (storage, geometry, solvers) that adapters implement.
//!
//! It never depends on domains, adapters or the UI. Nothing is implemented yet.
