//! The structural domain, for the first milestone: a structural BIM modeller
//! ([ADR 0011](https://github.com/hito-project/hito/blob/main/docs/decisions/0011-mvp-structural-bim.md)).
//!
//! A domain adds element types to the core's schema system and depends on the
//! core only. The app [registers](register) it at startup:
//!
//! ```
//! use hito_core::SchemaRegistry;
//! use hito_structural::{class, grid, level, material};
//!
//! let mut registry = SchemaRegistry::new();
//! hito_structural::register(&mut registry)?;
//! assert!(registry.class(&class(level::LEVEL)).is_some());
//! assert!(registry.class(&class(grid::GRID)).is_some());
//! assert!(registry.class(&class(material::CONCRETE)).is_some());
//! # Ok::<(), hito_core::SchemaError>(())
//! ```
//!
//! So far it has:
//!
//! - [`level`]s: named elevations that organise the building by floors, and
//!   the elements attached to them
//! - [`grid`]s: labelled lines that lay out the structure in plan
//! - [`material`]s: concrete, with its strength, elastic modulus, density and
//!   display colour
//!
//! Nothing here is specific to a country. Concrete grades and the formulas
//! behind their properties come from regional packs, such as `hito-pack-ar`
//! for CIRSOC 201
//! ([ADR 0008](https://github.com/hito-project/hito/blob/main/docs/decisions/0008-argentina-first-international-by-design.md)).

pub mod grid;
pub mod level;
pub mod material;

use hito_core::{ClassId, Recompute, Rule, Schema, SchemaError, SchemaRegistry, SchemaVersion};

/// The structural schema's name. It prefixes every class name, as in
/// `Structural:Level`, and is stored in files.
pub const SCHEMA: &str = "Structural";

/// The structural schema's version.
pub const VERSION: SchemaVersion = SchemaVersion::new(1, 0, 0);

/// The structural class or relationship type called `name`, such as
/// [`level::LEVEL`].
pub fn class(name: &str) -> ClassId {
    ClassId::new(SCHEMA, name)
}

/// The structural schema: every class and relationship type.
pub fn schema() -> Schema {
    let schema = Schema::new(SCHEMA, VERSION);
    let classes = level::classes()
        .into_iter()
        .chain(grid::classes())
        .chain(material::classes());
    let schema = classes.fold(schema, Schema::class);
    level::relationships()
        .into_iter()
        .fold(schema, Schema::relationship)
}

/// The structural recompute functions: how elements follow what they
/// depend on.
pub fn recomputes() -> Vec<Recompute> {
    level::recomputes()
}

/// The structural validation rules.
pub fn rules() -> Vec<Rule> {
    level::rules()
        .into_iter()
        .chain(grid::rules())
        .chain(material::rules())
        .collect()
}

/// Registers the structural schema, its recompute functions and its rules.
/// It needs only the core schema before it.
pub fn register(registry: &mut SchemaRegistry) -> Result<(), SchemaError> {
    registry.register(schema())?;
    recomputes()
        .into_iter()
        .try_for_each(|r| registry.add_recompute(r))?;
    rules().into_iter().try_for_each(|r| registry.add_rule(r))
}
