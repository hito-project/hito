//! Elements in the project browser and the properties panel (#33): the
//! browser lists the levels, and the properties panel edits the one picked
//! there. Each edit is one transaction, so one undo step (#19).
//!
//! Revit edits levels in elevation views, which HITO doesn't have yet
//! (#197), so the browser is where levels are picked for now. Picking in
//! the views comes with #30, which will share this selection.

use eframe::egui;
use hito_core::{Dimension, ElementId, ElementStore, StoreView, Value};
use hito_structural::{class, level};

use crate::command::{Input, InputFormat, Wants};
use crate::i18n::I18n;

/// What the browser and the properties panel remember between frames.
#[derive(Debug, Default)]
pub(super) struct Elements {
    /// The element picked in the project browser, whose properties show.
    selected: Option<ElementId>,
    /// Why the last edit couldn't be made, shown under the properties.
    error: Option<String>,
    /// What a deletion would remove, the picked element first, while the
    /// user is asked to confirm it.
    confirm: Option<Vec<ElementId>>,
}

/// A change the user asked for, made after the panels are drawn.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Edit {
    /// Sets an element's property.
    Set {
        element: ElementId,
        property: &'static str,
        value: Value,
    },
    /// Deletes an element, asking first if others go with it.
    Delete(ElementId),
}

impl Elements {
    /// The element picked in the project browser, if it still exists.
    pub fn selected(&self) -> Option<ElementId> {
        self.selected
    }

    pub fn select(&mut self, element: Option<ElementId>) {
        if self.selected != element {
            self.error = None;
        }
        self.selected = element;
    }

    /// Forgets elements that no longer exist, such as a deleted level.
    pub fn forget_deleted(&mut self, store: &ElementStore) {
        if self.selected.is_some_and(|id| store.get(id).is_none()) {
            self.select(None);
        }
    }

    pub fn fail(&mut self, message: String) {
        self.error = Some(message);
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    /// Asks the user to confirm deleting `deleted`, the first element and
    /// the ones that go with it.
    pub fn ask_to_delete(&mut self, deleted: Vec<ElementId>) {
        self.confirm = Some(deleted);
    }

    pub fn is_asking(&self) -> bool {
        self.confirm.is_some()
    }
}

/// A level's name, or an empty string if it has none.
fn name<'a>(view: &StoreView<'a>, id: ElementId) -> &'a str {
    match view.property(id, level::NAME) {
        Some(Value::Text(name)) => name,
        _ => "",
    }
}

fn is_level(view: &StoreView<'_>, id: ElementId) -> bool {
    let registry = view.registry();
    view.get(id)
        .is_some_and(|e| registry.is_a(e.class(), &class(level::LEVEL)))
}

/// The project browser's levels, lowest first. Clicking one picks it;
/// its context menu deletes it.
pub(super) fn browser_levels(
    ui: &mut egui::Ui,
    i18n: &I18n,
    store: &ElementStore,
    elements: &mut Elements,
    edits: &mut Vec<Edit>,
) {
    let view = store.view();
    let levels = level::levels(&view);
    if levels.is_empty() {
        ui.weak(i18n.tr("browser-none"));
    }
    for (id, _) in levels {
        let picked = elements.selected == Some(id);
        let response = ui.selectable_label(picked, name(&view, id));
        if response.clicked() {
            elements.select(Some(id));
        }
        response.context_menu(|ui| {
            if ui.button(i18n.tr("action-delete")).clicked() {
                edits.push(Edit::Delete(id));
            }
        });
    }
}

/// The picked element's properties, editable. Returns whether an element is
/// picked; with none, the panel shows the active view's, as in Revit.
pub(super) fn properties(
    ui: &mut egui::Ui,
    i18n: &I18n,
    input: &InputFormat,
    store: &ElementStore,
    elements: &mut Elements,
    edits: &mut Vec<Edit>,
) -> bool {
    let view = store.view();
    let Some(id) = elements.selected.filter(|&id| is_level(&view, id)) else {
        return false;
    };
    ui.strong(i18n.tr("properties-kind-level"));
    let current_name = name(&view, id);
    let elevation = match view.property(id, level::ELEVATION) {
        Some(Value::Real(e)) => *e,
        _ => 0.0,
    };
    egui::Grid::new("properties")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            let label = i18n.tr("properties-name");
            if let Some(text) = text_row(ui, &label, ("level-name", id), current_name) {
                match rename(&view, id, &text) {
                    Ok(value) => edits.push(Edit::Set {
                        element: id,
                        property: level::NAME,
                        value,
                    }),
                    Err((key, name)) => elements.fail(i18n.tr_with(key, "name", name)),
                }
            }
            let label = i18n.tr("properties-elevation");
            let shown = input.format(elevation, Dimension::LENGTH);
            if let Some(text) = text_row(ui, &label, ("level-elevation", id), &shown) {
                match input.parse(&text, Wants::Quantity(Dimension::LENGTH)) {
                    Ok(Input::Quantity(value)) => edits.push(Edit::Set {
                        element: id,
                        property: level::ELEVATION,
                        value: value.into(),
                    }),
                    Ok(Input::Point(_)) => unreachable!("a quantity was asked for"),
                    Err(error) => elements.fail(i18n.tr(error.message())),
                }
            }
        });
    if let Some(error) = &elements.error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    true
}

/// Checks a new name for level `id`: not empty, and no other level's.
/// Returns the trimmed name, or the message key of the problem with the
/// name it's about.
fn rename(
    view: &StoreView<'_>,
    id: ElementId,
    text: &str,
) -> Result<Value, (&'static str, String)> {
    let text = text.trim();
    if text.is_empty() {
        return Err(("level-name-empty", String::new()));
    }
    let taken = level::levels(view)
        .into_iter()
        .any(|(other, _)| other != id && name(view, other) == text);
    if taken {
        return Err(("level-name-taken", text.to_owned()));
    }
    Ok(Value::Text(text.to_owned()))
}

/// A property row with a text field, labelled for screen readers. The
/// field shows `current` until it's focused, keeps what's typed while it
/// is, and returns the text when the user presses Enter or leaves the field
/// with a change. Esc leaves it unchanged.
fn text_row(
    ui: &mut egui::Ui,
    label: &str,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    current: &str,
) -> Option<String> {
    let id = ui.id().with(id_salt);
    let typing = id.with("typing");
    let label = ui.label(label);
    let mut text = ui
        .data(|d| d.get_temp::<String>(typing))
        .unwrap_or_else(|| current.to_owned());
    let response = ui
        .add(egui::TextEdit::singleline(&mut text).id(id))
        .labelled_by(label.id);
    ui.end_row();
    if response.has_focus() {
        ui.data_mut(|d| d.insert_temp(typing, text));
        return None;
    }
    ui.data_mut(|d| d.remove::<String>(typing));
    let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
    (response.lost_focus() && !escaped && text != current).then_some(text)
}

/// Asks before a deletion that removes more than the picked element, as
/// Revit warns. Returns the element to delete once the user confirms.
pub(super) fn confirm_delete(
    ctx: &egui::Context,
    i18n: &I18n,
    store: &ElementStore,
    elements: &mut Elements,
) -> Option<ElementId> {
    let deleted = elements.confirm.as_ref()?;
    let first = deleted[0];
    let others = deleted.len() - 1;
    let view = store.view();
    let mut confirmed = false;
    let mut cancelled = false;
    let modal = egui::Modal::new(egui::Id::new("confirm-delete")).show(ctx, |ui| {
        ui.label(i18n.tr_args(
            "delete-also",
            [
                ("name", name(&view, first).to_owned().into()),
                ("count", others.into()),
            ],
        ));
        ui.horizontal(|ui| {
            confirmed = ui.button(i18n.tr("action-delete")).clicked();
            cancelled = ui.button(i18n.tr("action-cancel")).clicked();
        });
    });
    if confirmed || cancelled || modal.should_close() {
        elements.confirm = None;
    }
    confirmed.then_some(first)
}
