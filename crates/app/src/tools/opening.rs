//! The Door (`DR`) and Window (`WN`) tools: place doors and windows in
//! walls (#194).
//!
//! As in Revit, the tool takes a point on a wall: the wall there becomes
//! the host, the door or window is centred on the point along the wall,
//! and a door swings to the side of the wall the point is on. The tool
//! stays active for the next one until Esc, and each door or window is its
//! own transaction and undo step.
//!
//! The point is clicked on a wall in a plan view (#183), or typed on the
//! command line: both give the same [`Input::Point`], on the active level.
//! The plan view then draws the door's or window's plan symbol.
//!
//! The wall is the nearest one on the active level whose body holds the
//! point (see [`opening::wall_at`]). If the door or window would stick out
//! of the wall's end, it's moved along until it fits, as Revit keeps it on
//! its host. New doors and windows use the first type of their kind, by
//! name, until the type selector (#26) exists.

use hito_architectural::opening::{self, Placement};
use hito_architectural::opening_type::{self, Kind};
use hito_architectural::wall::{self, WALL};
use hito_core::{ElementId, StoreError, Transaction};

use crate::command::{Context, Input, Next, Prompt, Tool, Wants};
use crate::project;

/// Starts the Door tool.
pub fn start_door() -> Box<dyn Tool> {
    Box::new(OpeningTool::new(Kind::Door))
}

/// Starts the Window tool.
pub fn start_window() -> Box<dyn Tool> {
    Box::new(OpeningTool::new(Kind::Window))
}

#[derive(Debug, Clone, Copy)]
struct OpeningTool {
    kind: Kind,
    level: Option<ElementId>,
    point: Option<[f64; 2]>,
}

impl OpeningTool {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            level: None,
            point: None,
        }
    }
}

impl Tool for OpeningTool {
    fn start(&mut self, context: &Context<'_>) {
        self.level = context.level;
    }

    fn prompt(&self) -> Prompt {
        let message = match self.kind {
            Kind::Door => "door-prompt",
            Kind::Window => "window-prompt",
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
        self.point = Some([x, y]);
        Next::Repeat
    }

    fn after_commit(&self) -> Option<Box<dyn Tool>> {
        Some(Box::new(OpeningTool {
            point: None,
            ..*self
        }))
    }

    fn commit(self: Box<Self>, tx: &mut Transaction<'_>) -> Result<(), StoreError> {
        let point = self.point.expect("committed after a point");
        let model = project::physical_model(tx)?;
        let view = tx.view();
        // Without a wall there or a type, the command fails and says so.
        let no_wall = || StoreError::UnknownClass(hito_architectural::class(WALL));
        let wall = opening::wall_at(&view, self.level, point).ok_or_else(no_wall)?;
        let opening_type = *opening_type::types(&view, self.kind)
            .first()
            .ok_or_else(|| StoreError::UnknownClass(self.kind.type_class()))?;
        let on = opening::on_wall(&view, wall, point).ok_or_else(no_wall)?;
        let width = opening_type::size(&view, opening_type).map_or(0.0, |s| s.width);
        let length = wall::line(&view, wall).map_or(0.0, |(a, b)| (b[0] - a[0]).hypot(b[1] - a[1]));
        let distance_along = fit(on.distance_along, width, length);
        // Unflipped, a door swings to the wall's interior side.
        let placement =
            Placement::new(opening_type, wall, distance_along).facing_flipped(!on.interior_side);
        opening::insert(tx, model, &placement).map(drop)
    }
}

/// Where the centre of something `width` wide goes, near `along`, so it
/// stays within a wall `length` long. A wall too short keeps `along`, and
/// the domain refuses the door, so the command says it couldn't place it.
fn fit(along: f64, width: f64, length: f64) -> f64 {
    let half = width / 2.0;
    if length < width {
        return along;
    }
    along.clamp(half, length - half)
}

#[cfg(test)]
mod tests {
    use hito_core::{Element, ElementStore, SchemaRegistry, Value};

    use super::*;
    use crate::command::{Command, InputFormat, Outcome, Registry, Run, Session};
    use crate::i18n::{I18n, Locale};
    use crate::tools;

    struct Fixture {
        session: Session,
        i18n: I18n,
        format: InputFormat,
        store: ElementStore,
    }

    impl Fixture {
        /// A new project, as the shell makes one, with the Wall, Door and
        /// Window commands, and a 6 m wall from 0,0 to 6,0.
        fn new() -> Self {
            let i18n = I18n::new(Locale::EsAr);
            let mut schemas = SchemaRegistry::new();
            hito_architectural::register(&mut schemas).unwrap();
            let store = project::new_project(schemas, &i18n);
            let mut registry = Registry::new();
            let commands = [
                (
                    "WALL",
                    "command-wall",
                    "WA",
                    tools::wall::start as fn() -> _,
                ),
                ("DOOR", "command-door", "DR", start_door),
                ("WINDOW", "command-window", "WN", start_window),
            ];
            for (name, label, alias, start) in commands {
                let command = Command {
                    name,
                    label,
                    alias: Some(alias),
                    run: Run::Tool(start),
                };
                registry.register(command).unwrap();
            }
            let mut f = Self {
                session: Session::new(registry),
                i18n,
                format: InputFormat::new(),
                store,
            };
            f.enter("WA");
            f.enter("0,0");
            f.enter("6,0");
            f.session.cancel(&f.i18n);
            f
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

        fn wall(&self) -> ElementId {
            let class = hito_architectural::class(WALL);
            let registry = self.store.registry();
            self.store
                .elements()
                .find(|e| registry.is_a(e.class(), &class))
                .map(Element::id)
                .unwrap()
        }

        fn hosted(&self) -> Vec<ElementId> {
            opening::hosted(&self.store.view(), self.wall())
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn doors_go_in_the_wall_at_the_point_until_esc() {
        let mut f = Fixture::new();
        f.enter("DR");
        assert_eq!(f.session.prompt().unwrap().message, "door-prompt");
        let Outcome::Committed(changes) = f.enter("1.5,0") else {
            panic!("committed");
        };
        assert_eq!(changes.command().map(|c| c.name()), Some("DOOR"));
        // Still placing doors, each its own undo step.
        assert!(matches!(f.enter("4,-0.05"), Outcome::Committed(_)));
        f.session.cancel(&f.i18n);

        let view = f.store.view();
        let doors = f.hosted();
        assert_eq!(doors.len(), 2);
        let mut cuts: Vec<_> = doors
            .iter()
            .map(|&d| opening::cut(&view, d).unwrap())
            .collect();
        cuts.sort_by(|a, b| a.from.total_cmp(&b.from));
        let size = opening_type::size(&view, view.get(doors[0]).unwrap().type_id().unwrap());
        let half = size.unwrap().width / 2.0;
        assert!(close(cuts[0].from, 1.5 - half) && close(cuts[1].from, 4.0 - half));
        let class = hito_architectural::class(opening::DOOR);
        assert!(
            doors
                .iter()
                .all(|&d| view.get(d).unwrap().class() == &class)
        );
    }

    #[test]
    fn a_door_swings_to_the_side_of_the_point() {
        let mut f = Fixture::new();
        // The wall runs along x: its exterior is +y, its interior -y.
        f.enter("DR");
        f.enter("2,-0.05");
        f.enter("4,0.05");
        f.session.cancel(&f.i18n);
        let view = f.store.view();
        let mut flips: Vec<_> = f
            .hosted()
            .into_iter()
            .map(|d| {
                let along = opening::cut(&view, d).unwrap().from;
                let flipped = view.property(d, opening::FACING_FLIPPED);
                (along, flipped == Some(&Value::Boolean(true)))
            })
            .collect();
        flips.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(flips.iter().map(|f| f.1).collect::<Vec<_>>(), [false, true]);
    }

    #[test]
    fn windows_stand_on_their_types_sill_and_fit_in_the_wall() {
        let mut f = Fixture::new();
        f.enter("WN");
        assert_eq!(f.session.prompt().unwrap().message, "window-prompt");
        // Near the end: moved along until it fits.
        assert!(matches!(f.enter("5.9,0"), Outcome::Committed(_)));
        let view = f.store.view();
        let [window] = f.hosted()[..] else {
            panic!("one window")
        };
        let size = opening_type::size(&view, view.get(window).unwrap().type_id().unwrap()).unwrap();
        let cut = opening::cut(&view, window).unwrap();
        assert!(close(cut.to, 6.0));
        assert!(close(cut.bottom, size.sill_height) && size.sill_height > 0.0);
        assert!(view.get(window).unwrap().class() == &hito_architectural::class(opening::WINDOW));
    }

    #[test]
    fn a_point_off_any_wall_places_nothing() {
        let mut f = Fixture::new();
        f.enter("DR");
        assert!(matches!(f.enter("3,2"), Outcome::Nothing));
        assert!(f.session.message().is_some(), "says it couldn't");
        assert!(f.hosted().is_empty());
        assert_eq!(f.session.running().map(|c| c.name), Some("DOOR"));
    }

    #[test]
    fn a_wall_too_short_for_the_door_is_refused() {
        let mut f = Fixture::new();
        f.enter("WA");
        f.enter("0,5");
        f.enter("0.5,5");
        f.session.cancel(&f.i18n);
        f.enter("DR");
        assert!(matches!(f.enter("0.25,5"), Outcome::Nothing));
        assert!(f.session.message().is_some(), "says it couldn't");
        let door = hito_architectural::class(opening::DOOR);
        let registry = f.store.registry();
        assert!(!f.store.elements().any(|e| registry.is_a(e.class(), &door)));
    }

    #[test]
    fn deleting_the_wall_deletes_its_doors() {
        let mut f = Fixture::new();
        f.enter("DR");
        f.enter("3,0");
        f.session.cancel(&f.i18n);
        let door = f.hosted()[0];
        let wall = f.wall();
        let mut tx = f.store.transaction();
        tx.delete(wall).unwrap();
        tx.commit().unwrap();
        assert!(f.store.get(door).is_none());
    }

    #[test]
    fn fitting_keeps_it_inside_the_wall() {
        assert_eq!(fit(0.1, 0.8, 6.0), 0.4);
        assert_eq!(fit(5.9, 0.8, 6.0), 5.6);
        assert_eq!(fit(3.0, 0.8, 6.0), 3.0);
        assert_eq!(
            fit(0.2, 0.8, 0.5),
            0.2,
            "too short: left for the domain to refuse"
        );
    }
}
