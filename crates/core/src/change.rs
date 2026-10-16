//! What a committed transaction changed
//! ([element model §9](https://github.com/hito-project/hito/blob/main/docs/architecture/element-model.md#9-change-sets-one-structure-for-undo-saving-and-collaboration)).
//!
//! A [`ChangeSet`] is a record of facts: element 42 was like this and is now
//! like that. It's plain data, and applying it needs no domain logic, so it
//! means the same in any version of HITO. Undo and redo (#19) and change sets
//! (#21) are built on it. Its final shape is decided by the history spike
//! (#156).

use crate::{Element, ElementId, Model, Properties, Value};

/// The user command a transaction carries out, as plain data: a name and
/// parameters. The UI shows it in the undo list (#19).
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    name: String,
    parameters: Properties,
}

impl Command {
    /// A command called `name`, with no parameters yet.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parameters: Properties::new(),
        }
    }

    pub fn with_parameter(mut self, name: impl Into<String>, value: impl Into<Value>) -> Self {
        self.parameters.insert(name.into(), value.into());
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn parameters(&self) -> &Properties {
        &self.parameters
    }
}

/// Everything one transaction changed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChangeSet {
    pub(crate) command: Option<Command>,
    pub(crate) models: Vec<Model>,
    pub(crate) elements: Vec<ElementChange>,
}

impl ChangeSet {
    /// The command the transaction carried out, if it was given one.
    pub fn command(&self) -> Option<&Command> {
        self.command.as_ref()
    }

    /// The models the transaction added.
    pub fn models(&self) -> &[Model] {
        &self.models
    }

    /// The elements the transaction created, changed or deleted, in the
    /// order it first changed them. An element changed and then changed
    /// back isn't included.
    pub fn elements(&self) -> &[ElementChange] {
        &self.elements
    }
}

/// One element before and after a transaction: the whole element each time,
/// `None` before if the transaction created it, and `None` after if it
/// deleted it.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementChange {
    pub(crate) id: ElementId,
    pub(crate) before: Option<Element>,
    pub(crate) after: Option<Element>,
}

impl ElementChange {
    pub fn id(&self) -> ElementId {
        self.id
    }

    pub fn before(&self) -> Option<&Element> {
        self.before.as_ref()
    }

    pub fn after(&self) -> Option<&Element> {
        self.after.as_ref()
    }
}
