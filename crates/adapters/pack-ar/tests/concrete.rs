//! The CIRSOC 201-2005 concrete table (#36): every derived value is
//! recomputed from the code's formulas, and the reviewed table is locked by
//! a snapshot.

use std::fmt::Write;

use hito_core::{Dimension, ElementStore, Model, ModelKind, SchemaRegistry, Value};
use hito_pack_ar::concrete::{Cited, ConcreteTable, STANDARD_GRAVITY, Status, elastic_modulus};
use hito_structural::material;
use hito_units::{NumberFormat, UnitCatalogue, Units};

const MPA: f64 = 1e6;

fn table() -> ConcreteTable {
    ConcreteTable::cirsoc_201().expect("the built-in table reads")
}

/// CIRSOC 201-2005, 2.3.2, Tabla 2.7: the classes, each named after its f'c
/// in MPa.
#[test]
fn the_table_has_every_class_of_tabla_2_7() {
    let table = table();
    let names: Vec<_> = table.grades.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "H-15", "H-20", "H-25", "H-30", "H-35", "H-40", "H-45", "H-50", "H-60"
        ]
    );
    for grade in &table.grades {
        let in_name: f64 = grade.name["H-".len()..].parse().unwrap();
        assert_eq!(
            grade.compressive_strength.value,
            in_name * MPA,
            "{}",
            grade.name
        );
    }
}

/// CIRSOC 201-2005, 8.5.1, expression (8-1): Ec = 4700 √f'c, in MPa. Values
/// whose square roots are exact pin the constant and the root.
#[test]
fn the_elastic_modulus_follows_expression_8_1() {
    assert_eq!(elastic_modulus(25.0 * MPA), 23_500.0 * MPA);
    assert_eq!(elastic_modulus(36.0 * MPA), 28_200.0 * MPA);
    assert_eq!(elastic_modulus(49.0 * MPA), 32_900.0 * MPA);
    assert_eq!(elastic_modulus(0.0), 0.0);
}

/// Each listed modulus is (8-1) applied to the grade's f'c, rounded to
/// 1 MPa, as the table says.
#[test]
fn every_elastic_modulus_is_recomputed_from_f_c() {
    for grade in table().grades {
        let expected = elastic_modulus(grade.compressive_strength.value);
        let listed = grade.young_modulus.value;
        assert!(
            (listed - expected).abs() <= 0.5 * MPA,
            "{}: the table lists {} MPa, (8-1) gives {} MPa",
            grade.name,
            listed / MPA,
            expected / MPA,
        );
        assert_eq!(grade.young_modulus.status, Status::Derived);
        assert!(grade.young_modulus.clause.contains("(8-1)"));
    }
}

/// Each mass density is the unit weight divided by standard gravity, rounded
/// to 1 kg/m³, so self-weight is the CIRSOC 101 value. It's also within the
/// normal density that (8-1) is valid for (8.5.1: 2000 to 2800 kg/m³).
#[test]
fn every_mass_density_is_recomputed_from_the_unit_weight() {
    for grade in table().grades {
        let expected = grade.unit_weight.value / STANDARD_GRAVITY;
        let listed = grade.mass_density.value;
        assert!(
            (listed - expected).abs() <= 0.5,
            "{}: the table lists {listed} kg/m³, the unit weight gives {expected} kg/m³",
            grade.name,
        );
        assert!((2000.0..=2800.0).contains(&listed), "{}", grade.name);
        assert_eq!(grade.mass_density.status, Status::Derived);
    }
}

#[test]
fn every_value_cites_a_listed_source() {
    let table = table();
    for grade in &table.grades {
        let citations = [
            cited(&grade.usage),
            cited(&grade.compressive_strength),
            cited(&grade.young_modulus),
            cited(&grade.unit_weight),
            cited(&grade.mass_density),
            cited(&grade.shading_colour),
        ];
        for (source, clause, _, _) in citations {
            assert!(table.sources.contains_key(source), "{}", grade.name);
            assert!(!clause.trim().is_empty(), "{}", grade.name);
        }
    }
}

#[test]
fn every_grade_is_a_valid_material() {
    let mut registry = SchemaRegistry::new();
    hito_structural::register(&mut registry).unwrap();
    let mut store = ElementStore::new(registry);
    let mut tx = store.transaction();
    let definitions = tx.insert_model(Model::new(ModelKind::Definition)).unwrap();
    let table = table();
    let ids: Vec<_> = table
        .materials(definitions)
        .into_iter()
        .map(|m| tx.insert(m).unwrap())
        .collect();
    tx.commit().unwrap();

    let h30 = ids[3];
    assert_eq!(
        store.property(h30, material::NAME),
        Some(&Value::Text("H-30".into()))
    );
    assert_eq!(
        store.property(h30, material::COMPRESSIVE_STRENGTH),
        Some(&Value::Real(30.0 * MPA))
    );
}

#[test]
fn a_table_with_mistakes_doesnt_read() {
    let good = include_str!("../data/cirsoc-201-2005-concrete.toml");
    let error = |from: &str, to: &str| {
        assert!(good.contains(from), "the test edit `{from}` must apply");
        let bad = good.replacen(from, to, 1);
        ConcreteTable::parse(&bad).unwrap_err().to_string()
    };
    assert!(error("\"15 MPa\"", "\"15 kN\"").contains("H-15, compressive-strength"));
    assert!(error("\"15 MPa\"", "\"15\"").contains("has no unit"));
    assert!(error("\"#c8c8c8\"", "\"grey\"").contains("shading-colour"));
    assert!(error("name = \"H-20\"", "name = \"H-15\"").contains("twice"));
    assert!(error("source = \"hito\"", "source = \"nowhere\"").contains("nowhere"));
    assert!(error("status = \"derived\"", "status = \"guessed\"").contains("guessed"));
}

/// **The reviewed table, locked.** The snapshot in
/// `tests/snapshots/concrete__cirsoc_201_2005_concrete.snap` is the table as
/// an engineer reviewed it, value by value, with sources and statuses.
///
/// Any change to the data file that changes what HITO reads fails here. To
/// change the table:
///
/// 1. Edit `data/cirsoc-201-2005-concrete.toml`, citing the source of every
///    new value, and set its `[review]` back to `pending`.
/// 2. Run `cargo insta test -p hito-pack-ar --review` (or `cargo test`, then
///    rename the `.snap.new` file over the `.snap` one) and commit the new
///    snapshot.
/// 3. Have an engineer review the snapshot diff in the pull request. Then
///    put their name and the date in `[review]`, update the snapshot again,
///    and only then mark the pull request ready.
///
/// The pull request that added the table (#36) followed the same steps.
/// Comments in the data file aren't part of the snapshot.
#[test]
fn the_reviewed_table_is_unchanged() {
    insta::assert_snapshot!("cirsoc_201_2005_concrete", render(&table()));
}

fn cited<T>(c: &Cited<T>) -> (&str, &str, Status, Option<&str>) {
    (&c.source, &c.clause, c.status, c.note.as_deref())
}

/// The table as text for review: each value in the units engineers use and
/// exactly in SI, with its citation.
fn render(table: &ConcreteTable) -> String {
    let numbers = NumberFormat::new("en").unwrap();
    let mut units = Units::new(UnitCatalogue::standard(), numbers);
    let unit_weight = Dimension::FORCE.div(Dimension::VOLUME);
    units.set_display(Dimension::PRESSURE, "MPa", 0).unwrap();
    units.set_display(unit_weight, "kN/m³", 1).unwrap();
    units.set_display(Dimension::DENSITY, "kg/m³", 0).unwrap();

    let mut out = String::new();
    let review = &table.review;
    writeln!(out, "reviewed by {} on {}", review.reviewer, review.date).unwrap();
    for (key, source) in &table.sources {
        writeln!(out, "source {key}: {}", source.title).unwrap();
        for (field, value) in [
            ("edition", &source.edition),
            ("url", &source.url),
            ("sha256", &source.sha256),
        ] {
            if let Some(value) = value {
                writeln!(out, "  {field}: {value}").unwrap();
            }
        }
    }
    for grade in &table.grades {
        writeln!(out, "\n{}", grade.name).unwrap();
        let quantity = |c: &Cited<f64>, dimension| {
            let shown = units.format(c.value, dimension);
            format!("{shown} (SI {:?})", c.value)
        };
        let rows = [
            ("use", grade.usage.value.clone(), cited(&grade.usage)),
            (
                "compressive strength",
                quantity(&grade.compressive_strength, Dimension::PRESSURE),
                cited(&grade.compressive_strength),
            ),
            (
                "young modulus",
                quantity(&grade.young_modulus, Dimension::PRESSURE),
                cited(&grade.young_modulus),
            ),
            (
                "unit weight",
                quantity(&grade.unit_weight, unit_weight),
                cited(&grade.unit_weight),
            ),
            (
                "mass density",
                quantity(&grade.mass_density, Dimension::DENSITY),
                cited(&grade.mass_density),
            ),
            (
                "shading colour",
                grade.shading_colour.value.clone(),
                cited(&grade.shading_colour),
            ),
        ];
        for (label, value, (source, clause, status, note)) in rows {
            writeln!(out, "  {label}: {value}").unwrap();
            writeln!(out, "    {status:?}, {source}: {clause}").unwrap();
            if let Some(note) = note {
                writeln!(out, "    note: {note}").unwrap();
            }
        }
    }
    out
}
