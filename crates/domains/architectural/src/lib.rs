//! The architectural domain: walls for now, then doors, windows and
//! openings (ADR 0025).
//!
//! It depends on the core, the geometry port and the structural domain,
//! whose levels walls stand on. The app [registers](register) it after the
//! structural domain, and hands its [`generators`] to the geometry cache:
//!
//! ```
//! use hito_core::SchemaRegistry;
//! use hito_architectural::{class, wall};
//!
//! let mut registry = SchemaRegistry::new();
//! hito_architectural::register(&mut registry)?;
//! assert!(registry.class(&class(wall::WALL)).is_some());
//! # Ok::<(), hito_core::SchemaError>(())
//! ```
//!
//! So far it has:
//!
//! - [`wall_type`]s: a wall's layers, each with a thickness, a function and
//!   a material
//! - [`wall`]s: drawn as lines on a level, up to a level or a height, with
//!   offsets, attachable to slabs, structural or not, and flippable
//! - [`join`]s: walls that touch join on their own, layer by layer, and
//!   each wall's [`footprint`] in plan
//!
//! Nothing here is specific to a country
//! ([ADR 0008](https://github.com/hito-project/hito/blob/main/docs/decisions/0008-argentina-first-international-by-design.md)).

pub mod footprint;
mod geometry;
pub mod join;
pub mod plan;
mod read;
pub mod wall;
pub mod wall_type;

use hito_core::{ClassId, Recompute, Rule, Schema, SchemaError, SchemaRegistry, SchemaVersion};
use hito_geometry::Generator;

/// The architectural schema's name. It prefixes every class name, as in
/// `Architectural:Wall`, and is stored in files.
pub const SCHEMA: &str = "Architectural";

/// The architectural schema's version.
pub const VERSION: SchemaVersion = SchemaVersion::new(1, 0, 0);

/// The architectural class or relationship type called `name`, such as
/// [`wall::WALL`].
pub fn class(name: &str) -> ClassId {
    ClassId::new(SCHEMA, name)
}

/// The architectural schema: every class and relationship type.
pub fn schema() -> Schema {
    let schema = Schema::new(SCHEMA, VERSION);
    let classes = wall_type::classes().into_iter().chain(wall::classes());
    let schema = classes.fold(schema, Schema::class);
    let relationships = wall_type::relationships()
        .into_iter()
        .chain(wall::relationships())
        .chain(join::relationships());
    relationships.fold(schema, Schema::relationship)
}

/// The architectural recompute functions.
pub fn recomputes() -> Vec<Recompute> {
    wall::recomputes()
}

/// The architectural validation rules.
pub fn rules() -> Vec<Rule> {
    wall_type::rules()
        .into_iter()
        .chain(wall::rules())
        .collect()
}

/// The architectural geometry generators, for the
/// [`GeometryCache`](hito_geometry::GeometryCache).
pub fn generators() -> Vec<Generator> {
    geometry::generators()
}

/// Registers the structural domain, which walls stand on, if it isn't
/// registered yet, then the architectural schema, its recompute functions
/// and its rules.
pub fn register(registry: &mut SchemaRegistry) -> Result<(), SchemaError> {
    if registry.schema_version(hito_structural::SCHEMA).is_none() {
        hito_structural::register(registry)?;
    }
    registry.register(schema())?;
    recomputes()
        .into_iter()
        .try_for_each(|r| registry.add_recompute(r))?;
    rules().into_iter().try_for_each(|r| registry.add_rule(r))
}
