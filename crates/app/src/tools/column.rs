//! The Column tool (`CL`): places structural columns, one per point (#37).
//!
//! As Revit's, the tool stays active until Esc, and each column is its own
//! transaction and undo step. Points are typed on the command line until
//! plan views (#183) let them be clicked. A point on a grid intersection
//! (within 1 mm) places the column on the two grids, so it moves when they
//! move, as Revit's "Moves With Grids".
//!
//! Columns go on the active level (the level picked in the project
//! browser, or else the lowest level), up to the next level above it, or
//! 3 m high and unconnected if there's none, as Revit's options bar starts
//! with "Height". They use the project's first column type until the type
//! selector (#26) exists.

use hito_core::{ElementId, StoreError, Transaction};
use hito_structural::column::{self, COLUMN_TYPE, Placement, Top};
use hito_structural::grid;
use hito_structural::level::LEVEL;

use super::placement::{base_level, first_of, level_above};
use crate::command::{Context, Input, Next, Prompt, Tool, Wants};
use crate::project;

/// Starts the Column tool.
pub fn start() -> Box<dyn Tool> {
    Box::new(ColumnTool::default())
}

#[derive(Debug, Default, Clone, Copy)]
struct ColumnTool {
    level: Option<ElementId>,
    point: Option<[f64; 2]>,
}

impl Tool for ColumnTool {
    fn start(&mut self, context: &Context<'_>) {
        self.level = context.level;
    }

    fn prompt(&self) -> Prompt {
        Prompt {
            message: "column-prompt-point",
            wants: Wants::Point,
        }
    }

    fn input(&mut self, input: Input) -> Next {
        let Input::Point([x, y, _]) = input else {
            return Next::Reject("input-not-a-point");
        };
        self.point = Some([x, y]);
        Next::Repeat
    }

    /// The next column goes on the same level.
    fn after_commit(&self) -> Option<Box<dyn Tool>> {
        Some(Box::new(ColumnTool {
            level: self.level,
            point: None,
        }))
    }

    fn commit(self: Box<Self>, tx: &mut Transaction<'_>) -> Result<(), StoreError> {
        let point = self.point.expect("committed after a point");
        let model = project::physical_model(tx)?;
        let view = tx.view();
        // Without a level or a column type, the command fails and says so.
        let base = base_level(&view, self.level)
            .ok_or_else(|| StoreError::UnknownClass(hito_structural::class(LEVEL)))?;
        let column_type = first_of(&view, &hito_structural::class(COLUMN_TYPE))
            .ok_or_else(|| StoreError::UnknownClass(hito_structural::class(COLUMN_TYPE)))?;
        let top = match level_above(&view, base) {
            Some(level) => Top::Level { level, offset: 0.0 },
            None => Top::Unconnected { height: 3.0 },
        };
        let mut placement = Placement::new(column_type, base, point).with_top(top);
        if let Some(crossing) = grid::intersection_near(&view, point, column::TOLERANCE) {
            placement.location = crossing.point;
            placement = placement.on_grids(crossing.grids);
        }
        column::insert(tx, model, &placement).map(drop)
    }
}

#[cfg(test)]
mod tests {
    use hito_core::{Element, ElementStore, SchemaRegistry, Value};
    use hito_structural::{level, section};

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
        /// A new project, as the shell makes one, and the Column command.
        fn new() -> Self {
            let i18n = I18n::new(Locale::EsAr);
            let mut schemas = SchemaRegistry::new();
            hito_structural::register(&mut schemas).unwrap();
            let store = project::new_project(schemas, &i18n);
            let mut registry = Registry::new();
            let command = Command {
                name: "COLUMN",
                label: "command-column",
                alias: Some("CL"),
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

        fn columns(&self) -> Vec<ElementId> {
            column::columns(&self.store.view())
        }

        fn add(&mut self, element: Element) -> ElementId {
            let mut tx = self.store.transaction();
            let id = tx.insert(element).unwrap();
            tx.commit().unwrap();
            id
        }

        fn add_level(&mut self, name: &str, elevation: f64) -> ElementId {
            let model = self.model();
            self.add(
                Element::new(hito_structural::class(LEVEL), model)
                    .with_property(level::NAME, name)
                    .with_property(level::ELEVATION, elevation),
            )
        }

        fn add_grid(&mut self, label: &str, start: [f64; 2], end: [f64; 2]) -> ElementId {
            let model = self.model();
            self.add(grid::GridLine::new(start, end).element(model, label))
        }

        fn model(&mut self) -> hito_core::ModelId {
            let mut tx = self.store.transaction();
            let model = project::physical_model(&mut tx).unwrap();
            tx.roll_back();
            model
        }
    }

    #[test]
    fn columns_are_placed_one_per_point_until_esc() {
        let mut f = Fixture::new();
        f.enter("CL");
        assert_eq!(f.session.prompt().unwrap().message, "column-prompt-point");
        let Outcome::Committed(changes) = f.enter("0,0") else {
            panic!("committed");
        };
        assert_eq!(changes.command().map(|c| c.name()), Some("COLUMN"));
        assert!(matches!(f.enter("5,0"), Outcome::Committed(_)));
        assert!(matches!(f.enter("5,4"), Outcome::Committed(_)));
        f.session.cancel(&f.i18n);
        assert!(f.session.running().is_none());

        let view = f.store.view();
        let locations: Vec<_> = f
            .columns()
            .into_iter()
            .map(|c| column::location(&view, c).unwrap())
            .collect();
        assert_eq!(locations, [[0.0, 0.0], [5.0, 0.0], [5.0, 4.0]]);
    }

    #[test]
    fn columns_go_on_the_active_level_up_to_the_next_one() {
        let mut f = Fixture::new();
        let ground = level::levels(&f.store.view())[0].0;
        f.enter("CL");
        f.enter("0,0");
        f.session.cancel(&f.i18n);
        let first = f.columns()[0];
        assert_eq!(
            f.store.property(first, column::HEIGHT),
            Some(&Value::Real(3.0))
        );
        assert_eq!(level::level_of(&f.store.view(), first), Some(ground));

        let second = f.add_level("Nivel 2", 2.8);
        f.add_level("Nivel 3", 5.6);
        f.session.set_level(Some(second));
        f.enter("CL");
        f.enter("1,0");
        f.session.cancel(&f.i18n);
        let upper = f.columns()[1];
        let view = f.store.view();
        assert_eq!(level::level_of(&view, upper), Some(second));
        assert_eq!(column::elevations(&view, upper), Some((2.8, 5.6)));
    }

    #[test]
    fn a_column_on_a_grid_intersection_follows_the_grids() {
        let mut f = Fixture::new();
        let a = f.add_grid("A", [0.0, -1.0], [0.0, 10.0]);
        let one = f.add_grid("1", [-1.0, 2.0], [10.0, 2.0]);
        f.enter("CL");
        f.enter("0.0004,2");
        f.enter("3,3");
        f.session.cancel(&f.i18n);
        let columns = f.columns();
        let view = f.store.view();
        let mut grids = column::grids(&view, columns[0]).expect("on the grids");
        grids.sort();
        assert_eq!(grids, [a, one]);
        assert_eq!(column::location(&view, columns[0]), Some([0.0, 2.0]));
        assert_eq!(column::grids(&view, columns[1]), None);

        let mut tx = f.store.transaction();
        tx.set_property(a, grid::START_X, 1.5).unwrap();
        tx.set_property(a, grid::END_X, 1.5).unwrap();
        tx.commit().unwrap();
        assert_eq!(
            column::location(&f.store.view(), columns[0]),
            Some([1.5, 2.0])
        );
    }

    #[test]
    fn new_projects_have_a_generic_column_type() {
        let f = Fixture::new();
        let view = f.store.view();
        let column_type = first_of(&view, &hito_structural::class(COLUMN_TYPE)).unwrap();
        assert_eq!(
            f.store.property(column_type, section::NAME),
            Some(&Value::Text("Rectangular - 300 x 300 mm".into()))
        );
        assert_eq!(
            section::of(&view, column_type),
            Some(section::Section::Rectangle {
                width: 0.3,
                depth: 0.3
            })
        );
        // Of the generic concrete.
        let material = section::material_of(&view, column_type).expect("a material");
        assert_eq!(
            f.store.property(material, hito_structural::material::NAME),
            Some(&Value::Text("Hormigón".into()))
        );
    }
}
