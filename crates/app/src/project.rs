//! The project being edited: a new project as Revit's templates start one,
//! and where new elements go.

use hito_core::{
    Element, ElementId, ElementStore, Model, ModelId, ModelKind, SchemaRegistry, StoreError,
    StoreView, Transaction, Value,
};
use hito_structural::{class, level, material};

use crate::i18n::I18n;

/// A new project: a physical model with one level at 0.00 m, named in the
/// user's language ("Nivel 1"), so the first elements have a level to stand
/// on, a generic concrete, generic column, beam, slab and footing types
/// and, when the architectural schema is registered, a generic wall type.
/// More levels are added with the Level command. The store has `schemas`,
/// which must include the structural schema. Names are data from then on,
/// so they don't change with the language.
pub fn new_project(schemas: SchemaRegistry, i18n: &I18n) -> ElementStore {
    let mut store = ElementStore::new(schemas);
    let mut tx = store.transaction();
    let model = physical_model(&mut tx).expect("a new store takes a model");
    let level = Element::new(class(level::LEVEL), model)
        .with_property(level::NAME, level_name(i18n, 1))
        .with_property(level::ELEVATION, 0.0);
    tx.insert(level).expect("the default level is valid");
    generic_concrete(&mut tx, i18n).expect("the generic concrete is valid");
    generic_framing_types(&mut tx, i18n).expect("the generic framing types are valid");
    if tx
        .view()
        .registry()
        .schema_version(hito_architectural::SCHEMA)
        .is_some()
    {
        generic_wall_type(&mut tx, i18n).expect("the generic wall type is valid");
    }
    generic_slab_type(&mut tx, i18n).expect("the generic slab type is valid");
    generic_footing_type(&mut tx, i18n).expect("the generic footing type is valid");
    tx.commit().expect("the default level is valid");
    store
}

/// One rectangular footing type, 1.8 x 1.2 m and 45 cm thick, of the
/// generic concrete, as Revit's metric templates load "Footing-Rectangular
/// 1800 x 1200 x 450mm", so the Isolated footing tool works in a new
/// project. Regional footing types come with regional packs (#167).
fn generic_footing_type(tx: &mut Transaction<'_>, i18n: &I18n) -> Result<(), StoreError> {
    let concrete = generic_concrete(tx, i18n)?;
    let definitions = definitions_model(tx)?;
    let name = i18n.tr("footing-type-generic");
    let size = [1.8, 1.2, 0.45];
    hito_structural::footing::insert_type(tx, definitions, &name, size, Some(concrete)).map(drop)
}

/// One generic slab type, 15 cm of the generic concrete, as Revit's metric
/// templates have a "Generic 150mm" floor, so the Slab tool works in a new
/// project. Regional slab types come with regional packs (#167).
fn generic_slab_type(tx: &mut Transaction<'_>, i18n: &I18n) -> Result<(), StoreError> {
    let concrete = generic_concrete(tx, i18n)?;
    let definitions = definitions_model(tx)?;
    let name = i18n.tr("slab-type-generic");
    hito_structural::slab::insert_type(tx, definitions, &name, 0.15, Some(concrete)).map(drop)
}

/// One generic wall type, 20 cm of a single structural layer, as Revit's
/// templates start with "Generic - 200mm", so the Wall tool works in a new
/// project. It's named in the user's language and goes in a definition
/// model. Regional wall types come with regional packs (#167).
fn generic_wall_type(tx: &mut Transaction<'_>, i18n: &I18n) -> Result<(), StoreError> {
    use hito_architectural::wall_type::{self, Function, Layer};
    let definitions = definitions_model(tx)?;
    let name = i18n.tr("wall-type-generic");
    let layers = [Layer::new(Function::Structure, 0.2)];
    wall_type::insert(tx, definitions, &name, &layers).map(drop)
}

/// The project's generic concrete, added if it has none: a
/// [`ConcreteMaterial`](hito_structural::material::CONCRETE) named in the
/// user's language ("Hormigón"), with no strength, density or modulus, as
/// no code defines a generic concrete. The generic types use it, so a
/// generic structural wall is concrete. Graded concrete, such as CIRSOC
/// 201's H-25, comes from regional packs.
pub(crate) fn generic_concrete(
    tx: &mut Transaction<'_>,
    i18n: &I18n,
) -> Result<ElementId, StoreError> {
    let name = Value::Text(i18n.tr("material-concrete-generic"));
    let concrete = class(material::CONCRETE);
    let view = tx.view();
    let registry = view.registry();
    let existing = view.elements().find(|e| {
        registry.is_a(e.class(), &concrete) && e.properties().get(material::NAME) == Some(&name)
    });
    if let Some(existing) = existing {
        return Ok(existing.id());
    }
    let definitions = definitions_model(tx)?;
    tx.insert(Element::new(concrete, definitions).with_property(material::NAME, name))
}

/// One generic column type, a 30 x 30 cm rectangle, and one generic beam
/// type, 20 cm wide and 40 cm deep, both of the generic concrete, so the
/// Column and Beam tools work in a new project, as Revit's templates load
/// concrete column and beam families.
fn generic_framing_types(tx: &mut Transaction<'_>, i18n: &I18n) -> Result<(), StoreError> {
    use hito_structural::beam::BEAM_TYPE;
    use hito_structural::column::COLUMN_TYPE;
    use hito_structural::section::{self, Section};
    let concrete = generic_concrete(tx, i18n)?;
    let definitions = definitions_model(tx)?;
    let rectangle = |width, depth| Section::Rectangle { width, depth };
    let types = [
        (COLUMN_TYPE, "column-type-generic", rectangle(0.3, 0.3)),
        (BEAM_TYPE, "beam-type-generic", rectangle(0.2, 0.4)),
    ];
    for (class, name, shape) in types {
        section::insert(
            tx,
            definitions,
            class,
            &i18n.tr(name),
            shape,
            Some(concrete),
        )?;
    }
    Ok(())
}

/// The definition model types and materials go in, added if the project
/// has none.
fn definitions_model(tx: &mut Transaction<'_>) -> Result<ModelId, StoreError> {
    let existing = tx
        .view()
        .models()
        .find(|m| m.kind() == ModelKind::Definition)
        .map(Model::id);
    match existing {
        Some(model) => Ok(model),
        None => tx.insert_model(Model::new(ModelKind::Definition)),
    }
}

/// The physical model new elements go in, added if the project has none.
pub fn physical_model(tx: &mut Transaction<'_>) -> Result<ModelId, StoreError> {
    match physical(&tx.view()) {
        Some(model) => Ok(model),
        None => tx.insert_model(Model::new(ModelKind::Physical)),
    }
}

fn physical(view: &StoreView<'_>) -> Option<ModelId> {
    let mut models = view.models();
    models
        .find(|m| m.kind() == ModelKind::Physical)
        .map(Model::id)
}

/// A level's name with a number, such as "Nivel 2". The number is added in
/// code, not through a Fluent argument, so the name has no invisible
/// isolation marks around it.
pub fn level_name(i18n: &I18n, number: u32) -> String {
    format!("{} {number}", i18n.tr("level-name"))
}

#[cfg(test)]
pub(crate) mod tests {
    use hito_core::Value;

    use super::*;
    use crate::i18n::Locale;

    /// A new project with the structural schema, as the shell makes one.
    pub(crate) fn structural(i18n: &I18n) -> ElementStore {
        let mut schemas = SchemaRegistry::new();
        hito_structural::register(&mut schemas).unwrap();
        new_project(schemas, i18n)
    }

    #[test]
    fn a_new_project_has_one_level_named_in_the_users_language() {
        let store = structural(&I18n::new(Locale::EsAr));
        let view = store.view();
        let levels = level::levels(&view);
        let names: Vec<_> = levels
            .iter()
            .map(|(id, _)| store.property(*id, level::NAME).cloned())
            .collect();
        let name = |s: &str| Some(Value::Text(s.into()));
        assert_eq!(names, [name("Nivel 1")]);
        let elevations: Vec<_> = levels.iter().map(|(_, e)| *e).collect();
        assert_eq!(elevations, [0.0]);
        // The physical model, and the definitions with the generic concrete.
        assert_eq!(store.models().count(), 2);
    }

    #[test]
    fn a_new_project_has_a_generic_concrete_with_no_strength() {
        let mut store = structural(&I18n::new(Locale::EsAr));
        let view = store.view();
        let registry = view.registry();
        let concretes: Vec<_> = view
            .elements()
            .filter(|e| registry.is_a(e.class(), &class(material::CONCRETE)))
            .map(|e| e.id())
            .collect();
        let [concrete] = concretes[..] else {
            panic!("one concrete: {concretes:?}");
        };
        assert_eq!(
            store.property(concrete, material::NAME),
            Some(&Value::Text("Hormigón".into()))
        );
        assert_eq!(
            store.property(concrete, material::COMPRESSIVE_STRENGTH),
            None
        );

        // Asking for it again finds the same one.
        let i18n = I18n::new(Locale::EsAr);
        let mut tx = store.transaction();
        assert_eq!(generic_concrete(&mut tx, &i18n).unwrap(), concrete);
    }

    #[test]
    fn new_elements_go_in_the_physical_model() {
        let mut store = structural(&I18n::new(Locale::En));
        let existing = physical(&store.view());
        let mut tx = store.transaction();
        assert_eq!(physical_model(&mut tx).ok(), existing);
        tx.roll_back();

        let mut empty = ElementStore::new(SchemaRegistry::new());
        let mut tx = empty.transaction();
        let added = physical_model(&mut tx).unwrap();
        assert_eq!(physical_model(&mut tx).unwrap(), added, "added once");
    }
}
