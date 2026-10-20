//! The Argentine regional pack: what HITO takes from the CIRSOC codes
//! ([ADR 0008](https://github.com/hito-project/hito/blob/main/docs/decisions/0008-argentina-first-international-by-design.md)).
//!
//! The core and the domains know nothing about any country. A regional pack
//! adds a country's design-code data and rules on top of them, so adding a
//! country means adding a pack. This one has, so far:
//!
//! - [`concrete`]: the concrete grades of CIRSOC 201-2005 (H-15 to H-60), as
//!   structural materials. **No engineer has reviewed this table yet.**
//!
//! Engineering values live in data files under `data/`, one value per line,
//! each with its source clause, so an engineer can review them. Tests
//! recompute every derived value from the code's formulas, and snapshot
//! tests lock each table, so no change to it goes unnoticed.

pub mod concrete;
