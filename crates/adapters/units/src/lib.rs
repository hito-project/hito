//! Units of measure for people: reads typed quantities and shows stored ones
//! in the user's display units and locale
//! ([ADR 0008](https://github.com/hito-project/hito/blob/main/docs/decisions/0008-argentina-first-international-by-design.md)).
//!
//! The core stores quantities in coherent SI units and knows only their
//! [`Dimension`](hito_core::Dimension)s. Unit names, display preferences and number formats are
//! data held here, outside the core, so a region is a configuration, not
//! code:
//!
//! - [`UnitCatalogue`] lists the units input can name, each a symbol, a
//!   dimension and a factor to SI. [`UnitCatalogue::standard`] has SI, the
//!   metric technical units (kgf, tf) and imperial ones; more can be added.
//! - [`NumberFormat`] writes and reads numbers as a locale does, from Unicode
//!   CLDR data.
//! - [`Units`] puts them together with the user's [`DisplayUnit`]s.
//!
//! ```
//! use hito_core::Dimension;
//! use hito_units::{NumberFormat, UnitCatalogue, Units};
//!
//! let mut units = Units::new(UnitCatalogue::standard(), NumberFormat::new("es-AR")?);
//! units.set_display(Dimension::LENGTH, "m", 2)?;
//!
//! assert_eq!(units.parse("3,5 m + 20 cm", Dimension::LENGTH)?, 3.7);
//! assert_eq!(units.parse("350", Dimension::LENGTH)?, 350.0); // in the display unit
//! assert_eq!(units.parse("25 MPa", Dimension::PRESSURE)?, 25e6);
//! assert_eq!(units.format(3.7, Dimension::LENGTH), "3,70 m");
//! assert_eq!(
//!     units.parse("3 m + 2 kN", Dimension::LENGTH).unwrap_err().to_string(),
//!     "can't add a value in N to a value in m: they measure different things",
//! );
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Units are proportional to SI. Units with an offset, like degrees Celsius,
//! don't fit yet: no M1 property needs an absolute temperature.
//!
//! # Alternatives considered
//!
//! The expression parser is our own, about 450 lines. We checked in October
//! 2026 whether an existing crate should replace it, running this crate's
//! tests against a numbat-backed prototype. The pull request that added this
//! section has the full comparison.
//!
//! - **numbat** 1.24 (MIT OR Apache-2.0) is the closest fit, but it failed 7
//!   of 19 test groups. It reads `1 m 20 cm` and `10' 6"` as products, not
//!   sums. Its built-in catalogue can't be restricted, so unknown symbols
//!   like `parsecs` are accepted. It treats angles as pure numbers and
//!   `0 m` as a plain number. It rounds some constants (psi, lbf) to 7
//!   digits. Typed input reaches a whole language (`random()`, `if`, the
//!   constant `e`). It also adds 39 crates, one of which (`option-ext`,
//!   MPL-2.0) fails `deny.toml`, makes a binary 3.3 MB larger, takes 150 ms
//!   to load its prelude and 1.6 ms per parse. Our ICU4X number reading
//!   would have to stay anyway, and so would a tokenizer for mixed
//!   quantities.
//! - **fend-core** 1.5 (MIT, no dependencies) reads mixed quantities, but it
//!   only returns formatted strings. It gives no value, no dimension and no
//!   error positions.
//! - **rink-core** 0.9 is MPL-2.0, and its units database is forked from GNU
//!   units, which is GPL-3.0-or-later (ADR 0009 rules out both).
//! - **uom** and **dimensioned** check dimensions at compile time. HITO's
//!   schemas declare dimensions at run time.

mod catalogue;
mod error;
mod expr;
mod number;
mod units;

pub use catalogue::{STANDARD, Unit, UnitCatalogue};
pub use error::ParseError;
pub use number::{InvalidLocale, NumberFormat};
pub use units::{DisplayUnit, Units};
