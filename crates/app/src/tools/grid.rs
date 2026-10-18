//! The Grid tool (`GR`): draws straight grid lines between two points (#34).
//!
//! Like Revit's, it stays active until Esc, each grid its own undo step,
//! and labels grids automatically: the first is `1`, and each new one
//! follows the newest grid's label (`1` → `2`, `A` → `B`), skipping labels
//! already taken. Labels are edited in the properties panel. Picking the
//! points in a plan view comes with #183; until then they're typed (`0,0`).

use hito_core::{ElementId, StoreError, StoreView, Transaction, Value};
use hito_structural::{class, grid};

use crate::command::{Input, Next, Prompt, Tool, Wants};
use crate::project;

/// The label of the first grid in a project, as in Revit.
const FIRST_LABEL: &str = "1";

/// Starts the Grid tool.
pub fn start() -> Box<dyn Tool> {
    Box::new(GridTool::default())
}

#[derive(Default)]
struct GridTool {
    start: Option<[f64; 2]>,
    end: Option<[f64; 2]>,
}

impl Tool for GridTool {
    fn prompt(&self) -> Prompt {
        let message = match self.start {
            None => "grid-prompt-start",
            Some(_) => "grid-prompt-end",
        };
        Prompt {
            message,
            wants: Wants::Point,
        }
    }

    fn input(&mut self, input: Input) -> Next {
        let Input::Point([x, y, _]) = input else {
            return Next::Reject("input-not-a-point");
        };
        match self.start {
            None => {
                self.start = Some([x, y]);
                Next::Ask
            }
            Some(start) if start == [x, y] => Next::Reject("grid-points-differ"),
            Some(_) => {
                self.end = Some([x, y]);
                Next::Repeat
            }
        }
    }

    fn commit(self: Box<Self>, tx: &mut Transaction<'_>) -> Result<(), StoreError> {
        let (start, end) = (self.start.expect("asked"), self.end.expect("asked"));
        let model = project::physical_model(tx)?;
        let label = next_grid_label(&tx.view());
        tx.insert(grid::GridLine::new(start, end).element(model, &label))
            .map(drop)
    }
}

/// The label for a new grid: the one after the newest grid's label, or `1`
/// for the first grid. If the newest label doesn't count up (such as
/// `A'`), the next free number is used.
pub fn next_grid_label(view: &StoreView<'_>) -> String {
    let registry = view.registry();
    let newest = view
        .elements()
        .filter(|e| registry.is_a(e.class(), &class(grid::GRID)))
        .filter_map(|e| match e.properties().get(grid::NAME) {
            Some(Value::Text(label)) => Some((e.id(), label.as_str())),
            _ => None,
        })
        .max_by_key(|(id, _): &(ElementId, &str)| *id);
    let free = |label: &str| grid::find(view, label).is_none();
    match newest.and_then(|(_, label)| grid::next_free_label(view, label)) {
        Some(label) => label,
        None if free(FIRST_LABEL) => FIRST_LABEL.to_owned(),
        None => grid::next_free_label(view, FIRST_LABEL).expect("numbers count up"),
    }
}

#[cfg(test)]
mod tests {
    use hito_core::ElementStore;

    use super::*;
    use crate::command::{Command, InputFormat, Outcome, Registry, Run, Session};
    use crate::i18n::{I18n, Locale};

    fn labels(store: &ElementStore) -> Vec<(String, grid::GridLine)> {
        let mut grids: Vec<_> = store
            .elements()
            .filter(|e| e.class() == &class(grid::GRID))
            .map(|e| e.id())
            .collect();
        grids.sort();
        grids
            .into_iter()
            .map(|id| match store.property(id, grid::NAME) {
                Some(Value::Text(label)) => (
                    label.clone(),
                    grid::GridLine::of(&store.view(), id).unwrap(),
                ),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    fn rename(store: &mut ElementStore, from: &str, to: &str) {
        let id = grid::find(&store.view(), from).unwrap();
        let mut tx = store.transaction();
        tx.set_property(id, grid::NAME, to).unwrap();
        tx.commit().unwrap();
    }

    #[test]
    fn grids_are_drawn_between_typed_points_until_esc() {
        let i18n = I18n::new(Locale::En);
        let mut store = project::tests::structural(&i18n);
        let mut registry = Registry::new();
        let command = Command {
            name: "GRID",
            label: "command-grid",
            alias: Some("GR"),
            run: Run::Tool(start),
        };
        registry.register(command).unwrap();
        let mut session = Session::new(registry);
        let format = InputFormat::new();
        let mut enter =
            |session: &mut Session, text| session.enter(text, &i18n, &format, &mut store);

        enter(&mut session, "GR");
        enter(&mut session, "0,0");
        enter(&mut session, "0,0");
        assert!(session.message().is_some(), "the ends must differ");
        let Outcome::Committed(changes) = enter(&mut session, "0,10") else {
            panic!("committed");
        };
        assert_eq!(changes.command().map(|c| c.name()), Some("GRID"));
        enter(&mut session, "6,0");
        assert!(matches!(
            enter(&mut session, "6,10 m"),
            Outcome::Committed(_)
        ));
        assert_eq!(session.running().map(|c| c.name), Some("GRID"));
        session.cancel(&i18n);

        let line = |a, b| grid::GridLine::new(a, b);
        assert_eq!(
            labels(&store),
            [
                ("1".to_string(), line([0.0, 0.0], [0.0, 10.0])),
                ("2".to_string(), line([6.0, 0.0], [6.0, 10.0])),
            ]
        );
    }

    #[test]
    fn labels_follow_the_newest_grid() {
        let mut store = project::tests::structural(&I18n::new(Locale::En));
        assert_eq!(next_grid_label(&store.view()), "1");
        let add = |store: &mut ElementStore| {
            let label = next_grid_label(&store.view());
            let mut tx = store.transaction();
            let model = project::physical_model(&mut tx).unwrap();
            let line = grid::GridLine::new([0.0, 0.0], [1.0, 0.0]);
            tx.insert(line.element(model, &label)).unwrap();
            tx.commit().unwrap();
        };
        add(&mut store);
        add(&mut store);
        assert_eq!(next_grid_label(&store.view()), "3");

        // Renamed to letters, the next grids follow in letters.
        rename(&mut store, "2", "A");
        assert_eq!(next_grid_label(&store.view()), "B");
        add(&mut store);
        rename(&mut store, "1", "C");
        assert_eq!(next_grid_label(&store.view()), "D", "B and C are taken");

        // A label that doesn't count up falls back to numbers.
        rename(&mut store, "B", "A'");
        assert_eq!(next_grid_label(&store.view()), "1");
    }
}
