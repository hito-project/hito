//! Validation rules: checks a domain adds on top of its schema, run when a
//! [`Transaction`](crate::Transaction) commits.

use std::fmt;
use std::sync::Arc;

use crate::{ClassId, Element, ElementId, StoreView};

type Check = dyn Fn(&StoreView<'_>, &Element) -> Result<(), String> + Send + Sync;

/// A check on the elements of one class and its subclasses.
///
/// A domain adds its rules to the
/// [`SchemaRegistry`](crate::SchemaRegistry) at startup, after its schema.
/// When a transaction commits, each rule checks every element of its class
/// that the transaction created or changed. The check sees the whole model as
/// the transaction left it, and returns a message if the element breaks the
/// rule:
///
/// ```
/// use hito_core::{ClassId, Rule, Value};
///
/// let positive_height = Rule::new(
///     "PositiveHeight",
///     ClassId::new("Example", "Column"),
///     |_store, column| match column.properties().get("Height") {
///         Some(Value::Real(height)) if *height <= 0.0 => Err(format!("height is {height} m")),
///         _ => Ok(()),
///     },
/// );
/// ```
#[derive(Clone)]
pub struct Rule {
    name: String,
    class: ClassId,
    check: Arc<Check>,
}

impl Rule {
    /// A rule called `name` on the elements of `class`. The name identifies
    /// the rule in a [`Violation`], so the UI can explain it.
    pub fn new(
        name: impl Into<String>,
        class: ClassId,
        check: impl Fn(&StoreView<'_>, &Element) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            class,
            check: Arc::new(check),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn class(&self) -> &ClassId {
        &self.class
    }

    /// Checks one element.
    pub(crate) fn check(&self, view: &StoreView<'_>, element: &Element) -> Result<(), Violation> {
        (self.check)(view, element).map_err(|message| Violation {
            rule: self.name.clone(),
            element: element.id(),
            message,
        })
    }
}

impl fmt::Debug for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rule")
            .field("name", &self.name)
            .field("class", &self.class)
            .finish_non_exhaustive()
    }
}

/// An element that breaks a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// The rule's name.
    pub rule: String,
    pub element: ElementId,
    /// What the rule found, in the domain's words.
    pub message: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            rule,
            element,
            message,
        } = self;
        write!(f, "element {element} breaks rule `{rule}`: {message}")
    }
}
