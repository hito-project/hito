//! The selected elements, shared by every view and the properties panel,
//! as in Revit. Selecting isn't a model change, so it isn't undone.

use std::collections::BTreeSet;

use hito_core::{ElementId, ElementStore};

/// How a click or a box changes the selection, as in Revit: on its own it
/// replaces it, with Ctrl it adds to it, with Shift it removes from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Replace,
    Add,
    Remove,
}

impl Mode {
    /// The mode the held keys choose. Ctrl is Cmd on macOS.
    pub fn from_keys(ctrl: bool, shift: bool) -> Self {
        if ctrl {
            Mode::Add
        } else if shift {
            Mode::Remove
        } else {
            Mode::Replace
        }
    }
}

/// The selected elements.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    ids: BTreeSet<ElementId>,
}

impl Selection {
    pub fn ids(&self) -> &BTreeSet<ElementId> {
        &self.ids
    }

    pub fn contains(&self, id: ElementId) -> bool {
        self.ids.contains(&id)
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn clear(&mut self) {
        self.ids.clear();
    }

    /// Changes the selection with what a click or a box picked. Replacing
    /// with nothing, a click on empty space, clears it.
    pub fn apply(&mut self, picked: impl IntoIterator<Item = ElementId>, mode: Mode) {
        match mode {
            Mode::Replace => self.ids = picked.into_iter().collect(),
            Mode::Add => self.ids.extend(picked),
            Mode::Remove => {
                for id in picked {
                    self.ids.remove(&id);
                }
            }
        }
    }

    /// Drops elements the model no longer has, after a change.
    pub fn retain_existing(&mut self, store: &ElementStore) {
        self.ids.retain(|&id| store.get(id).is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_replaces_ctrl_adds_shift_removes() {
        let [a, b, c] = [(); 3].map(|_| ElementId::generate());
        let mut selection = Selection::default();
        selection.apply([a], Mode::from_keys(false, false));
        selection.apply([b, c], Mode::from_keys(true, false));
        assert_eq!(selection.len(), 3);
        selection.apply([b], Mode::from_keys(false, true));
        assert!(selection.contains(a) && !selection.contains(b) && selection.contains(c));
        selection.apply([b], Mode::Replace);
        assert_eq!(selection.ids().iter().copied().collect::<Vec<_>>(), [b]);
        selection.apply([], Mode::Replace);
        assert!(selection.is_empty(), "a click on nothing clears it");
    }

    #[test]
    fn ctrl_wins_over_shift() {
        assert_eq!(Mode::from_keys(true, true), Mode::Add);
    }
}
