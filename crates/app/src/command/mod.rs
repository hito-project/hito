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
//! Nothing here uses egui; the shell draws the command line.

mod input;
mod matching;
mod session;

use hito_core::{Dimension, StoreError, Transaction};

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

    /// Makes the tool's change, once [`Tool::input`] has returned
    /// [`Next::Done`] or [`Next::Repeat`]. An error discards the whole
    /// transaction.
    fn commit(self: Box<Self>, tx: &mut Transaction<'_>) -> Result<(), StoreError>;
}

/// What a tool knows about where it runs, given to [`Tool::start`].
pub struct Context<'a> {
    /// The user interface's language, for data the tool names in it, such
    /// as a new level's name.
    pub i18n: &'a I18n,
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
    /// A point, as coordinates. Picking it in a view comes with #29.
    Point,
    /// A quantity of a dimension, such as a length, with or without units.
    /// [`Dimension::NONE`] is a plain number.
    Quantity(Dimension),
}

/// An answer to a prompt, in the core's SI units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    /// X, Y and Z in metres. Z is 0 when only X and Y were given.
    Point([f64; 3]),
    Quantity(f64),
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
