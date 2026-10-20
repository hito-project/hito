//! The Level tool (`LL`): adds levels at typed elevations (#33).
//!
//! Revit draws levels in an elevation or section view. Until HITO has those
//! (#197, #195), the tool asks for the elevation on the command line. Like
//! Revit's, it stays active, so several levels go in one after another, each
//! its own undo step, until Esc. New levels are named after the newest
//! numbered level, counting up: "Nivel 2" → "Nivel 3".

use hito_core::{Dimension, Element, ElementId, StoreError, StoreView, Transaction, Value};
use hito_structural::{class, grid, level};

use crate::command::{Context, Input, Next, Prompt, Tool, Wants};
use crate::project;

/// Starts the Level tool.
pub fn start() -> Box<dyn Tool> {
    Box::new(LevelTool::default())
}

#[derive(Default)]
struct LevelTool {
    /// The localised word new level names start with when no level name
    /// ends in a number, such as "Nivel".
    stem: String,
    elevation: Option<f64>,
}

impl Tool for LevelTool {
    fn start(&mut self, context: &Context<'_>) {
        self.stem = context.i18n.tr("level-name");
    }

    fn prompt(&self) -> Prompt {
        Prompt {
            message: "level-prompt-elevation",
            wants: Wants::Quantity(Dimension::LENGTH),
        }
    }

    fn input(&mut self, input: Input) -> Next {
        match input {
            Input::Quantity(elevation) => {
                self.elevation = Some(elevation);
                Next::Repeat
            }
            Input::Point(_) | Input::Element(_) => Next::Reject("input-not-a-value"),
        }
    }

    fn commit(self: Box<Self>, tx: &mut Transaction<'_>) -> Result<(), StoreError> {
        let elevation = self.elevation.expect("asked for the elevation");
        let model = project::physical_model(tx)?;
        let name = next_level_name(&tx.view(), &self.stem);
        let level = Element::new(class(level::LEVEL), model)
            .with_property(level::NAME, name)
            .with_property(level::ELEVATION, elevation);
        tx.insert(level).map(drop)
    }
}

/// The name for a new level: the newest level whose name ends in a number,
/// counted up ("Nivel 2" → "Nivel 3", "Piso 09" → "Piso 10"), or `stem`
/// and 1 if no level name ends in a number. It skips names already taken,
/// since level names are unique.
pub fn next_level_name(view: &StoreView<'_>, stem: &str) -> String {
    let registry = view.registry();
    let names: Vec<(ElementId, &str)> = view
        .elements()
        .filter(|e| registry.is_a(e.class(), &class(level::LEVEL)))
        .filter_map(|e| match e.properties().get(level::NAME) {
            Some(Value::Text(name)) => Some((e.id(), name.as_str())),
            _ => None,
        })
        .collect();
    let numbered = names
        .iter()
        .filter(|(_, name)| name.ends_with(|c: char| c.is_ascii_digit()))
        .max_by_key(|(id, _)| *id);
    let mut name = match numbered {
        // `next_label` counts up a final run of digits, keeping the rest.
        Some((_, name)) => next_number(name),
        None => format!("{stem} 1"),
    };
    while names.iter().any(|(_, taken)| *taken == name) {
        name = next_number(&name);
    }
    name
}

/// A name that ends in a number, with the number counted up.
fn next_number(name: &str) -> String {
    grid::next_label(name).expect("the name ends in a digit")
}

#[cfg(test)]
mod tests {
    use hito_core::ElementStore;

    use super::*;
    use crate::command::{InputFormat, Outcome, Registry, Run, Session};
    use crate::i18n::{I18n, Locale};

    fn level_names(store: &ElementStore) -> Vec<(String, f64)> {
        let view = store.view();
        level::levels(&view)
            .into_iter()
            .map(|(id, elevation)| match store.property(id, level::NAME) {
                Some(Value::Text(name)) => (name.clone(), elevation),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    fn rename(store: &mut ElementStore, from: &str, to: &str) {
        let id = store
            .elements()
            .find(|e| e.properties().get(level::NAME) == Some(&Value::Text(from.into())))
            .map(Element::id)
            .unwrap();
        let mut tx = store.transaction();
        tx.set_property(id, level::NAME, to).unwrap();
        tx.commit().unwrap();
    }

    #[test]
    fn levels_are_added_at_typed_elevations_until_esc() {
        let i18n = I18n::new(Locale::EsAr);
        let mut store = project::tests::structural(&i18n);
        let mut registry = Registry::new();
        let command = crate::command::Command {
            name: "LEVEL",
            label: "command-level",
            alias: Some("LL"),
            run: Run::Tool(start),
        };
        registry.register(command).unwrap();
        let mut session = Session::new(registry);
        let format = InputFormat::new();
        let mut enter =
            |session: &mut Session, text| session.enter(text, &i18n, &format, &mut store);

        enter(&mut session, "LL");
        let Outcome::Committed(changes) = enter(&mut session, "3 m") else {
            panic!("committed");
        };
        assert_eq!(changes.command().map(|c| c.name()), Some("LEVEL"));
        assert!(matches!(
            enter(&mut session, "300 cm + 3 m"),
            Outcome::Committed(_)
        ));
        assert_eq!(session.running().map(|c| c.name), Some("LEVEL"));
        session.cancel(&i18n);
        // The first new level after the default one is "Nivel 2".
        let expected = [("Nivel 1", 0.0), ("Nivel 2", 3.0), ("Nivel 3", 6.0)];
        let expected: Vec<_> = expected.iter().map(|(n, e)| (n.to_string(), *e)).collect();
        assert_eq!(level_names(&store), expected);
    }

    #[test]
    fn new_names_count_up_from_the_newest_numbered_level() {
        let mut store = project::tests::structural(&I18n::new(Locale::En));
        assert_eq!(next_level_name(&store.view(), "Level"), "Level 2");
        let mut tx = store.transaction();
        let model = project::physical_model(&mut tx).unwrap();
        let second = Element::new(class(level::LEVEL), model)
            .with_property(level::NAME, "Level 2")
            .with_property(level::ELEVATION, 3.0);
        tx.insert(second).unwrap();
        tx.commit().unwrap();
        assert_eq!(next_level_name(&store.view(), "Level"), "Level 3");

        // Renamed levels without a number don't count; taken names are
        // skipped.
        rename(&mut store, "Level 2", "Roof");
        assert_eq!(next_level_name(&store.view(), "Level"), "Level 2");
        rename(&mut store, "Level 1", "Ground floor");
        assert_eq!(next_level_name(&store.view(), "Level"), "Level 1");
        rename(&mut store, "Roof", "Piso 09");
        assert_eq!(next_level_name(&store.view(), "Level"), "Piso 10");
    }
}
