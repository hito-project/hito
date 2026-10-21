//! Running commands: the command line's state between keystrokes.

use hito_core::{ChangeSet, ElementId, ElementStore};

use super::{Command, Context, Input, InputFormat, Next, Prompt, Registry, Run, Tool};
use crate::i18n::I18n;
use crate::shell::Action;

/// The registered commands, the one running, and what the command line
/// last said.
pub struct Session {
    registry: Registry,
    running: Option<Running>,
    message: Option<String>,
    /// The level tools place elements on, given to them when they start.
    level: Option<ElementId>,
    /// What is selected, given to the next tool the user starts.
    selection: Vec<ElementId>,
}

struct Running {
    command: Command,
    tool: Box<dyn Tool>,
    /// The level it was started with.
    level: Option<ElementId>,
}

/// What a step of a command did, for the shell to follow up.
#[derive(Debug)]
pub enum Outcome {
    /// Nothing for the shell to do: the command asks for more, or said why
    /// it can't go on.
    Nothing,
    /// The window should change.
    Window(Action),
    /// A tool finished and changed the model. Undo (#19) records this
    /// change set as one step, named after the command.
    Committed(ChangeSet),
}

impl Session {
    pub fn new(registry: Registry) -> Self {
        Self {
            registry,
            running: None,
            message: None,
            level: None,
            selection: Vec::new(),
        }
    }

    /// Sets the level tools started from now on place elements on.
    pub fn set_level(&mut self, level: Option<ElementId>) {
        self.level = level;
    }

    /// Sets what is selected, for the next tool the user starts.
    pub fn set_selection(&mut self, selection: impl IntoIterator<Item = ElementId>) {
        self.selection = selection.into_iter().collect();
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// The registry, to add commands to.
    pub fn registry_mut(&mut self) -> &mut Registry {
        &mut self.registry
    }

    /// The command waiting for input, if there is one.
    pub fn running(&self) -> Option<&Command> {
        self.running.as_ref().map(|r| &r.command)
    }

    /// The level the running command was started with, which it places
    /// elements on.
    pub fn running_level(&self) -> Option<ElementId> {
        self.running.as_ref().and_then(|r| r.level)
    }

    /// What the running command asks for.
    pub fn prompt(&self) -> Option<Prompt> {
        self.running.as_ref().map(|r| r.tool.prompt())
    }

    /// What the command line last said, such as why a command couldn't
    /// run.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Runs the command called `name`, cancelling the one running, as a
    /// ribbon button or a shortcut does in Revit.
    /// A tool that is ready at once ([`Tool::ready_at_start`]) commits
    /// straight away.
    pub fn start(&mut self, name: &str, i18n: &I18n, store: &mut ElementStore) -> Outcome {
        let Some(&command) = self.registry.get(name) else {
            return Outcome::Nothing;
        };
        if self.running.is_some() {
            self.cancel(i18n);
        }
        self.message = None;
        match command.run {
            Run::Window(action) => Outcome::Window(action),
            Run::Tool(start) => {
                // Only the tool the user starts gets the selection, not the
                // ones the session starts again by itself.
                let selection = std::mem::take(&mut self.selection);
                let tool = started(start, i18n, self.level, &selection);
                let ready = tool.ready_at_start(&store.view());
                self.running = Some(Running {
                    command,
                    tool,
                    level: self.level,
                });
                match ready {
                    true => self.finish(Next::Repeat, i18n, store),
                    false => Outcome::Nothing,
                }
            }
            Run::Unavailable => {
                let label = i18n.tr(command.label);
                self.message = Some(i18n.tr_with("command-not-available", "command", label));
                Outcome::Nothing
            }
        }
    }

    /// Takes what was typed and entered on the command line: the answer to
    /// the running command's prompt, or else a command to run, by label,
    /// name or alias. A partial name runs the first command it completes
    /// to, as in AutoCAD.
    pub fn enter(
        &mut self,
        text: &str,
        i18n: &I18n,
        format: &InputFormat,
        store: &mut ElementStore,
    ) -> Outcome {
        let Some(running) = &mut self.running else {
            if text.trim().is_empty() {
                return Outcome::Nothing;
            }
            let command = self
                .registry
                .resolve(i18n, text)
                .or_else(|| self.registry.complete(i18n, text).first().copied());
            return match command {
                Some(command) => {
                    let name = command.name;
                    self.start(name, i18n, store)
                }
                None => {
                    self.message = Some(i18n.tr_with("command-unknown", "input", text.trim()));
                    Outcome::Nothing
                }
            };
        };
        let input = match format.parse(text, running.tool.prompt().wants) {
            Ok(input) => input,
            Err(error) => {
                self.message = Some(i18n.tr(error.message()));
                return Outcome::Nothing;
            }
        };
        self.input(input, i18n, store)
    }

    /// Answers the running command's prompt with input that's already
    /// read, such as a point clicked in a plan view, as typing it and
    /// pressing Enter does. An element picked in a view goes to the tool's
    /// [`Tool::pick`], with the model. Does nothing if no command is
    /// running.
    pub fn input(&mut self, input: Input, i18n: &I18n, store: &mut ElementStore) -> Outcome {
        let Some(running) = &mut self.running else {
            return Outcome::Nothing;
        };
        self.message = None;
        let next = match input {
            Input::Element(element) => running.tool.pick(element, &store.view()),
            input => running.tool.input(input),
        };
        self.finish(next, i18n, store)
    }

    /// Follows up what the running tool said after its input: commits it
    /// once it's done, and carries on with the next one if it repeats.
    fn finish(&mut self, next: Next, i18n: &I18n, store: &mut ElementStore) -> Outcome {
        match next {
            Next::Ask => Outcome::Nothing,
            Next::Reject(message) => {
                self.message = Some(i18n.tr(message));
                Outcome::Nothing
            }
            next @ (Next::Done | Next::Repeat) => {
                let Running {
                    command,
                    tool,
                    level,
                } = self.running.take().expect("running");
                let mut next_tool = match next {
                    Next::Repeat => tool.after_commit(),
                    _ => None,
                };
                let mut tx = store.transaction_for(hito_core::Command::new(command.name));
                let outcome = match tool.commit(&mut tx).and_then(|()| tx.commit()) {
                    Ok(changes) => Outcome::Committed(changes),
                    Err(_) => {
                        next_tool = None;
                        let label = i18n.tr(command.label);
                        self.message = Some(i18n.tr_with("command-failed", "command", label));
                        Outcome::Nothing
                    }
                };
                if let (Next::Repeat, Run::Tool(start)) = (next, command.run) {
                    let tool = next_tool.unwrap_or_else(|| started(start, i18n, level, &[]));
                    self.running = Some(Running {
                        command,
                        tool,
                        level,
                    });
                }
                outcome
            }
        }
    }

    /// Cancels the running command, as Esc does. Its input so far is
    /// dropped and the model doesn't change.
    pub fn cancel(&mut self, i18n: &I18n) {
        if self.running.take().is_some() {
            self.message = Some(i18n.tr("command-cancelled"));
        } else {
            self.message = None;
        }
    }
}

/// A new tool from its command's factory, started.
fn started(
    start: fn() -> Box<dyn Tool>,
    i18n: &I18n,
    level: Option<ElementId>,
    selection: &[ElementId],
) -> Box<dyn Tool> {
    let mut tool = start();
    tool.start(&Context {
        i18n,
        level,
        selection,
    });
    tool
}

#[cfg(test)]
mod tests {
    use hito_core::{Dimension, Model, ModelKind, SchemaRegistry, StoreError, Transaction};

    use super::*;
    use crate::command::{ClassRef, Input, Wants};
    use crate::i18n::Locale;
    use hito_core::StoreView;

    /// Asks for a point and a length, then adds a model.
    #[derive(Default)]
    struct Probe {
        point: Option<[f64; 3]>,
    }

    impl Tool for Probe {
        fn prompt(&self) -> Prompt {
            match self.point {
                None => Prompt {
                    message: "probe-point",
                    wants: Wants::Point,
                },
                Some(_) => Prompt {
                    message: "probe-length",
                    wants: Wants::Quantity(Dimension::LENGTH),
                },
            }
        }

        fn input(&mut self, input: Input) -> Next {
            match input {
                Input::Point(p) => {
                    self.point = Some(p);
                    Next::Ask
                }
                Input::Quantity(length) if length <= 0.0 => Next::Reject("probe-positive"),
                Input::Quantity(_) => Next::Done,
                Input::Element(_) => unreachable!("given to pick"),
            }
        }

        fn commit(self: Box<Self>, tx: &mut Transaction<'_>) -> Result<(), StoreError> {
            tx.insert_model(Model::new(ModelKind::Physical)).map(drop)
        }
    }

    /// Asks for a length, adds a model for each, and stays active. Checks
    /// it was started in English.
    #[derive(Default)]
    struct Stamp {
        started_in: Option<Locale>,
    }

    impl Tool for Stamp {
        fn start(&mut self, context: &Context<'_>) {
            self.started_in = Some(context.i18n.locale());
        }

        fn prompt(&self) -> Prompt {
            Prompt {
                message: "stamp-length",
                wants: Wants::Quantity(Dimension::LENGTH),
            }
        }

        fn input(&mut self, _: Input) -> Next {
            Next::Repeat
        }

        fn commit(self: Box<Self>, tx: &mut Transaction<'_>) -> Result<(), StoreError> {
            assert_eq!(
                self.started_in,
                Some(Locale::En),
                "started before committing"
            );
            tx.insert_model(Model::new(ModelKind::Physical)).map(drop)
        }
    }

    /// Asks for a model element to be picked, then adds a model.
    #[derive(Default)]
    struct Choose {
        picked: Option<ElementId>,
    }

    const ANY: &[ClassRef] = &[ClassRef {
        schema: hito_core::ClassId::CORE_SCHEMA,
        name: "Element",
    }];

    impl Tool for Choose {
        fn prompt(&self) -> Prompt {
            Prompt {
                message: "choose-element",
                wants: Wants::Element(ANY),
            }
        }

        fn input(&mut self, _: Input) -> Next {
            Next::Reject("choose-not-typed")
        }

        fn pick(&mut self, element: ElementId, model: &StoreView<'_>) -> Next {
            if model.get(element).is_none() {
                return Next::Reject("choose-unknown");
            }
            self.picked = Some(element);
            Next::Done
        }

        fn commit(self: Box<Self>, tx: &mut Transaction<'_>) -> Result<(), StoreError> {
            assert!(self.picked.is_some());
            tx.insert_model(Model::new(ModelKind::Physical)).map(drop)
        }
    }

    struct Fixture {
        session: Session,
        i18n: I18n,
        format: InputFormat,
        store: ElementStore,
    }

    impl Fixture {
        fn new() -> Self {
            let mut registry = Registry::new();
            let commands = [
                (
                    "PROBE",
                    "command-beam",
                    Some("PR"),
                    Run::Tool(|| Box::new(Probe::default())),
                ),
                (
                    "STAMP",
                    "command-level",
                    None,
                    Run::Tool(|| Box::new(Stamp::default())),
                ),
                (
                    "CHOOSE",
                    "command-grid",
                    None,
                    Run::Tool(|| Box::new(Choose::default())),
                ),
                ("COLUMN", "command-column", Some("CL"), Run::Unavailable),
                (
                    "TILEVIEWS",
                    "action-tile-views",
                    Some("WT"),
                    Run::Window(Action::TileViews),
                ),
            ];
            for (name, label, alias, run) in commands {
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
                i18n: I18n::new(Locale::En),
                format: InputFormat::new(),
                store: ElementStore::new(SchemaRegistry::new()),
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
    fn a_tool_prompts_then_commits_one_named_transaction() {
        let mut f = Fixture::new();
        f.enter("pr");
        assert_eq!(f.session.running().map(|c| c.name), Some("PROBE"));
        assert_eq!(f.session.prompt().unwrap().wants, Wants::Point);

        f.enter("two");
        assert!(f.session.message().is_some(), "says it isn't a point");
        assert_eq!(f.session.prompt().unwrap().wants, Wants::Point);

        f.enter("1,2");
        f.enter("-1 m");
        assert!(f.session.message().is_some(), "the tool rejected it");
        let Outcome::Committed(changes) = f.enter("30 cm") else {
            panic!("committed");
        };
        assert_eq!(changes.command().map(|c| c.name()), Some("PROBE"));
        assert_eq!(f.store.models().count(), 1);
        assert!(f.session.running().is_none());
    }

    #[test]
    fn cancelling_changes_nothing() {
        let mut f = Fixture::new();
        f.session.start("PROBE", &f.i18n, &mut f.store);
        f.enter("1,2");
        f.session.cancel(&f.i18n);
        assert!(f.session.running().is_none());
        assert_eq!(
            f.session.message(),
            Some(f.i18n.tr("command-cancelled").as_str())
        );
        assert_eq!(f.store.models().count(), 0);
    }

    #[test]
    fn starting_another_command_cancels_the_running_one() {
        let mut f = Fixture::new();
        f.session.start("PROBE", &f.i18n, &mut f.store);
        let outcome = f.session.start("TILEVIEWS", &f.i18n, &mut f.store);
        assert!(matches!(outcome, Outcome::Window(Action::TileViews)));
        assert!(f.session.running().is_none());
    }

    #[test]
    fn a_repeating_tool_commits_each_element_and_stays_active() {
        let mut f = Fixture::new();
        f.session.start("STAMP", &f.i18n, &mut f.store);
        for n in 1..=2 {
            let Outcome::Committed(changes) = f.enter("3 m") else {
                panic!("committed");
            };
            assert_eq!(changes.command().map(|c| c.name()), Some("STAMP"));
            assert_eq!(changes.models().len(), 1, "one element per transaction");
            assert_eq!(f.store.models().count(), n);
            assert_eq!(f.session.running().map(|c| c.name), Some("STAMP"));
        }
        // Esc ends it, and keeps what was committed.
        f.session.cancel(&f.i18n);
        assert!(f.session.running().is_none());
        assert_eq!(f.store.models().count(), 2);
    }

    /// A clicked point answers a prompt as a typed one does.
    #[test]
    fn input_already_read_answers_the_prompt() {
        let mut f = Fixture::new();
        let point = Input::Point([1.0, 2.0, 0.5]);
        let Fixture {
            session,
            i18n,
            store,
            ..
        } = &mut f;
        assert!(matches!(
            session.input(point, i18n, store),
            Outcome::Nothing
        ));
        let level = ElementId::generate();
        session.set_level(Some(level));
        session.start("PROBE", i18n, store);
        session.set_level(None);
        assert_eq!(session.running_level(), Some(level), "as it started");
        session.input(point, i18n, store);
        assert_eq!(
            session.prompt().unwrap().wants,
            Wants::Quantity(Dimension::LENGTH)
        );
        let Outcome::Committed(_) = session.input(Input::Quantity(2.0), i18n, store) else {
            panic!("committed");
        };
        assert_eq!(store.models().count(), 1);
    }

    /// A picked element goes to the tool's `pick`, with the model, and
    /// can't be typed.
    #[test]
    fn a_picked_element_answers_a_prompt_that_wants_one() {
        let mut f = Fixture::new();
        f.session.start("CHOOSE", &f.i18n, &mut f.store);
        f.enter("1,2");
        assert!(f.session.message().is_some(), "can't be typed");
        assert_eq!(f.session.running().map(|c| c.name), Some("CHOOSE"));

        let Fixture {
            session,
            i18n,
            store,
            ..
        } = &mut f;
        let unknown = Input::Element(ElementId::generate());
        assert!(matches!(
            session.input(unknown, i18n, store),
            Outcome::Nothing
        ));
        assert!(session.message().is_some(), "the tool refused it");

        let mut tx = store.transaction();
        let model = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
        let level = hito_core::Element::new(hito_core::ClassId::ELEMENT, model);
        let element = tx.insert(level).unwrap();
        tx.commit().unwrap();
        let Outcome::Committed(_) = session.input(Input::Element(element), i18n, store) else {
            panic!("committed");
        };
        assert_eq!(store.models().count(), 2);
    }

    /// Tools that never ask for an element refuse one.
    #[test]
    fn tools_refuse_a_pick_by_default() {
        let mut f = Fixture::new();
        f.session.start("PROBE", &f.i18n, &mut f.store);
        let Fixture {
            session,
            i18n,
            store,
            ..
        } = &mut f;
        session.input(Input::Element(ElementId::generate()), i18n, store);
        assert_eq!(
            session.message(),
            Some(i18n.tr("input-not-a-pick").as_str())
        );
        assert_eq!(session.prompt().unwrap().wants, Wants::Point, "asks again");
    }

    #[test]
    fn unknown_and_unavailable_commands_say_so() {
        let mut f = Fixture::new();
        f.enter("xyzzy");
        assert!(f.session.message().unwrap().contains("xyzzy"));
        f.enter("CL");
        assert!(f.session.message().unwrap().contains("Column"));
        assert!(f.session.running().is_none());
    }
}
