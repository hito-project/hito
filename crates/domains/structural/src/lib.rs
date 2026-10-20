//! The structural domain, for the first milestone: a structural BIM modeller
//! ([ADR 0011](https://github.com/hito-project/hito/blob/main/docs/decisions/0011-mvp-structural-bim.md)).
//!
//! A domain adds element types to the core's schema system and depends on the
//! core and the geometry port only. The app [registers](register) it at
//! startup, and hands its [`generators`] to the geometry cache:
//!
//! ```
//! use hito_core::SchemaRegistry;
//! use hito_structural::{class, column, grid, level, material};
//!
//! let mut registry = SchemaRegistry::new();
//! hito_structural::register(&mut registry)?;
//! assert!(registry.class(&class(level::LEVEL)).is_some());
//! assert!(registry.class(&class(grid::GRID)).is_some());
//! assert!(registry.class(&class(material::CONCRETE)).is_some());
//! assert!(registry.class(&class(column::COLUMN)).is_some());
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
//! - [`section`] types: the rectangle or circle, and the material, that
//!   column types share with beam types
//! - [`column`]s: from a base level to a top level, on grid intersections
//!   or anywhere
//! - [`beam`]s: drawn on a level between columns or points, following the
//!   columns they frame into and stopping at their faces
//! - [`slab`]s: floors drawn by their boundary on a level, with their
//!   types and openings, and the [`outline`]s they're drawn with
//! - isolated [`footing`]s: rectangular pads under columns or free on a
//!   level
//!
//! Nothing here is specific to a country. Concrete grades and the formulas
//! behind their properties come from regional packs, such as `hito-pack-ar`
//! for CIRSOC 201
//! ([ADR 0008](https://github.com/hito-project/hito/blob/main/docs/decisions/0008-argentina-first-international-by-design.md)).

pub mod beam;
pub mod column;
pub mod footing;
mod geometry;
pub mod grid;
pub mod level;
pub mod material;
pub mod outline;
mod read;
pub mod section;
pub mod slab;

use hito_core::{ClassId, Recompute, Rule, Schema, SchemaError, SchemaRegistry, SchemaVersion};
use hito_geometry::Generator;

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
        .chain(material::classes())
        .chain(section::classes())
        .chain(column::classes())
        .chain(beam::classes())
        .chain(slab::classes())
        .chain(footing::classes());
    let schema = classes.fold(schema, Schema::class);
    let relationships = level::relationships()
        .into_iter()
        .chain(section::relationships())
        .chain(column::relationships())
        .chain(beam::relationships())
        .chain(slab::relationships())
        .chain(footing::relationships());
    relationships.fold(schema, Schema::relationship)
}

/// The structural recompute functions: how elements follow what they
/// depend on.
pub fn recomputes() -> Vec<Recompute> {
    level::recomputes()
        .into_iter()
        .chain(column::recomputes())
        .chain(beam::recomputes())
        .chain(slab::recomputes())
        .chain(footing::recomputes())
        .collect()
}

/// The structural validation rules.
pub fn rules() -> Vec<Rule> {
    level::rules()
        .into_iter()
        .chain(grid::rules())
        .chain(material::rules())
        .chain(section::rules())
        .chain(column::rules())
        .chain(beam::rules())
        .chain(slab::rules())
        .chain(footing::rules())
        .collect()
}

/// The structural geometry generators, for the
/// [`GeometryCache`](hito_geometry::GeometryCache).
pub fn generators() -> Vec<Generator> {
    geometry::generators()
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
