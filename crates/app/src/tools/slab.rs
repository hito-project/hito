//! The Slab tool (`SB`) and the Vertical opening tool: sketch a closed
//! boundary, corner by corner (#39).
//!
//! As in Revit's sketch mode, the boundary is drawn as lines: each point
//! after the first ends one line and starts the next, and typing the first
//! point again closes the boundary and finishes the slab. A point that
//! would make the boundary cross or touch itself is refused as it's typed.
//! Points are clicked in a plan view (#183) or typed on the command line:
//! absolute (`5,0`), or relative to the last point (`@4<90`, `@4,0`).
//!
//! Slabs go on the active level (the level picked in the project browser,
//! or else the lowest level), their top on it, with the project's first
//! slab type until the type selector (#26) exists. Each slab is one
//! transaction and one undo step, and the tool stays active for the next.
//!
//! The Vertical opening tool, as Revit's Opening > Vertical, first asks
//! for the slab to cut, picked in a plan view, and then sketches the
//! opening the same way. It stays active for more openings in the same
//! slab until Esc.

use hito_core::{ElementId, StoreError, StoreView, Transaction};
use hito_structural::level::LEVEL;
use hito_structural::outline::{self, Point, TOLERANCE};
use hito_structural::slab::{self, Placement, SLAB, SLAB_TYPE};

use super::placement;
use crate::command::{ClassRef, Context, Input, Next, Prompt, Tool, Wants};
use crate::project;

/// What the Vertical opening tool picks first.
const SLABS: &[ClassRef] = &[ClassRef {
    schema: hito_structural::SCHEMA,
    name: SLAB,
}];

/// Starts the Slab tool.
pub fn start() -> Box<dyn Tool> {
    Box::new(SketchTool::new(Target::Slab))
}

/// Starts the Vertical opening tool.
pub fn start_opening() -> Box<dyn Tool> {
    Box::new(SketchTool::new(Target::Opening))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Slab,
    Opening,
}

#[derive(Debug, Clone)]
struct SketchTool {
    target: Target,
    level: Option<ElementId>,
    /// The slab an opening cuts, once it's picked.
    host: Option<ElementId>,
    corners: Vec<Point>,
}

impl SketchTool {
    fn new(target: Target) -> Self {
        Self {
            target,
            level: None,
            host: None,
            corners: Vec::new(),
        }
    }
}

impl Tool for SketchTool {
    fn start(&mut self, context: &Context<'_>) {
        self.level = context.level;
    }

    fn prompt(&self) -> Prompt {
        let (first, next) = match self.target {
            Target::Opening if self.host.is_none() => {
                return Prompt {
                    message: "opening-prompt-slab",
                    wants: Wants::Element(SLABS),
                };
            }
            Target::Slab => ("slab-prompt-start", "slab-prompt-next"),
            Target::Opening => ("opening-prompt-start", "opening-prompt-next"),
        };
        match self.corners.last() {
            None => Prompt {
                message: first,
                wants: Wants::Point,
            },
            Some(&[x, y]) => Prompt {
                message: next,
                wants: Wants::PointFrom([x, y, 0.0]),
            },
        }
    }

    fn pick(&mut self, element: ElementId, model: &StoreView<'_>) -> Next {
        let wants_slab = self.target == Target::Opening && self.host.is_none();
        let class = hito_structural::class(SLAB);
        let is_slab = model
            .get(element)
            .is_some_and(|e| model.registry().is_a(e.class(), &class));
        if !wants_slab || !is_slab {
            return Next::Reject("opening-pick-a-slab");
        }
        self.host = Some(element);
        Next::Ask
    }

    fn input(&mut self, input: Input) -> Next {
        let Input::Point([x, y, _]) = input else {
            return Next::Reject("input-not-a-point");
        };
        let point = [x, y];
        let near = |a: Point| (a[0] - x).hypot(a[1] - y) <= TOLERANCE;
        match (self.corners.first(), self.corners.last()) {
            (_, Some(&last)) if near(last) => Next::Reject("sketch-same-point"),
            (Some(&first), _) if near(first) => match outline::check(&self.corners) {
                Ok(()) => Next::Repeat,
                Err(_) if self.corners.len() < 3 => Next::Reject("sketch-too-few"),
                Err(_) => Next::Reject("sketch-crossing"),
            },
            _ => {
                self.corners.push(point);
                if outline::check_open(&self.corners).is_err() {
                    self.corners.pop();
                    return Next::Reject("sketch-crossing");
                }
                Next::Ask
            }
        }
    }

    /// An opening's next one goes in the same slab; a slab's starts again.
    fn after_commit(&self) -> Option<Box<dyn Tool>> {
        Some(Box::new(SketchTool {
            corners: Vec::new(),
            ..self.clone()
        }))
    }

    fn commit(self: Box<Self>, tx: &mut Transaction<'_>) -> Result<(), StoreError> {
        let model = project::physical_model(tx)?;
        let view = tx.view();
        match self.target {
            Target::Slab => {
                // Without a level or a slab type, the command fails and
                // says so.
                let level = placement::base_level(&view, self.level)
                    .ok_or_else(|| StoreError::UnknownClass(hito_structural::class(LEVEL)))?;
                let slab_type = default_type(&view)
                    .ok_or_else(|| StoreError::UnknownClass(hito_structural::class(SLAB_TYPE)))?;
                let placement = Placement::new(slab_type, level, &self.corners);
                slab::insert(tx, model, &placement).map(drop)
            }
            Target::Opening => {
                let host = self.host.expect("the slab is picked first");
                // The slab's rules refuse an opening that isn't inside it.
                slab::insert_opening(tx, model, host, &self.corners).map(drop)
            }
        }
    }
}

/// The slab type new slabs get: the project's first.
fn default_type(view: &StoreView<'_>) -> Option<ElementId> {
    placement::first_of(view, &hito_structural::class(SLAB_TYPE))
}

#[cfg(test)]
mod tests {
    use hito_core::{Element, ElementStore, SchemaRegistry, Value};
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
        /// A new project, as the shell makes one, with the Slab and
        /// Vertical opening commands.
        fn new() -> Self {
            let i18n = I18n::new(Locale::EsAr);
            let mut schemas = SchemaRegistry::new();
            hito_structural::register(&mut schemas).unwrap();
            let store = project::new_project(schemas, &i18n);
            let mut registry = Registry::new();
            let commands = [
                ("SLAB", "command-slab", Some("SB"), start as fn() -> _),
                (
                    "VERTICALOPENING",
                    "command-vertical-opening",
                    None,
                    start_opening,
                ),
            ];
            for (name, label, alias, start) in commands {
                let run = Run::Tool(start);
                let command = Command {
                    name,
                    label,
                    alias,
                    run,
                };
                registry.register(command).unwrap();
            }
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

        /// Picks an element, as a click in a plan view does.
        fn pick(&mut self, element: ElementId) -> Outcome {
            let Self {
                session,
                i18n,
                store,
                ..
            } = self;
            session.input(Input::Element(element), i18n, store)
        }

        fn enter_all(&mut self, points: &[&str]) -> Vec<Outcome> {
            points.iter().map(|p| self.enter(p)).collect()
        }

        fn slabs(&self) -> Vec<ElementId> {
            slab::slabs(&self.store.view())
        }
    }

    #[test]
    fn a_slab_is_sketched_corner_by_corner_and_closed_on_its_first_point() {
        let mut f = Fixture::new();
        f.enter("SB");
        assert_eq!(f.session.prompt().unwrap().message, "slab-prompt-start");
        let outcomes = f.enter_all(&["0,0", "@6<0", "@4<90", "@-6,0"]);
        assert!(outcomes.iter().all(|o| matches!(o, Outcome::Nothing)));
        assert_eq!(
            f.session.prompt().unwrap().wants,
            Wants::PointFrom([0.0, 4.0, 0.0])
        );
        let Outcome::Committed(changes) = f.enter("0,0") else {
            panic!("closing the boundary commits the slab");
        };
        assert_eq!(changes.command().map(|c| c.name()), Some("SLAB"));

        let slabs = f.slabs();
        assert_eq!(slabs.len(), 1);
        let view = f.store.view();
        let boundary = slab::boundary(&view, slabs[0]);
        let expected = [[0.0, 0.0], [6.0, 0.0], [6.0, 4.0], [0.0, 4.0]];
        for (corner, expected) in boundary.iter().zip(expected) {
            assert!((corner[0] - expected[0]).abs() < 1e-9);
            assert!((corner[1] - expected[1]).abs() < 1e-9);
        }
        assert_eq!(boundary.len(), 4);
        let ground = level::levels(&view)[0].0;
        assert_eq!(level::level_of(&view, slabs[0]), Some(ground));
        let (bottom, top) = slab::elevations(&view, slabs[0]).unwrap();
        assert!((top - 0.0).abs() < 1e-12 && (bottom + 0.15).abs() < 1e-12);

        // The tool stays active for the next slab.
        assert_eq!(f.session.prompt().unwrap().message, "slab-prompt-start");
    }

    #[test]
    fn a_boundary_that_would_cross_or_close_too_early_is_refused() {
        let mut f = Fixture::new();
        f.enter("SB");
        f.enter_all(&["0,0", "4,0"]);
        // Closing with two corners is refused, and so is the same point.
        assert!(matches!(f.enter("0,0"), Outcome::Nothing));
        assert!(f.session.message().is_some());
        assert!(matches!(f.enter("4,0"), Outcome::Nothing));
        f.enter_all(&["4,4", "2,4"]);
        // Back across the first line.
        assert!(matches!(f.enter("2,-1"), Outcome::Nothing));
        assert!(f.session.message().is_some());
        // The refused point wasn't kept: the next one goes on from 2,4.
        assert_eq!(
            f.session.prompt().unwrap().wants,
            Wants::PointFrom([2.0, 4.0, 0.0])
        );
        assert!(matches!(f.enter("0,4"), Outcome::Nothing));
        assert!(matches!(f.enter("0,0"), Outcome::Committed(_)));
        assert_eq!(f.slabs().len(), 1);
    }

    #[test]
    fn slabs_go_on_the_active_level() {
        let mut f = Fixture::new();
        let mut tx = f.store.transaction();
        let model = project::physical_model(&mut tx).unwrap();
        let first = Element::new(hito_structural::class(LEVEL), model)
            .with_property(level::NAME, "Nivel 2")
            .with_property(level::ELEVATION, 2.8);
        let first = tx.insert(first).unwrap();
        tx.commit().unwrap();
        f.session.set_level(Some(first));
        f.enter("SB");
        f.enter_all(&["0,0", "5,0", "5,5", "0,0"]);
        let slab = f.slabs()[0];
        let view = f.store.view();
        assert_eq!(level::level_of(&view, slab), Some(first));
        assert_eq!(
            f.store.property(slab, slab::TOP_ELEVATION),
            Some(&Value::Real(2.8))
        );
    }

    #[test]
    fn a_vertical_opening_cuts_the_slab_picked_first() {
        let mut f = Fixture::new();
        f.enter("SB");
        f.enter_all(&["0,0", "6,0", "6,4", "0,4", "0,0"]);
        f.enter_all(&["10,0", "16,0", "16,4", "10,0"]);
        f.session.cancel(&f.i18n);
        let [floor, other] = f.slabs()[..] else {
            panic!("two slabs");
        };

        f.enter("VERTICALOPENING");
        assert_eq!(
            f.session.prompt().unwrap(),
            Prompt {
                message: "opening-prompt-slab",
                wants: Wants::Element(SLABS),
            }
        );
        // The slab can't be typed, and only a slab can be picked.
        f.enter("1,1");
        assert_eq!(f.session.prompt().unwrap().message, "opening-prompt-slab");
        let level = level::levels(&f.store.view())[0].0;
        f.pick(level);
        assert!(f.session.message().is_some());
        f.pick(floor);
        assert_eq!(f.session.prompt().unwrap().message, "opening-prompt-start");
        let outcomes = f.enter_all(&["1,1", "@1,0", "@0,2", "@-1,0", "1,1"]);
        assert!(matches!(outcomes[4], Outcome::Committed(_)));
        assert_eq!(slab::openings(&f.store.view(), floor).len(), 1);

        // The next opening goes in the same slab, so one outside it, even
        // inside another slab, can't be made.
        assert_eq!(f.session.prompt().unwrap().message, "opening-prompt-start");
        let outcomes = f.enter_all(&["11,1", "12,1", "12,2", "11,1"]);
        assert!(matches!(outcomes[3], Outcome::Nothing));
        assert!(f.session.message().is_some());
        assert_eq!(slab::openings(&f.store.view(), floor).len(), 1);
        assert!(slab::openings(&f.store.view(), other).is_empty());
    }

    #[test]
    fn new_projects_have_a_generic_slab_type() {
        let f = Fixture::new();
        let view = f.store.view();
        let slab_type = default_type(&view).unwrap();
        assert_eq!(
            f.store.property(slab_type, slab::NAME),
            Some(&Value::Text("Genérico - 150 mm".into()))
        );
        assert_eq!(slab::thickness(&view, slab_type), Some(0.15));
        // Of the generic concrete, as the other generic types.
        let concrete = slab::material(&view, slab_type).unwrap();
        assert_eq!(
            f.store.property(concrete, hito_structural::material::NAME),
            Some(&Value::Text("Hormigón".into()))
        );
    }
}
