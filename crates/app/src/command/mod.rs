//! Commands: everything the user can do, by name
//! ([ADR 0015](https://github.com/hito-project/hito/blob/main/docs/decisions/0015-ui-toolkit.md),
//! section 2).
//!
//! Every [`Command`] has a language-neutral name (`COLUMN`), a localised
//! label (`Columna`) and an optional Revit-style alias (`CL`). A ribbon
//! button, a name typed in the command line and an alias typed in a view
//! all run the same command.
//!
//! A command either changes the window ([`Run::Window`]) or runs a [`Tool`]
//! that asks for input on the command line, AutoCAD style, and then changes
//! the model in one transaction ([`Session`]). Placement tools (#33, #34,
//! #37 to #41) are tools, so each of them is one undo step (#19).
//!
//! While a tool runs, the shell shows a contextual ribbon tab with Finish
//! and Cancel ([`Tool::finish`], [`Session::cancel`]), as Revit does, so no
//! tool needs the command line to end. An options bar under the ribbon
//! shows the running tool's values, such as a wall's height, and changes
//! them by clicking ([`Tool::options`]). The values set are remembered for
//! each command until the program closes, as Revit's options bar does.
//!
//! Nothing here uses egui; the shell draws the command line.

mod input;
mod matching;
mod session;

use hito_core::{ClassId, Dimension, ElementId, StoreError, StoreView, Transaction};

pub use input::{InputError, InputFormat};
pub use matching::fold;
pub use session::{Outcome, Session};

use crate::i18n::I18n;
use crate::shell::Action;

/// A command, as the registry lists it.
#[derive(Debug, Clone, Copy)]
pub struct Command {
    /// The language-neutral name, accepted in any UI language, like
    /// AutoCAD's `_LINE`. Upper case letters and digits. It's also the name
    /// a transaction records, for the undo list and the file's history.
    pub name: &'static str,
    /// The message key of its localised label.
    pub label: &'static str,
    /// A Revit-style keyboard shortcut, such as `CL`.
    pub alias: Option<&'static str>,
    pub run: Run,
}

/// What running a command does.
#[derive(Debug, Clone, Copy)]
pub enum Run {
    /// Changes the window, such as tiling the views. Not part of the
    /// model's history, as in Revit.
    Window(Action),
    /// Starts a tool, which asks for input and then changes the model.
    Tool(fn() -> Box<dyn Tool>),
    /// A command from a later story. It's listed, so its alias and name
    /// already work, but it only says it isn't available yet.
    Unavailable,
}

/// A command in progress: it asks for input, one prompt at a time, until it
/// has what it needs, and then makes its change in one transaction.
///
/// The [`Session`] opens that transaction, named after the command, so a
/// tool never opens one itself, and cancelling with Esc changes nothing.
pub trait Tool {
    /// Called once when the tool starts, before its first prompt, with what
    /// it may need to know about where it runs. Most tools need nothing.
    fn start(&mut self, context: &Context<'_>) {
        let _ = context;
    }

    /// What the tool asks for next.
    fn prompt(&self) -> Prompt;

    /// Takes the answer to the last prompt.
    fn input(&mut self, input: Input) -> Next;

    /// Takes an element picked in a view, the answer to a prompt that
    /// wants [`Wants::Element`] ([`Input::Element`]), with the model to
    /// read it from. Tools that never ask for one keep the default, which
    /// refuses it.
    fn pick(&mut self, element: ElementId, model: &StoreView<'_>) -> Next {
        let _ = (element, model);
        Next::Reject("input-not-a-pick")
    }

    /// Makes the tool's change, once [`Tool::input`] has returned
    /// [`Next::Done`] or [`Next::Repeat`]. An error discards the whole
    /// transaction.
    fn commit(self: Box<Self>, tx: &mut Transaction<'_>) -> Result<(), StoreError>;

    /// After [`Next::Repeat`], the tool that carries on, such as a wall tool
    /// that starts the next wall where the last one ended (a chain). `None`,
    /// the default, starts the command again from scratch. Called just
    /// before [`Tool::commit`]; if the commit fails, the command starts
    /// again from scratch instead.
    fn after_commit(&self) -> Option<Box<dyn Tool>> {
        None
    }

    /// Whether the tool already has what it needs once it has started, so
    /// the session commits it at once, as after [`Next::Repeat`], before
    /// its first prompt. Such as the Isolated footing tool started with
    /// columns selected, which places one under each. `false`, the
    /// default, for tools that ask first.
    fn ready_at_start(&self, model: &StoreView<'_>) -> bool {
        let _ = model;
        false
    }

    /// Finishes the tool, as the contextual tab's Finish button, or Enter
    /// on an empty command line, does. `None`, the default, means there's
    /// nothing more to make: the tool ends, and anything half drawn, such
    /// as a wall's start point, is dropped. A sketch tool returns
    /// `Some(Next::Done)` to commit its sketch and end, or
    /// `Some(Next::Reject(..))` when the sketch can't be finished yet.
    fn finish(&mut self) -> Option<Next> {
        None
    }

    /// The message key of the contextual tab's title while the tool runs,
    /// for tools that sketch, such as "Modify | Create Floor Boundary".
    /// `None`, the default, titles it "Modify | Place" and the command's
    /// label, as Revit does.
    fn tab_title(&self) -> Option<&'static str> {
        None
    }

    /// The values the options bar shows while the tool runs, as Revit's
    /// options bar shows a wall's height and location line, with the model
    /// to read levels from. The default, none, shows an empty bar.
    fn options(&self, model: &StoreView<'_>) -> Vec<ToolOption> {
        let _ = model;
        Vec::new()
    }

    /// Sets the option called `key`, as the options bar does when the user
    /// changes it, or the session does when the tool starts with the value
    /// last set. `Err` holds the message key of why the value is refused.
    /// The default refuses every option.
    fn set_option(&mut self, key: &'static str, value: OptionValue) -> Result<(), &'static str> {
        let _ = (key, value);
        Err("option-unknown")
    }
}

/// One value on the options bar ([`Tool::options`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOption {
    /// Names the option to [`Tool::set_option`], and is the message key of
    /// its label.
    pub key: &'static str,
    pub value: OptionValue,
    /// Whether it can be changed now, such as a column's unconnected
    /// height only while its top is unconnected.
    pub enabled: bool,
}

/// An option's value.
#[derive(Debug, Clone, PartialEq)]
pub enum OptionValue {
    /// A quantity, in SI units, typed with or without units.
    Quantity { value: f64, dimension: Dimension },
    /// One of a list, such as a wall's location line or a column's top.
    Choice {
        selected: usize,
        choices: Vec<Choice>,
    },
}

impl OptionValue {
    /// The quantity, if it is one.
    pub fn quantity(&self) -> Option<f64> {
        match self {
            OptionValue::Quantity { value, .. } => Some(*value),
            OptionValue::Choice { .. } => None,
        }
    }

    /// The choice selected, if it is a choice.
    pub fn chosen(&self) -> Option<Choice> {
        match self {
            OptionValue::Choice { selected, choices } => choices.get(*selected).cloned(),
            OptionValue::Quantity { .. } => None,
        }
    }
}

/// One of an [`OptionValue::Choice`]'s choices. It names itself, so a
/// remembered choice is found again even when the list has changed.
#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    /// A choice named by this message key, such as "Wall Centerline".
    Key(&'static str),
    /// An element, such as a level, shown by its name, which is data.
    Element { id: ElementId, name: String },
}

/// What a tool knows about where it runs, given to [`Tool::start`].
pub struct Context<'a> {
    /// The user interface's language, for data the tool names in it, such
    /// as a new level's name.
    pub i18n: &'a I18n,
    /// The level new elements go on: the active plan view's (#183), or
    /// else the one picked in the project browser.
    /// `None` lets the tool choose, such as the lowest level.
    pub level: Option<ElementId>,
    /// What was selected when the user started the command, such as the
    /// columns to place footings under. Empty when the session starts a
    /// tool again by itself, for the next element or on another level.
    pub selection: &'a [ElementId],
}

/// What a tool asks for, shown on the command line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prompt {
    /// The message key of the question, such as "Specify the first point".
    pub message: &'static str,
    pub wants: Wants,
}

/// The kind of answer a prompt takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Wants {
    /// A point, as coordinates, typed or clicked in a plan view (#183). A
    /// clicked point is in world coordinates, on the plan's level: its Z
    /// is the level's elevation.
    Point,
    /// A point, which can also be typed relative to this one, as in
    /// AutoCAD: `@dx,dy` or `@length<angle`, the angle in degrees
    /// anticlockwise from x. How walls take typed lengths.
    PointFrom([f64; 3]),
    /// A quantity of a dimension, such as a length, with or without units.
    /// [`Dimension::NONE`] is a plain number.
    Quantity(Dimension),
    /// An element of one of these classes (or a class derived from one),
    /// clicked in a plan view, such as the slab an opening cuts. It can't
    /// be typed.
    Element(&'static [ClassRef]),
    /// A point, as [`Wants::Point`] or, with `from`, [`Wants::PointFrom`],
    /// or else an element of one of `classes` clicked in a plan view: a
    /// click on such an element picks it, and a click anywhere else gives
    /// the point. Such as a slab's boundary, drawn or picked from walls.
    PointOrElement {
        from: Option<[f64; 3]>,
        classes: &'static [ClassRef],
    },
}

impl Wants {
    /// The classes a click in a plan view picks, if it picks elements.
    pub fn classes(self) -> Option<&'static [ClassRef]> {
        match self {
            Wants::Element(classes) | Wants::PointOrElement { classes, .. } => Some(classes),
            _ => None,
        }
    }

    /// How a point is asked for, if one is: [`Wants::Point`] or
    /// [`Wants::PointFrom`].
    pub fn point(self) -> Option<Wants> {
        match self {
            Wants::Point | Wants::PointFrom(_) => Some(self),
            Wants::PointOrElement { from: None, .. } => Some(Wants::Point),
            Wants::PointOrElement {
                from: Some(base), ..
            } => Some(Wants::PointFrom(base)),
            Wants::Quantity(_) | Wants::Element(_) => None,
        }
    }

    /// Whether the answer is clicked in a plan view: a point or an
    /// element. A tool asking for one opens a plan view when it starts in
    /// 3D (#229).
    pub fn is_clicked(self) -> bool {
        self.point().is_some() || self.classes().is_some()
    }
}

/// A class named by its schema and its name, so a [`Prompt`] can name the
/// classes it picks in a constant. [`ClassRef::id`] gives the core's
/// [`ClassId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassRef {
    pub schema: &'static str,
    pub name: &'static str,
}

impl ClassRef {
    pub fn id(self) -> ClassId {
        ClassId::new(self.schema, self.name)
    }
}

/// An answer to a prompt, in the core's SI units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    /// X, Y and Z in metres. Z is 0 when only X and Y were given.
    Point([f64; 3]),
    Quantity(f64),
    /// An element picked in a view, for [`Wants::Element`]. The session
    /// gives it to [`Tool::pick`], with the model, not to [`Tool::input`].
    Element(ElementId),
}

/// What a tool does after an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Asks its next prompt.
    Ask,
    /// Asks the same prompt again, after the message with this key, such as
    /// "The second point must differ from the first".
    Reject(&'static str),
    /// Has what it needs, and is ready to commit.
    Done,
    /// Has what it needs for one element: commit it, then start again for
    /// the next one, as Revit's placement tools stay active until Esc.
    /// Each element is its own transaction, so its own undo step.
    Repeat,
}

/// Every command, in the order completions list them.
#[derive(Debug, Default)]
pub struct Registry {
    commands: Vec<Command>,
}

/// A command that can't be registered, because its name or alias is taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Taken(pub &'static str);

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a command. Names and aliases are compared ignoring case.
    pub fn register(&mut self, command: Command) -> Result<(), Taken> {
        let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
        for other in &self.commands {
            if same(other.name, command.name) {
                return Err(Taken(command.name));
            }
            if let (Some(a), Some(b)) = (other.alias, command.alias)
                && same(a, b)
            {
                return Err(Taken(b));
            }
        }
        self.commands.push(command);
        Ok(())
    }

    /// The command with this language-neutral name.
    pub fn get(&self, name: &str) -> Option<&Command> {
        self.commands
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Command> {
        self.commands.iter()
    }

    /// The command with this alias.
    pub fn by_alias(&self, keys: &str) -> Option<&Command> {
        self.commands
            .iter()
            .find(|c| c.alias.is_some_and(|a| a.eq_ignore_ascii_case(keys)))
    }

    /// Whether `keys` could still become an alias with more keys, as `C`
    /// can become `CL`.
    pub fn alias_continues(&self, keys: &str) -> bool {
        let keys = keys.to_ascii_uppercase();
        self.commands.iter().any(|c| {
            c.alias
                .is_some_and(|a| a.len() > keys.len() && a.to_ascii_uppercase().starts_with(&keys))
        })
    }
}
