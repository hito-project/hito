//! The Wall tool (`WA`): draws walls as a chain of straight lines (#193).
//!
//! As in Revit, the tool draws a chain: each point after the first ends a
//! wall and starts the next, and Esc ends the chain. Each wall is its own
//! transaction and undo step, and joins the walls it touches as it's
//! placed. Points are clicked in a plan view (#183) or typed on the command
//! line: absolute (`5,0`), or relative to the last point with a
//! typed length (`@4<90`, `@4,0`).
//!
//! Walls go on the active level (the level picked in the project browser,
//! or else the lowest level), up to the next level above it, or 3 m high
//! and unconnected if there's none, as Revit's options bar starts. They use
//! the project's first wall type until the type selector (#26) exists.

use hito_architectural::wall::{self, Placement, Top};
use hito_architectural::wall_type::WALL_TYPE;
use hito_core::{Element, ElementId, StoreError, StoreView, Transaction};
use hito_structural::level::{self, LEVEL};

use crate::command::{Context, Input, Next, Prompt, Tool, Wants};
use crate::project;

/// Starts the Wall tool.
pub fn start() -> Box<dyn Tool> {
    Box::new(WallTool::default())
}

#[derive(Debug, Default, Clone, Copy)]
struct WallTool {
    level: Option<ElementId>,
    start: Option<[f64; 3]>,
    end: Option<[f64; 3]>,
}

impl Tool for WallTool {
    fn start(&mut self, context: &Context<'_>) {
        self.level = context.level;
    }

    fn prompt(&self) -> Prompt {
        match self.start {
            None => Prompt {
                message: "wall-prompt-start",
                wants: Wants::Point,
            },
            Some(start) => Prompt {
                message: "wall-prompt-next",
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
            Some(start) if plan_distance(start, point) <= wall::TOLERANCE => {
                Next::Reject("wall-same-point")
            }
            Some(_) => {
                self.end = Some(point);
                Next::Repeat
            }
        }
    }

    /// The next wall of the chain starts where this one ends.
    fn after_commit(&self) -> Option<Box<dyn Tool>> {
        Some(Box::new(WallTool {
            level: self.level,
            start: self.end,
            end: None,
        }))
    }

    fn commit(self: Box<Self>, tx: &mut Transaction<'_>) -> Result<(), StoreError> {
        let (Some(start), Some(end)) = (self.start, self.end) else {
            unreachable!("committed after both points");
        };
        let model = project::physical_model(tx)?;
        let view = tx.view();
        // Without a level or a wall type, the command fails and says so.
        let base = base_level(&view, self.level)
            .ok_or_else(|| StoreError::UnknownClass(hito_structural::class(LEVEL)))?;
        let wall_type = default_type(&view)
            .ok_or_else(|| StoreError::UnknownClass(hito_architectural::class(WALL_TYPE)))?;
        let top = match level_above(&view, base) {
            Some(level) => Top::Level { level, offset: 0.0 },
            None => Top::Unconnected { height: 3.0 },
        };
        let plan = |p: [f64; 3]| [p[0], p[1]];
        let placement = Placement::new(wall_type, base, plan(start), plan(end)).with_top(top);
        wall::insert(tx, model, &placement).map(drop)
    }
}

fn plan_distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The level walls go on: the active one if it still exists, or else the
/// lowest.
fn base_level(view: &StoreView<'_>, active: Option<ElementId>) -> Option<ElementId> {
    let levels = level::levels(view);
    let exists = |id: &ElementId| levels.iter().any(|(l, _)| l == id);
    active
        .filter(exists)
        .or_else(|| levels.first().map(|(id, _)| *id))
}

/// The lowest level above `base`, if any.
fn level_above(view: &StoreView<'_>, base: ElementId) -> Option<ElementId> {
    let levels = level::levels(view);
    let (_, elevation) = levels.iter().find(|(id, _)| *id == base)?;
    let above = levels
        .iter()
        .find(|(_, e)| *e > elevation + wall::TOLERANCE);
    above.map(|(id, _)| *id)
}

/// The wall type new walls get: the project's first.
fn default_type(view: &StoreView<'_>) -> Option<ElementId> {
    let registry = view.registry();
    let class = hito_architectural::class(WALL_TYPE);
    view.elements()
        .filter(|e| registry.is_a(e.class(), &class))
        .map(Element::id)
        .min()
}

#[cfg(test)]
mod tests {
    use hito_architectural::footprint::End;
    use hito_architectural::join;
    use hito_core::{ElementStore, SchemaRegistry, Value};

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
        /// A new project, as the shell makes one, and the Wall command.
        fn new() -> Self {
            let i18n = I18n::new(Locale::EsAr);
            let mut schemas = SchemaRegistry::new();
            hito_architectural::register(&mut schemas).unwrap();
            let store = project::new_project(schemas, &i18n);
            let mut registry = Registry::new();
            let command = Command {
                name: "WALL",
                label: "command-wall",
                alias: Some("WA"),
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

        fn walls(&self) -> Vec<ElementId> {
            let registry = self.store.registry();
            let class = hito_architectural::class(wall::WALL);
            let mut walls: Vec<_> = self
                .store
                .elements()
                .filter(|e| registry.is_a(e.class(), &class))
                .map(Element::id)
                .collect();
            walls.sort();
            walls
        }

        fn line(&self, wall: ElementId) -> ([f64; 2], [f64; 2]) {
            wall::line(&self.store.view(), wall).unwrap()
        }

        fn add_level(&mut self, name: &str, elevation: f64) -> ElementId {
            let mut tx = self.store.transaction();
            let model = project::physical_model(&mut tx).unwrap();
            let level = Element::new(hito_structural::class(LEVEL), model)
                .with_property(level::NAME, name)
                .with_property(level::ELEVATION, elevation);
            let id = tx.insert(level).unwrap();
            tx.commit().unwrap();
            id
        }
    }

    fn close(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    #[test]
    fn walls_are_drawn_as_a_chain_until_esc() {
        let mut f = Fixture::new();
        f.enter("WA");
        assert_eq!(f.session.prompt().unwrap().message, "wall-prompt-start");
        assert!(matches!(f.enter("0,0"), Outcome::Nothing));
        let Outcome::Committed(changes) = f.enter("@6<0") else {
            panic!("committed");
        };
        assert_eq!(changes.command().map(|c| c.name()), Some("WALL"));
        // The chain goes on from the last point.
        assert_eq!(
            f.session.prompt().unwrap().wants,
            Wants::PointFrom([6.0, 0.0, 0.0])
        );
        assert!(matches!(f.enter("@4 m<90"), Outcome::Committed(_)));
        assert!(matches!(f.enter("@-6,0"), Outcome::Committed(_)));
        assert!(matches!(f.enter("0,0"), Outcome::Committed(_)));
        f.session.cancel(&f.i18n);
        assert!(f.session.running().is_none());

        let walls = f.walls();
        assert_eq!(walls.len(), 4);
        let corners = [[0.0, 0.0], [6.0, 0.0], [6.0, 4.0], [0.0, 4.0], [0.0, 0.0]];
        for (wall, pair) in walls.iter().zip(corners.windows(2)) {
            let (start, end) = f.line(*wall);
            assert!(
                close(start, pair[0]) && close(end, pair[1]),
                "{start:?} {end:?}"
            );
        }
        // Each corner joined, the last one closing the ring.
        let view = f.store.view();
        for (i, &wall) in walls.iter().enumerate() {
            let next = walls[(i + 1) % walls.len()];
            assert_eq!(join::joined_at(&view, wall, End::End), [next]);
        }
    }

    #[test]
    fn a_point_on_the_last_one_is_refused() {
        let mut f = Fixture::new();
        f.enter("WA");
        f.enter("1,1");
        assert!(matches!(f.enter("@0,0"), Outcome::Nothing));
        assert!(f.session.message().is_some());
        assert_eq!(
            f.session.prompt().unwrap().wants,
            Wants::PointFrom([1.0, 1.0, 0.0])
        );
        assert!(f.walls().is_empty());
    }

    #[test]
    fn walls_go_on_the_active_level_up_to_the_next_one() {
        let mut f = Fixture::new();
        let ground = level::levels(&f.store.view())[0].0;

        // With one level, walls are 3 m and unconnected.
        f.enter("WA");
        f.enter("0,0");
        f.enter("5,0");
        f.session.cancel(&f.i18n);
        let first = f.walls()[0];
        let height = f.store.property(first, wall::HEIGHT);
        assert_eq!(height, Some(&Value::Real(3.0)));
        assert_eq!(level::level_of(&f.store.view(), first), Some(ground));

        // With a level picked, walls go on it, up to the next level.
        let second = f.add_level("Nivel 2", 2.8);
        let third = f.add_level("Nivel 3", 5.6);
        f.session.set_level(Some(second));
        f.enter("WA");
        f.enter("0,10");
        f.enter("5,10");
        f.session.cancel(&f.i18n);
        let upper = *f.walls().iter().find(|&&w| w != first).unwrap();
        let view = f.store.view();
        assert_eq!(level::level_of(&view, upper), Some(second));
        assert_eq!(wall::elevations(&view, upper), Some((2.8, 5.6)));
        let top = hito_architectural::class(wall::TOP_LEVEL);
        let tops: Vec<_> = view
            .relationships_from(upper)
            .filter(|r| r.class() == &top)
            .map(|r| r.target())
            .collect();
        assert_eq!(tops, [third]);
    }

    #[test]
    fn new_projects_have_a_generic_wall_type() {
        let f = Fixture::new();
        let view = f.store.view();
        let wall_type = default_type(&view).unwrap();
        assert_eq!(
            f.store
                .property(wall_type, hito_architectural::wall_type::NAME),
            Some(&Value::Text("Genérico - 200 mm".into()))
        );
        let width = hito_architectural::wall_type::width(&view, wall_type);
        assert!((width - 0.2).abs() < 1e-12);
    }

    /// Its layer is the generic concrete, so a generic wall marked
    /// structural is concrete, with no warning (#207).
    #[test]
    fn a_generic_structural_wall_is_concrete() {
        let mut f = Fixture::new();
        let view = f.store.view();
        let wall_type = default_type(&view).unwrap();
        let ground = level::levels(&view)[0].0;
        let placement = Placement::new(wall_type, ground, [0.0, 0.0], [5.0, 0.0]).structural(true);
        let mut tx = f.store.transaction();
        let model = project::physical_model(&mut tx).unwrap();
        let wall = wall::insert(&mut tx, model, &placement).unwrap();
        let changes = tx.commit().unwrap();
        assert!(changes.warnings().is_empty(), "{:?}", changes.warnings());
        let material = wall::structural_material(&f.store.view(), wall).unwrap();
        assert_eq!(
            f.store.property(material, hito_structural::material::NAME),
            Some(&Value::Text("Hormigón".into()))
        );
    }
}
