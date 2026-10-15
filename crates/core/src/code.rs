//! Codes: optional, human-readable names for elements
//! ([element model §1](https://github.com/hito-project/hito/blob/main/docs/architecture/element-model.md#1-identity-128-bit-ids-generated-locally)).
//!
//! Structural marks are the first use. Argentine drawings label columns,
//! beams and slabs C, V and L, numbered per floor, so `C12` is unique on its
//! floor but not in the building. A code is therefore unique only within a
//! **scope** (the floor, here), and only among codes that follow the same
//! **spec** (the marking rule).
//!
//! A code is a name, not the identity. Renaming `C12` to `C13` changes the
//! code and keeps the element's [`ElementId`].

use std::fmt;

use crate::ElementId;

/// The rule a code follows, such as structural marks. A domain names its own
/// specs; the core only compares them.
///
/// Codes with different specs never clash, even with the same scope and value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CodeSpec(String);

impl CodeSpec {
    /// The spec with this name. Domains prefix the name with their schema's,
    /// as in `Structural:Mark`, so specs from different domains can't clash.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The spec's name.
    pub fn name(&self) -> &str {
        &self.0
    }
}

/// An element's code: a value that is unique among the codes with the same
/// spec and scope.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Code {
    spec: CodeSpec,
    scope: ElementId,
    value: String,
}

impl Code {
    /// A code with this spec, scope and value. The scope is the element the
    /// code is unique within, such as a level for per-floor marks.
    ///
    /// Fails if the value is empty: an element without a name has no code.
    pub fn new(
        spec: CodeSpec,
        scope: ElementId,
        value: impl Into<String>,
    ) -> Result<Self, EmptyCodeError> {
        let value = value.into();
        if value.is_empty() {
            return Err(EmptyCodeError);
        }
        Ok(Self { spec, scope, value })
    }

    /// The rule this code follows.
    pub fn spec(&self) -> &CodeSpec {
        &self.spec
    }

    /// The element this code is unique within.
    pub fn scope(&self) -> ElementId {
        self.scope
    }

    /// The code's value, such as `C12`.
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// [`Code::new`] was given an empty value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyCodeError;

impl fmt::Display for EmptyCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a code's value can't be empty")
    }
}

impl std::error::Error for EmptyCodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_equal_only_with_the_same_spec_scope_and_value() {
        let mark = CodeSpec::new("Test:Mark");
        let floor_1 = ElementId::generate();
        let floor_2 = ElementId::generate();
        let c12 = Code::new(mark.clone(), floor_1, "C12").unwrap();

        assert_eq!(c12, Code::new(mark.clone(), floor_1, "C12").unwrap());
        assert_ne!(c12, Code::new(mark.clone(), floor_2, "C12").unwrap());
        assert_ne!(c12, Code::new(mark, floor_1, "C13").unwrap());
        assert_ne!(
            c12,
            Code::new(CodeSpec::new("Test:Other"), floor_1, "C12").unwrap()
        );
    }

    #[test]
    fn an_empty_value_is_not_a_code() {
        let spec = CodeSpec::new("Test:Mark");
        assert_eq!(
            Code::new(spec, ElementId::generate(), ""),
            Err(EmptyCodeError)
        );
    }
}
