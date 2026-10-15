//! Attribute values and entity instance names.

use std::borrow::Cow;
use std::fmt;

/// An entity instance name, written `#123`.
///
/// [`StepWriter::add`](crate::StepWriter::add) hands these out, numbered from
/// 1 in creation order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(u64);

impl EntityId {
    /// The instance name `#n`. Part 21 numbers start at 1.
    ///
    /// # Panics
    ///
    /// If `n` is 0.
    pub const fn new(n: u64) -> Self {
        assert!(n > 0, "Part 21 instance names start at #1");
        EntityId(n)
    }

    /// The number after the `#`.
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// One attribute value of an entity instance.
///
/// Names (of enumerations and typed values) are `Cow<'static, str>`, so the
/// usual constant names cost nothing and owned names still fit.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// `$`: an optional attribute that is not set.
    Null,
    /// `*`: an attribute that a subtype redeclares as derived.
    Derived,
    /// An INTEGER, such as `42` or `-7`.
    Integer(i64),
    /// A REAL, always written with a decimal point, such as `3.` or `1.5E-7`.
    /// It must be finite: Part 21 has no NaN or infinity.
    Real(f64),
    /// A STRING, written between quotes with `'`, `\` and non-ASCII escaped.
    String(String),
    /// An enumeration item, written `.NAME.`. The name is a keyword
    /// ([`is_keyword`](crate::is_keyword)), such as `NOTDEFINED`.
    /// A LOGICAL's unknown value is the enumeration `U`.
    Enumeration(Cow<'static, str>),
    /// A BOOLEAN, written `.T.` or `.F.`.
    Boolean(bool),
    /// A reference to another entity instance, written `#123`.
    Reference(EntityId),
    /// An aggregate (list, set, bag or array), written `(a,b,c)`.
    List(Vec<Value>),
    /// A typed value, written `NAME(value)`, as a SELECT needs to say which
    /// defined type a value has, such as `IFCLABEL('Viga')`.
    Typed(Cow<'static, str>, Box<Value>),
}

impl Value {
    /// An enumeration item, `.NAME.`.
    pub fn enumeration(name: impl Into<Cow<'static, str>>) -> Self {
        Value::Enumeration(name.into())
    }

    /// A typed value, `NAME(value)`.
    pub fn typed(name: impl Into<Cow<'static, str>>, value: impl Into<Value>) -> Self {
        Value::Typed(name.into(), Box::new(value.into()))
    }
}

impl From<f64> for Value {
    fn from(x: f64) -> Self {
        Value::Real(x)
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Integer(n)
    }
}

impl From<i32> for Value {
    fn from(n: i32) -> Self {
        Value::Integer(n.into())
    }
}

impl From<u32> for Value {
    fn from(n: u32) -> Self {
        Value::Integer(n.into())
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_owned())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Boolean(b)
    }
}

impl From<EntityId> for Value {
    fn from(id: EntityId) -> Self {
        Value::Reference(id)
    }
}

/// `None` is `$`.
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(o: Option<T>) -> Self {
        o.map_or(Value::Null, Into::into)
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(items: Vec<T>) -> Self {
        Value::List(items.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<Value>, const N: usize> From<[T; N]> for Value {
    fn from(items: [T; N]) -> Self {
        Value::List(items.into_iter().map(Into::into).collect())
    }
}

/// Builds an attribute list, converting each item with [`Value::from`]:
/// `args![origin, "Viga", Value::Null, [0.0, 0.0, 1.0]]`.
#[macro_export]
macro_rules! args {
    ($($x:expr),* $(,)?) => { ::std::vec![$($crate::Value::from($x)),*] };
}
