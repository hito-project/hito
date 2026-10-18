//! The project being edited: a new project as Revit's templates start one,
//! and where new elements go.

use hito_core::{
    Element, ElementStore, Model, ModelId, ModelKind, SchemaRegistry, StoreError, StoreView,
    Transaction,
};
use hito_structural::{class, level};

use crate::i18n::I18n;

/// The default levels of a new project: their numbers and elevations, in
/// metres. Revit's templates start with two levels, so the first walls and
/// columns have a level to stand on and one to reach (ADR 0007).
const DEFAULT_LEVELS: [(u32, f64); 2] = [(1, 0.0), (2, 3.0)];

/// A new project: a physical model with the default levels, named in the
/// user's language ("Nivel 1", "Nivel 2"), in a store with `schemas`, which
/// must include the structural schema. Names are data from then on, so they
/// don't change with the language.
pub fn new_project(schemas: SchemaRegistry, i18n: &I18n) -> ElementStore {
    let mut store = ElementStore::new(schemas);
    let mut tx = store.transaction();
    let model = physical_model(&mut tx).expect("a new store takes a model");
    for (number, elevation) in DEFAULT_LEVELS {
        let level = Element::new(class(level::LEVEL), model)
            .with_property(level::NAME, level_name(i18n, number))
            .with_property(level::ELEVATION, elevation);
        tx.insert(level).expect("the default levels are valid");
    }
    tx.commit().expect("the default levels are valid");
    store
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
    fn a_new_project_has_two_levels_named_in_the_users_language() {
        let store = structural(&I18n::new(Locale::EsAr));
        let view = store.view();
        let levels = level::levels(&view);
        let names: Vec<_> = levels
            .iter()
            .map(|(id, _)| store.property(*id, level::NAME).cloned())
            .collect();
        let name = |s: &str| Some(Value::Text(s.into()));
        assert_eq!(names, [name("Nivel 1"), name("Nivel 2")]);
        let elevations: Vec<_> = levels.iter().map(|(_, e)| *e).collect();
        assert_eq!(elevations, [0.0, 3.0]);
        assert_eq!(store.models().count(), 1);
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
