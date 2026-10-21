//! The Isolated footing tool: places rectangular footings by their centre
//! (#41).
//!
//! As Revit's Structural Foundation: Isolated, it stays active until Esc,
//! each footing its own transaction and undo step. Footings go on the
//! active level (the level picked in the project browser, or else the
//! lowest level), their top on it, with the project's first footing type
//! until the type selector (#26) exists. Centres are clicked in a plan view
//! (#183) or typed on the command line (`3,4`, or `@6,0` from the last
//! one).
//!
//! A footing placed on a column that stands on the level goes under it,
//! as Revit's "At Columns" does ([`Placement::under`]): it's centred on the
//! column, its top at the column's base, and it follows the column. In
//! plan, a click on or near a column snaps to its centre, so clicking
//! columns one after another places a footing under each.

use hito_core::{ElementId, StoreError, StoreView, Transaction};
use hito_structural::column;
use hito_structural::footing::{self, FOOTING_TYPE, Placement};
use hito_structural::level::LEVEL;

use super::placement;
use crate::command::{Context, Input, Next, Prompt, Tool, Wants};
use crate::project;

/// Starts the Isolated footing tool.
pub fn start() -> Box<dyn Tool> {
    Box::new(FootingTool::default())
}

#[derive(Debug, Default, Clone)]
struct FootingTool {
    level: Option<ElementId>,
    /// What was selected when the tool started: a footing goes under each
    /// column among it, at once.
    selected: Vec<ElementId>,
    /// The last footing's centre, which the next can be typed from.
    last: Option<[f64; 2]>,
    centre: Option<[f64; 2]>,
}

impl FootingTool {
    /// The selected columns.
    fn columns(&self, view: &StoreView<'_>) -> Vec<ElementId> {
        let class = hito_structural::class(column::COLUMN);
        let registry = view.registry();
        let is_column = |&id: &ElementId| {
            view.get(id)
                .is_some_and(|e| registry.is_a(e.class(), &class))
        };
        self.selected.iter().copied().filter(is_column).collect()
    }
}

impl Tool for FootingTool {
    fn start(&mut self, context: &Context<'_>) {
        self.level = context.level;
        self.selected = context.selection.to_vec();
    }

    /// With columns selected, a footing goes under each at once, as
    /// Revit's "At Columns" with the columns picked.
    fn ready_at_start(&self, model: &StoreView<'_>) -> bool {
        !self.columns(model).is_empty()
    }

    fn prompt(&self) -> Prompt {
        let wants = match self.last {
            None => Wants::Point,
            Some([x, y]) => Wants::PointFrom([x, y, 0.0]),
        };
        Prompt {
            message: "footing-prompt-centre",
            wants,
        }
    }

    fn input(&mut self, input: Input) -> Next {
        let Input::Point([x, y, _]) = input else {
            return Next::Reject("input-not-a-point");
        };
        self.centre = Some([x, y]);
        Next::Repeat
    }

    /// The next footing is clicked, on the same level.
    fn after_commit(&self) -> Option<Box<dyn Tool>> {
        Some(Box::new(FootingTool {
            level: self.level,
            last: self.centre,
            ..FootingTool::default()
        }))
    }

    fn commit(self: Box<Self>, tx: &mut Transaction<'_>) -> Result<(), StoreError> {
        let model = project::physical_model(tx)?;
        let view = tx.view();
        // Without a footing type, the command fails and says so.
        let footing_type = default_type(&view)
            .ok_or_else(|| StoreError::UnknownClass(hito_structural::class(FOOTING_TYPE)))?;
        let Some(centre) = self.centre else {
            // Started with columns selected: one under each.
            for column in self.columns(&view) {
                footing::insert(tx, model, &Placement::under(footing_type, column))?;
            }
            return Ok(());
        };
        // Without a level, the command fails and says so.
        let level = placement::base_level(&view, self.level)
            .ok_or_else(|| StoreError::UnknownClass(hito_structural::class(LEVEL)))?;
        // On a column standing on the level, it goes under the column, as
        // Revit's "At Columns", and follows it.
        let placement = match column::standing_at(&view, level, centre) {
            Some(column) => Placement::under(footing_type, column),
            None => Placement::at(footing_type, level, centre),
        };
        footing::insert(tx, model, &placement).map(drop)
    }
}

/// The footing type new footings get: the project's first.
fn default_type(view: &StoreView<'_>) -> Option<ElementId> {
    placement::first_of(view, &hito_structural::class(FOOTING_TYPE))
}

#[cfg(test)]
mod tests {
    use hito_core::{ElementId, ElementStore, SchemaRegistry, Value};
    use hito_structural::column::COLUMN_TYPE;
    use hito_structural::level;

    use super::*;
    use crate::command::{Command, InputFormat, Outcome, Registry, Run, Session};
    use crate::i18n::{I18n, Locale};

    struct Fixture {
        session: Session,
        i18n: I18n,
        format: InputFormat,
        store: ElementStore,
    }

    impl Fixture {
        fn new() -> Self {
            let i18n = I18n::new(Locale::EsAr);
            let mut schemas = SchemaRegistry::new();
            hito_structural::register(&mut schemas).unwrap();
            let store = project::new_project(schemas, &i18n);
            let mut registry = Registry::new();
            let command = Command {
                name: "ISOLATEDFOOTING",
                label: "command-isolated-footing",
                alias: None,
                run: Run::Tool(start),
            };
            registry.register(command).unwrap();
            Self {
                session: Session::new(registry),
                i18n,
                format: InputFormat::new(),
                store,
            }
        }

        fn enter(&mut self, text: &str) -> Outcome {
            let Self {
                session,
                i18n,
                format,
                store,
            } = self;
            session.enter(text, i18n, format, store)
        }
    }

    #[test]
    fn footings_are_placed_by_their_centre_until_esc() {
        let mut f = Fixture::new();
        f.enter("ISOLATEDFOOTING");
        assert_eq!(f.session.prompt().unwrap().wants, Wants::Point);
        let Outcome::Committed(changes) = f.enter("3,4") else {
            panic!("committed");
        };
        assert_eq!(changes.command().map(|c| c.name()), Some("ISOLATEDFOOTING"));
        // The next one can be typed from the last.
        assert_eq!(
            f.session.prompt().unwrap().wants,
            Wants::PointFrom([3.0, 4.0, 0.0])
        );
        assert!(matches!(f.enter("@6,0"), Outcome::Committed(_)));
        f.session.cancel(&f.i18n);

        let view = f.store.view();
        let footings = footing::footings(&view);
        assert_eq!(footings.len(), 2);
        assert_eq!(footing::centre(&view, footings[0]), Some([3.0, 4.0]));
        assert_eq!(footing::centre(&view, footings[1]), Some([9.0, 4.0]));
        let ground = level::levels(&view)[0].0;
        assert_eq!(level::level_of(&view, footings[0]), Some(ground));
        let (bottom, top) = footing::elevations(&view, footings[0]).unwrap();
        assert!(top.abs() < 1e-12 && (bottom + 0.45).abs() < 1e-12);
    }

    /// A column on the ground floor at `at`, 3 m high.
    fn insert_column(f: &mut Fixture, at: [f64; 2]) -> ElementId {
        let view = f.store.view();
        let ground = level::levels(&view)[0].0;
        let column_type = placement::first_of(&view, &hito_structural::class(COLUMN_TYPE));
        let placement = column::Placement::new(column_type.unwrap(), ground, at)
            .with_top(column::Top::Unconnected { height: 3.0 });
        let mut tx = f.store.transaction();
        let model = project::physical_model(&mut tx).unwrap();
        let column = column::insert(&mut tx, model, &placement).unwrap();
        tx.commit().unwrap();
        column
    }

    #[test]
    fn started_with_columns_selected_it_places_one_under_each_at_once() {
        let mut f = Fixture::new();
        let a = insert_column(&mut f, [0.0, 0.0]);
        let b = insert_column(&mut f, [6.0, 0.0]);
        let ground = level::levels(&f.store.view())[0].0;
        // A level among the selection is left out.
        f.session.set_selection([a, ground, b]);
        let Outcome::Committed(changes) = f.enter("ISOLATEDFOOTING") else {
            panic!("committed at once");
        };
        assert_eq!(changes.command().map(|c| c.name()), Some("ISOLATEDFOOTING"));
        let view = f.store.view();
        let mut under: Vec<_> = footing::footings(&view)
            .into_iter()
            .map(|id| footing::column(&view, id))
            .collect();
        under.sort();
        let mut expected = [Some(a), Some(b)];
        expected.sort();
        assert_eq!(under, expected, "one transaction, one under each");

        // It carries on, for footings clicked or typed.
        assert_eq!(f.session.prompt().unwrap().wants, Wants::Point);
        assert!(matches!(f.enter("3,4"), Outcome::Committed(_)));
        assert_eq!(footing::footings(&f.store.view()).len(), 3);
    }

    #[test]
    fn started_without_columns_selected_it_waits_for_a_point() {
        let mut f = Fixture::new();
        let ground = level::levels(&f.store.view())[0].0;
        f.session.set_selection([ground]);
        assert!(matches!(f.enter("ISOLATEDFOOTING"), Outcome::Nothing));
        assert!(footing::footings(&f.store.view()).is_empty());
        assert_eq!(f.session.prompt().unwrap().wants, Wants::Point);
    }

    #[test]
    fn a_footing_placed_on_a_column_goes_under_it() {
        let mut f = Fixture::new();
        let view = f.store.view();
        let ground = level::levels(&view)[0].0;
        let column_type = placement::first_of(&view, &hito_structural::class(COLUMN_TYPE));
        let placement = column::Placement::new(column_type.unwrap(), ground, [3.0, 4.0])
            .with_top(column::Top::Unconnected { height: 3.0 });
        let mut tx = f.store.transaction();
        let model = project::physical_model(&mut tx).unwrap();
        let column = column::insert(&mut tx, model, &placement).unwrap();
        tx.commit().unwrap();

        f.enter("ISOLATEDFOOTING");
        // Off the column's centre, but on it; then away from it.
        assert!(matches!(f.enter("3.1,4.1"), Outcome::Committed(_)));
        assert!(matches!(f.enter("8,8"), Outcome::Committed(_)));
        f.session.cancel(&f.i18n);
        let view = f.store.view();
        let footings = footing::footings(&view);
        assert_eq!(footings.len(), 2);
        assert_eq!(footing::column(&view, footings[0]), Some(column));
        assert_eq!(footing::centre(&view, footings[0]), Some([3.0, 4.0]));
        assert_eq!(footing::column(&view, footings[1]), None, "free");
        assert_eq!(footing::centre(&view, footings[1]), Some([8.0, 8.0]));
    }

    #[test]
    fn new_projects_have_a_rectangular_footing_type() {
        let f = Fixture::new();
        let view = f.store.view();
        let footing_type = default_type(&view).unwrap();
        assert_eq!(
            f.store.property(footing_type, footing::NAME),
            Some(&Value::Text("Rectangular - 1800 x 1200 x 450 mm".into()))
        );
        let size = [footing::LENGTH, footing::WIDTH, footing::THICKNESS]
            .map(|name| f.store.property(footing_type, name).cloned());
        let real = |x| Some(Value::Real(x));
        assert_eq!(size, [real(1.8), real(1.2), real(0.45)]);
        // Of the generic concrete, as the other generic types.
        let concrete = footing::material(&view, footing_type).unwrap();
        assert_eq!(
            f.store.property(concrete, hito_structural::material::NAME),
            Some(&Value::Text("Hormigón".into()))
        );
    }
}
