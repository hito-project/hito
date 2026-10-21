//! The Beam tool (`BM`): draws structural beams between two points (#38).
//!
//! As Revit's, with its "Chain" option off by default, each beam takes a
//! start point and an end point, and the tool stays active until Esc. Each
//! beam is its own transaction and undo step. Points are clicked in a plan
//! view (#183), or typed on the command line: absolute (`5,0`), or the end
//! relative to the start (`@5<0`). A click on or within a few points of a
//! column snaps to its centre, so the end frames into it.
//!
//! Beams go on the active level (the level picked in the project browser,
//! or else the lowest level), with their top at the level, as Revit places
//! them in a plan view. An end on a column that reaches the level (anywhere
//! on its section) frames into the column, at its centre, so the beam
//! follows it. Beams use the project's first beam type until the type
//! selector (#26) exists.

use hito_core::{ElementId, StoreError, StoreView, Transaction};
use hito_structural::beam::{self, BEAM_TYPE, End, Placement};
use hito_structural::level::{self, LEVEL};

use super::placement::{base_level, first_of};
use crate::command::{Context, Input, Next, Prompt, Tool, Wants};
use crate::project;

/// Starts the Beam tool.
pub fn start() -> Box<dyn Tool> {
    Box::new(BeamTool::default())
}

#[derive(Debug, Default, Clone, Copy)]
struct BeamTool {
    level: Option<ElementId>,
    start: Option<[f64; 3]>,
    end: Option<[f64; 3]>,
}

impl Tool for BeamTool {
    fn start(&mut self, context: &Context<'_>) {
        self.level = context.level;
    }

    fn prompt(&self) -> Prompt {
        match self.start {
            None => Prompt {
                message: "beam-prompt-start",
                wants: Wants::Point,
            },
            Some(start) => Prompt {
                message: "beam-prompt-end",
                wants: Wants::PointFrom(start),
            },
        }
    }

    fn input(&mut self, input: Input) -> Next {
        let Input::Point(point) = input else {
            return Next::Reject("input-not-a-point");
        };
        match self.start {
            None => {
                self.start = Some(point);
                Next::Ask
            }
            Some(start) if (start[0] - point[0]).hypot(start[1] - point[1]) <= beam::TOLERANCE => {
                Next::Reject("beam-same-point")
            }
            Some(_) => {
                self.end = Some(point);
                Next::Repeat
            }
        }
    }

    /// The next beam goes on the same level.
    fn after_commit(&self) -> Option<Box<dyn Tool>> {
        Some(Box::new(BeamTool {
            level: self.level,
            ..BeamTool::default()
        }))
    }

    fn commit(self: Box<Self>, tx: &mut Transaction<'_>) -> Result<(), StoreError> {
        let (Some(start), Some(end)) = (self.start, self.end) else {
            unreachable!("committed after both points");
        };
        let model = project::physical_model(tx)?;
        let view = tx.view();
        // Without a level or a beam type, the command fails and says so.
        let level = base_level(&view, self.level)
            .ok_or_else(|| StoreError::UnknownClass(hito_structural::class(LEVEL)))?;
        let beam_type = first_of(&view, &hito_structural::class(BEAM_TYPE))
            .ok_or_else(|| StoreError::UnknownClass(hito_structural::class(BEAM_TYPE)))?;
        let elevation = elevation(&view, level);
        let end_at = |p: [f64; 3]| match beam::column_at(&view, [p[0], p[1]], elevation) {
            Some(column) => End::Column(column),
            None => End::Point([p[0], p[1]]),
        };
        // Both ends on the same column fails, as the rules refuse it.
        let placement = Placement::new(beam_type, level, end_at(start), end_at(end));
        beam::insert(tx, model, &placement).map(drop)
    }
}

fn elevation(view: &StoreView<'_>, level: ElementId) -> f64 {
    let levels = level::levels(view);
    let found = levels.iter().find(|(id, _)| *id == level);
    found.map_or(0.0, |(_, elevation)| *elevation)
}

#[cfg(test)]
mod tests {
    use hito_core::{Element, ElementStore, SchemaRegistry, Value};
    use hito_structural::column::{self, COLUMN_TYPE, Top};
    use hito_structural::section;

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
        /// A new project, as the shell makes one, and the Beam command.
        fn new() -> Self {
            let i18n = I18n::new(Locale::EsAr);
            let mut schemas = SchemaRegistry::new();
            hito_structural::register(&mut schemas).unwrap();
            let store = project::new_project(schemas, &i18n);
            let mut registry = Registry::new();
            let command = Command {
                name: "BEAM",
                label: "command-beam",
                alias: Some("BM"),
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

        fn beams(&self) -> Vec<ElementId> {
            let view = self.store.view();
            let registry = view.registry();
            let class = hito_structural::class(beam::BEAM);
            let mut beams: Vec<_> = view
                .elements()
                .filter(|e| registry.is_a(e.class(), &class))
                .map(Element::id)
                .collect();
            beams.sort();
            beams
        }

        /// A level at `elevation`, and a column on the ground level up to
        /// it at each of `at`.
        fn columns_up_to(
            &mut self,
            elevation: f64,
            at: &[[f64; 2]],
        ) -> (ElementId, Vec<ElementId>) {
            let ground = level::levels(&self.store.view())[0].0;
            let mut tx = self.store.transaction();
            let model = project::physical_model(&mut tx).unwrap();
            let top = Element::new(hito_structural::class(LEVEL), model)
                .with_property(level::NAME, "Nivel 2")
                .with_property(level::ELEVATION, elevation);
            let top = tx.insert(top).unwrap();
            let column_type = first_of(&tx.view(), &hito_structural::class(COLUMN_TYPE)).unwrap();
            let columns = at
                .iter()
                .map(|&point| {
                    let placement =
                        column::Placement::new(column_type, ground, point).with_top(Top::Level {
                            level: top,
                            offset: 0.0,
                        });
                    column::insert(&mut tx, model, &placement).unwrap()
                })
                .collect();
            tx.commit().unwrap();
            (top, columns)
        }
    }

    #[test]
    fn beams_are_drawn_two_points_each_until_esc() {
        let mut f = Fixture::new();
        f.enter("BM");
        assert_eq!(f.session.prompt().unwrap().message, "beam-prompt-start");
        assert!(matches!(f.enter("0,0"), Outcome::Nothing));
        let Outcome::Committed(changes) = f.enter("@5<0") else {
            panic!("committed");
        };
        assert_eq!(changes.command().map(|c| c.name()), Some("BEAM"));
        // Not a chain: the next beam asks for its start again.
        assert_eq!(f.session.prompt().unwrap().wants, Wants::Point);
        f.enter("0,4");
        assert!(matches!(f.enter("5,4"), Outcome::Committed(_)));
        f.session.cancel(&f.i18n);
        assert!(f.session.running().is_none());

        let view = f.store.view();
        let lines: Vec<_> = f
            .beams()
            .into_iter()
            .map(|b| beam::line(&view, b).unwrap())
            .collect();
        assert_eq!(lines, [([0.0, 0.0], [5.0, 0.0]), ([0.0, 4.0], [5.0, 4.0])]);
    }

    #[test]
    fn a_point_on_the_start_is_refused() {
        let mut f = Fixture::new();
        f.enter("BM");
        f.enter("1,1");
        assert!(matches!(f.enter("@0,0"), Outcome::Nothing));
        assert!(f.session.message().is_some());
        assert!(f.beams().is_empty());
    }

    #[test]
    fn ends_on_columns_frame_into_them_on_the_active_level() {
        let mut f = Fixture::new();
        let (top, columns) = f.columns_up_to(3.0, &[[0.0, 0.0], [6.0, 0.0]]);
        f.session.set_level(Some(top));
        f.enter("BM");
        // Off centre, but on the 30 cm columns.
        f.enter("0.1,0.05");
        f.enter("6,-0.1");
        f.session.cancel(&f.i18n);
        let beam = f.beams()[0];
        let view = f.store.view();
        assert_eq!(
            beam::columns(&view, beam),
            [Some(columns[0]), Some(columns[1])]
        );
        assert_eq!(beam::line(&view, beam), Some(([0.0, 0.0], [6.0, 0.0])));
        assert_eq!(
            f.store.property(beam, level::BASE_ELEVATION),
            Some(&Value::Real(3.0))
        );

        // Moving a column moves the beam's end.
        let mut tx = f.store.transaction();
        tx.set_property(columns[1], column::Y, 2.0).unwrap();
        tx.commit().unwrap();
        assert_eq!(
            beam::line(&f.store.view(), beam),
            Some(([0.0, 0.0], [6.0, 2.0]))
        );
    }

    #[test]
    fn on_a_level_the_columns_dont_reach_the_ends_are_free() {
        let mut f = Fixture::new();
        f.columns_up_to(3.0, &[[0.0, 0.0], [6.0, 0.0]]);
        // The active level is the lowest, at the columns' base.
        f.enter("BM");
        f.enter("0,0");
        f.enter("6,0");
        f.session.cancel(&f.i18n);
        let beam = f.beams()[0];
        assert_eq!(beam::columns(&f.store.view(), beam), [None, None]);
    }

    #[test]
    fn new_projects_have_a_generic_beam_type() {
        let f = Fixture::new();
        let view = f.store.view();
        let beam_type = first_of(&view, &hito_structural::class(BEAM_TYPE)).unwrap();
        assert_eq!(
            f.store.property(beam_type, section::NAME),
            Some(&Value::Text("Rectangular - 200 x 400 mm".into()))
        );
        assert_eq!(
            section::of(&view, beam_type),
            Some(section::Section::Rectangle {
                width: 0.2,
                depth: 0.4
            })
        );
        // Of the generic concrete, the same as the generic column type's.
        let column_type = first_of(&view, &hito_structural::class(COLUMN_TYPE)).unwrap();
        let material = section::material_of(&view, beam_type);
        assert!(material.is_some());
        assert_eq!(material, section::material_of(&view, column_type));
    }
}
