//! The concrete grades of CIRSOC 201-2005, as structural materials (#36).
//!
//! The grades and their values are in
//! [`data/cirsoc-201-2005-concrete.toml`](https://github.com/hito-project/hito/blob/main/crates/adapters/pack-ar/data/cirsoc-201-2005-concrete.toml),
//! built into the program. **No engineer has reviewed the table yet.** Every
//! value there names the clause it comes from:
//!
//! - the classes and their specified compressive strength f'c: article
//!   2.3.2, Tabla 2.7
//! - the elastic modulus: article 8.5.1, expression (8-1), [`elastic_modulus`]
//! - the unit weight for self-weight: CIRSOC 101-2005, Tabla 3.1, and the
//!   mass density derived from it with [`STANDARD_GRAVITY`]
//!
//! A project gets them as [`ConcreteMaterial`](hito_structural::material::CONCRETE)
//! elements, which users can then edit like their own:
//!
//! ```
//! use hito_core::{ElementStore, Model, ModelKind, SchemaRegistry, Value};
//! use hito_pack_ar::concrete::ConcreteTable;
//! use hito_structural::material;
//!
//! let mut registry = SchemaRegistry::new();
//! hito_structural::register(&mut registry)?;
//! let mut store = ElementStore::new(registry);
//! let mut tx = store.transaction();
//! let definitions = tx.insert_model(Model::new(ModelKind::Definition))?;
//! let table = ConcreteTable::cirsoc_201()?;
//! for material in table.materials(definitions) {
//!     tx.insert(material)?;
//! }
//! tx.commit()?;
//!
//! let h25 = table.grade("H-25").unwrap();
//! assert_eq!(h25.compressive_strength.value, 25e6);
//! assert_eq!(h25.young_modulus.value, 23_500e6);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::collections::BTreeMap;
use std::fmt;

use hito_core::{Dimension, Element, ModelId};
use hito_structural::{class, material};
use hito_units::{NumberFormat, UnitCatalogue, Units};
use serde::Deserialize;

/// The table, as built into the program. It isn't reviewed yet.
const CIRSOC_201: &str = include_str!("../data/cirsoc-201-2005-concrete.toml");

/// Standard gravity, g<sub>n</sub> = 9.806 65 m/s², exact by definition
/// (3rd CGPM, 1901). It turns a unit weight from CIRSOC 101 into a mass
/// density, and back into the same self-weight.
pub const STANDARD_GRAVITY: f64 = 9.806_65;

/// The elastic modulus E<sub>c</sub> of normal-density concrete (2000 to
/// 2800 kg/m³) with specified compressive strength `compressive_strength`,
/// both in Pa: CIRSOC 201-2005, article 8.5.1, expression (8-1),
/// E<sub>c</sub> = 4700 √f'c, in MPa.
///
/// The code says the expression "can" be used: a tested modulus replaces it.
/// Its commentary (C 8.5.1) adds a factor K for some Argentine aggregates,
/// which isn't applied here.
pub fn elastic_modulus(compressive_strength: f64) -> f64 {
    const MPA: f64 = 1e6;
    4700.0 * (compressive_strength / MPA).sqrt() * MPA
}

/// A source the table cites.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub title: String,
    pub edition: Option<String>,
    pub url: Option<String>,
    /// SHA-256 of the file that was read, so a reviewer can check they read
    /// the same one.
    pub sha256: Option<String>,
}

/// How a value in the table was established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Read in the cited clause of the primary source.
    Verified,
    /// Computed from verified values with the cited formula. The tests
    /// recompute it.
    Derived,
    /// A choice HITO makes where the code doesn't decide. An engineer
    /// reviews it.
    Assumption,
    /// Not checked against the primary source yet.
    Unverified,
}

/// A value and where it comes from.
#[derive(Debug, Clone, PartialEq)]
pub struct Cited<T> {
    /// The value, in coherent SI units if it's a quantity.
    pub value: T,
    /// The key of its [`Source`] in [`ConcreteTable::sources`].
    pub source: String,
    /// Where in the source: the clause, table or formula.
    pub clause: String,
    pub status: Status,
    pub note: Option<String>,
}

/// A concrete grade, such as H-25, with its cited values.
#[derive(Debug, Clone, PartialEq)]
pub struct ConcreteGrade {
    /// The grade's name, as the code writes it without spaces: `H-25`.
    pub name: String,
    /// What the code allows it for: plain, reinforced or prestressed
    /// concrete.
    pub usage: Cited<String>,
    /// f'c, in Pa.
    pub compressive_strength: Cited<f64>,
    /// E<sub>c</sub>, in Pa.
    pub young_modulus: Cited<f64>,
    /// The weight per unit volume for self-weight, in N/m³.
    pub unit_weight: Cited<f64>,
    /// In kg/m³.
    pub mass_density: Cited<f64>,
    /// `#rrggbb`.
    pub shading_colour: Cited<String>,
}

impl ConcreteGrade {
    /// The grade as a concrete material element in `model`, a definition
    /// model. It's new each time, with a new ID.
    pub fn material(&self, model: ModelId) -> Element {
        Element::new(class(material::CONCRETE), model)
            .with_property(material::NAME, self.name.as_str())
            .with_property(
                material::COMPRESSIVE_STRENGTH,
                self.compressive_strength.value,
            )
            .with_property(material::YOUNG_MODULUS, self.young_modulus.value)
            .with_property(material::MASS_DENSITY, self.mass_density.value)
            .with_property(material::SHADING_COLOUR, self.shading_colour.value.as_str())
    }
}

/// Who last reviewed a table, and when. Both are [`Review::NOT_REVIEWED`]
/// until an engineer has.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub reviewer: String,
    pub date: String,
}

impl Review {
    /// What the data file says while no engineer has reviewed the table.
    pub const NOT_REVIEWED: &str = "not reviewed";

    /// Whether an engineer has reviewed the table.
    pub fn is_reviewed(&self) -> bool {
        self.reviewer != Self::NOT_REVIEWED
    }
}

/// A table of concrete grades and the sources it cites.
#[derive(Debug, Clone, PartialEq)]
pub struct ConcreteTable {
    pub review: Review,
    pub sources: BTreeMap<String, Source>,
    /// In the table's order, weakest first.
    pub grades: Vec<ConcreteGrade>,
}

impl ConcreteTable {
    /// The CIRSOC 201-2005 grades, from the table built into the program.
    /// No engineer has reviewed it yet (see [`Review::is_reviewed`]). The
    /// tests check it always reads.
    pub fn cirsoc_201() -> Result<Self, DataError> {
        Self::parse(CIRSOC_201)
    }

    /// Reads a table in the format of `data/cirsoc-201-2005-concrete.toml`.
    /// Quantities are read through the standard unit catalogue, with a
    /// decimal point.
    pub fn parse(text: &str) -> Result<Self, DataError> {
        let raw: RawTable = toml::from_str(text).map_err(|e| DataError(e.to_string()))?;
        let numbers = NumberFormat::new("en").map_err(|e| DataError(e.to_string()))?;
        let reader = Reader {
            units: Units::new(UnitCatalogue::standard(), numbers),
            sources: &raw.sources,
        };
        let mut grades: Vec<ConcreteGrade> = Vec::new();
        for raw in raw.grades {
            let grade = reader.grade(raw)?;
            if grades.iter().any(|g| g.name == grade.name) {
                return Err(DataError(format!("grade {} appears twice", grade.name)));
            }
            grades.push(grade);
        }
        Ok(Self {
            review: raw.review,
            sources: raw.sources,
            grades,
        })
    }

    /// The grade called `name`, such as `H-25`.
    pub fn grade(&self, name: &str) -> Option<&ConcreteGrade> {
        self.grades.iter().find(|g| g.name == name)
    }

    /// Every grade as a concrete material element in `model`, a definition
    /// model.
    pub fn materials(&self, model: ModelId) -> Vec<Element> {
        self.grades.iter().map(|g| g.material(model)).collect()
    }
}

/// A table that can't be read, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataError(pub String);

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the concrete table can't be read: {}", self.0)
    }
}

impl std::error::Error for DataError {}

/// The table as written, before units are read.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTable {
    review: Review,
    sources: BTreeMap<String, Source>,
    grades: Vec<RawGrade>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawGrade {
    name: String,
    #[serde(rename = "use")]
    usage: RawValue,
    compressive_strength: RawValue,
    young_modulus: RawValue,
    unit_weight: RawValue,
    mass_density: RawValue,
    shading_colour: RawValue,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawValue {
    value: String,
    source: String,
    clause: String,
    status: Status,
    note: Option<String>,
}

struct Reader<'a> {
    units: Units,
    sources: &'a BTreeMap<String, Source>,
}

impl Reader<'_> {
    fn grade(&self, raw: RawGrade) -> Result<ConcreteGrade, DataError> {
        let name = raw.name;
        let error = |what: &str, why: String| DataError(format!("{name}, {what}: {why}"));
        let stress = Dimension::PRESSURE;
        let unit_weight = Dimension::FORCE.div(Dimension::VOLUME);
        let shading_colour = self.text(raw.shading_colour, |e| error("shading-colour", e))?;
        if material::parse_colour(&shading_colour.value).is_none() {
            let why = format!("`{}` isn't #rrggbb", shading_colour.value);
            return Err(error("shading-colour", why));
        }
        Ok(ConcreteGrade {
            usage: self.text(raw.usage, |e| error("use", e))?,
            compressive_strength: self.quantity(raw.compressive_strength, stress, |e| {
                error("compressive-strength", e)
            })?,
            young_modulus: self
                .quantity(raw.young_modulus, stress, |e| error("young-modulus", e))?,
            unit_weight: self
                .quantity(raw.unit_weight, unit_weight, |e| error("unit-weight", e))?,
            mass_density: self.quantity(raw.mass_density, Dimension::DENSITY, |e| {
                error("mass-density", e)
            })?,
            shading_colour,
            name,
        })
    }

    fn quantity(
        &self,
        raw: RawValue,
        dimension: Dimension,
        error: impl Fn(String) -> DataError,
    ) -> Result<Cited<f64>, DataError> {
        // A bare number would be read in SI units, silently: 25 Pa for a
        // strength of 25 MPa. Every value must name its unit.
        if self.units.parse(&raw.value, Dimension::NONE).is_ok() {
            return Err(error(format!("`{}` has no unit", raw.value)));
        }
        let value = self.units.parse(&raw.value, dimension);
        let value = value.map_err(|e| error(e.to_string()))?;
        self.cite(raw, value, error)
    }

    fn text(
        &self,
        raw: RawValue,
        error: impl Fn(String) -> DataError,
    ) -> Result<Cited<String>, DataError> {
        let value = raw.value.clone();
        self.cite(raw, value, error)
    }

    fn cite<T>(
        &self,
        raw: RawValue,
        value: T,
        error: impl Fn(String) -> DataError,
    ) -> Result<Cited<T>, DataError> {
        if !self.sources.contains_key(&raw.source) {
            return Err(error(format!("source `{}` isn't listed", raw.source)));
        }
        Ok(Cited {
            value,
            source: raw.source,
            clause: raw.clause,
            status: raw.status,
            note: raw.note,
        })
    }
}
