//! Provenance: where an imported or linked element came from
//! ([element model §1](https://github.com/hito-project/hito/blob/main/docs/architecture/element-model.md#1-identity-128-bit-ids-generated-locally)).
//!
//! An element imported from another file records its identifier in that
//! file. Re-importing the file then finds the element by that identifier and
//! updates it, instead of deleting it and creating a new one with a new
//! [`ElementId`]. That keeps IDs stable for everything that refers to them,
//! such as external schedules and budgets (SR-15).
//!
//! This follows BIS's `ExternalSourceAspect`.

use crate::ElementId;

/// The identity of an element in the external source it came from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExternalSource {
    source: ElementId,
    identifier: String,
}

impl ExternalSource {
    /// An element that `source` identifies as `identifier`.
    ///
    /// `source` is the element that stands for the external file or system,
    /// so two files can use the same identifier without clashing.
    /// `identifier` is whatever the source uses, such as an IFC `GlobalId` or
    /// a DWG handle, kept exactly as the source wrote it.
    pub fn new(source: ElementId, identifier: impl Into<String>) -> Self {
        Self {
            source,
            identifier: identifier.into(),
        }
    }

    /// The element that stands for the external source.
    pub fn source(&self) -> ElementId {
        self.source
    }

    /// The element's identifier in the source.
    pub fn identifier(&self) -> &str {
        &self.identifier
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_identifier_in_two_sources_is_two_different_elements() {
        let file_a = ElementId::generate();
        let file_b = ElementId::generate();
        let global_id = "2O2Fr$t4X7Zf8NOew3FLOH";
        assert_ne!(
            ExternalSource::new(file_a, global_id),
            ExternalSource::new(file_b, global_id)
        );
        assert_eq!(
            ExternalSource::new(file_a, global_id).identifier(),
            global_id
        );
    }
}
