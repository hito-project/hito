//! Materials: what structural elements are made of (#36).
//!
//! A material is a definition element, shared by the elements and types
//! that use it. Its properties come in two parts, kept apart so that later
//! rendering work (E29) adds to materials without changing these:
//!
//! - **engineering properties**, which analysis and design read: density,
//!   and for concrete its specified compressive strength and elastic modulus
//! - **graphics**: the colour elements of the material are shaded with in
//!   views. Rendering appearance (textures, reflectance) isn't part of it and
//!   will be its own definition.
//!
//! Property names follow IFC's material property sets (`Pset_MaterialCommon`,
//! `Pset_MaterialMechanical`, `Pset_MaterialConcrete`), so export maps them
//! one to one. Values are in coherent SI units, as the core stores them:
//! 25 MPa is `25e6`.
//!
//! The domain has no material library and no design-code formulas. A
//! regional pack provides the grades its codes define (such as CIRSOC 201's
//! H-25), and users create their own, as any other element:
//!
//! ```
//! use hito_core::{Element, ElementStore, Model, ModelKind, SchemaRegistry};
//! use hito_structural::{class, material};
//!
//! let mut registry = SchemaRegistry::new();
//! hito_structural::register(&mut registry)?;
//! let mut store = ElementStore::new(registry);
//! let mut tx = store.transaction();
//! let definitions = tx.insert_model(Model::new(ModelKind::Definition))?;
//! let concrete = Element::new(class(material::CONCRETE), definitions)
//!     .with_property(material::NAME, "Site mix, tested")
//!     .with_property(material::COMPRESSIVE_STRENGTH, 27e6)
//!     .with_property(material::YOUNG_MODULUS, 26.5e9)
//!     .with_property(material::MASS_DENSITY, 2400.0)
//!     .with_property(material::SHADING_COLOUR, "#a0a0a0");
//! tx.insert(concrete)?;
//! tx.commit()?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use hito_core::{ClassDef, ClassId, DataType, Dimension, Rule, Value};

use crate::class;

/// The base class of every material.
pub const MATERIAL: &str = "Material";
/// Concrete, of any grade.
pub const CONCRETE: &str = "ConcreteMaterial";

/// The material's name, unique among the project's materials, such as
/// `H-25`. It's data, not interface text, so it isn't translated.
pub const NAME: &str = "Name";
/// Mass per unit volume, in kg/m³ (IFC `MassDensity`).
pub const MASS_DENSITY: &str = "MassDensity";
/// The colour elements of the material are shaded with in views, as
/// `#rrggbb` in sRGB.
pub const SHADING_COLOUR: &str = "ShadingColour";
/// Concrete's specified compressive strength, f'c, in Pa (IFC
/// `CompressiveStrength`).
pub const COMPRESSIVE_STRENGTH: &str = "CompressiveStrength";
/// The modulus of elasticity, in Pa (IFC `YoungModulus`). For concrete it's
/// the secant modulus that design codes give from f'c, or a tested value.
pub const YOUNG_MODULUS: &str = "YoungModulus";

const STRESS: DataType = DataType::Quantity(Dimension::PRESSURE);

/// The material classes, base classes first.
pub(crate) fn classes() -> [ClassDef; 2] {
    let material = ClassDef::new(MATERIAL, ClassId::DEFINITION_ELEMENT)
        .property(NAME, DataType::Text)
        .property(MASS_DENSITY, DataType::Quantity(Dimension::DENSITY))
        .property(SHADING_COLOUR, DataType::Text);
    let concrete = ClassDef::new(CONCRETE, class(MATERIAL))
        .property(COMPRESSIVE_STRENGTH, STRESS)
        .property(YOUNG_MODULUS, STRESS);
    [material, concrete]
}

/// The material rules:
///
/// - `MaterialName`: every material has a name, and no two materials share
///   one, as in Revit.
/// - `PositiveMaterialProperty`: density, strength and modulus, when set,
///   are greater than zero.
/// - `ShadingColour`: the colour, when set, is written `#rrggbb`.
pub(crate) fn rules() -> Vec<Rule> {
    let name = Rule::new("MaterialName", class(MATERIAL), |store, material| {
        let name = match material.properties().get(NAME) {
            Some(Value::Text(name)) if !name.trim().is_empty() => name,
            _ => return Err("the material has no name".into()),
        };
        let registry = store.registry();
        let same_name = store.elements().find(|other| {
            other.id() != material.id()
                && registry.is_a(other.class(), &class(MATERIAL))
                && other.properties().get(NAME) == Some(&Value::Text(name.clone()))
        });
        match same_name {
            Some(other) => Err(format!("material {} is also called `{name}`", other.id())),
            None => Ok(()),
        }
    });

    let positive = Rule::new(
        "PositiveMaterialProperty",
        class(MATERIAL),
        |_, material| {
            let properties = [MASS_DENSITY, COMPRESSIVE_STRENGTH, YOUNG_MODULUS];
            let not_positive =
                properties
                    .into_iter()
                    .find_map(|name| match material.properties().get(name) {
                        Some(Value::Real(x)) if *x <= 0.0 => Some((name, *x)),
                        _ => None,
                    });
            match not_positive {
                Some((name, x)) => Err(format!("{name} is {x}, not greater than zero")),
                None => Ok(()),
            }
        },
    );

    let colour = Rule::new(
        "ShadingColour",
        class(MATERIAL),
        |_, material| match material.properties().get(SHADING_COLOUR) {
            Some(Value::Text(colour)) if parse_colour(colour).is_none() => Err(format!(
                "the shading colour `{colour}` isn't written #rrggbb"
            )),
            _ => Ok(()),
        },
    );

    vec![name, positive, colour]
}

/// The red, green and blue of a colour written `#rrggbb`, such as a
/// [`SHADING_COLOUR`]. Either case is accepted.
pub fn parse_colour(colour: &str) -> Option<[u8; 3]> {
    let hex = colour.strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_six_hex_digits() {
        assert_eq!(parse_colour("#A0b0C0"), Some([0xa0, 0xb0, 0xc0]));
        assert_eq!(parse_colour("#000000"), Some([0, 0, 0]));
        assert_eq!(parse_colour("a0b0c0"), None);
        assert_eq!(parse_colour("#a0b0c"), None);
        assert_eq!(parse_colour("#a0b0c0f"), None);
        assert_eq!(parse_colour("#g0b0c0"), None);
        // Six bytes, but not six digits.
        assert_eq!(parse_colour("#+1+1+1"), None);
        assert_eq!(parse_colour("#ééé"), None);
    }
}
