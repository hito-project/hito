//! The Isolated footing tool: places rectangular footings by their centre
//! (#41).
//!
//! As Revit's Structural Foundation: Isolated, it stays active until Esc,
//! each footing its own transaction and undo step. Footings go on the
//! active level (the level picked in the project browser, or else the
//! lowest level), their top on it, with the project's first footing type
//! until the type selector (#26) exists. Centres are typed on the command
//! line (`3,4`, or `@6,0` from the last one) until plan views (#183) let
//! them be clicked.
//!
//! Placing footings under columns ("At Columns" in Revit) needs columns
//! (#37) and picking them, so it comes with them; the domain already
//! places a footing under a column and keeps it there
//! ([`Placement::under`]).

use hito_core::{ElementId, StoreError, StoreView, Transaction};
use hito_structural::footing::{self, FOOTING_TYPE, Placement};
use hito_structural::level::LEVEL;

use super::placement;
use crate::command::{Context, Input, Next, Prompt, Tool, Wants};
use crate::project;

/// Starts the Isolated footing tool.
pub fn start() -> Box<dyn Tool> {
    Box::new(FootingTool::default())
}

#[derive(Debug, Default, Clone, Copy)]
struct FootingTool {
    level: Option<ElementId>,
    /// The last footing's centre, which the next can be typed from.
    last: Option<[f64; 2]>,
    centre: Option<[f64; 2]>,
}

impl Tool for FootingTool {
    fn start(&mut self, context: &Context<'_>) {
        self.level = context.level;
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

    fn after_commit(&self) -> Option<Box<dyn Tool>> {
        Some(Box::new(FootingTool {
            level: self.level,
            last: self.centre,
            centre: None,
        }))
    }

    fn commit(self: Box<Self>, tx: &mut Transaction<'_>) -> Result<(), StoreError> {
        let centre = self.centre.expect("committed after the centre");
        let model = project::physical_model(tx)?;
        let view = tx.view();
        // Without a level or a footing type, the command fails and says so.
        let level = placement::base_level(&view, self.level)
            .ok_or_else(|| StoreError::UnknownClass(hito_structural::class(LEVEL)))?;
        let footing_type = default_type(&view)
            .ok_or_else(|| StoreError::UnknownClass(hito_structural::class(FOOTING_TYPE)))?;
        footing::insert(tx, model, &Placement::at(footing_type, level, centre)).map(drop)
    }
}

/// The footing type new footings get: the project's first.
fn default_type(view: &StoreView<'_>) -> Option<ElementId> {
    placement::first_of(view, &hito_structural::class(FOOTING_TYPE))
}

#[cfg(test)]
mod tests {
    use hito_core::{ElementStore, SchemaRegistry, Value};
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
