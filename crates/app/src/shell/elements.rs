//! Elements in the project browser and the properties panel (#33, #34):
//! the browser lists the levels and grids, and the properties panel edits
//! the selected one. Each edit is one transaction, so one undo step (#19).
//!
//! Revit edits levels in elevation views, which HITO doesn't have yet
//! (#197), so the browser is where levels are picked for now; grids are
//! also picked in plan views (#183). The browser picks into the one
//! selection the views share (#30), and the properties panel and Delete
//! act on it.

use eframe::egui;
use hito_core::{Dimension, ElementId, ElementStore, StoreView, Value};
use hito_structural::{class, grid, level};

use crate::command::{Input, InputFormat, Wants};
use crate::i18n::I18n;
use crate::selection::{Mode, Selection};

use super::properties::TypeEditor;
use crate::parameters::{Current, Parameter};

/// What the browser and the properties panel remember between frames.
#[derive(Debug, Default)]
pub(super) struct Elements {
    /// The selected elements, picked in the browser or in the 3D views.
    selection: Selection,
    /// Why the last edit couldn't be made, shown under the properties.
    error: Option<String>,
    /// A deletion that removes more than the selected elements, while the
    /// user is asked to confirm it.
    confirm: Option<Deletion>,
    /// The type being edited in the type editor, if it's open.
    pub(super) type_editor: Option<TypeEditor>,
}

/// A deletion waiting for the user to confirm it.
#[derive(Debug, Clone, PartialEq)]
struct Deletion {
    /// The elements the user asked to delete.
    selected: Vec<ElementId>,
    /// How many elements go in all, with the ones attached to them.
    deleted: usize,
}

/// A change the user asked for, made after the panels are drawn.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Edit {
    /// Sets properties of one or more elements, in one transaction: a
    /// value typed for several selected elements, or a type editor's
    /// changes.
    Set {
        elements: Vec<ElementId>,
        values: Vec<(&'static str, Value)>,
    },
    /// Gives elements another type, as Revit's type selector does.
    SetType {
        elements: Vec<ElementId>,
        type_id: ElementId,
    },
    /// Sets parameters of one or more elements, in one transaction,
    /// through the domains' operations: a value typed or picked in the
    /// properties panel for the selected elements.
    Change {
        elements: Vec<ElementId>,
        changes: Vec<(&'static Parameter, Current)>,
    },
    /// The type editor's OK or Apply: its changes to a type, made in one
    /// transaction. With `duplicate`, they're made to a copy of the type
    /// under that name, which `instances` then take.
    TypeChanges {
        type_id: ElementId,
        duplicate: Option<String>,
        changes: Vec<(&'static Parameter, Current)>,
        instances: Vec<ElementId>,
    },
    /// Deletes elements, asking first if others go with them.
    Delete(Vec<ElementId>),
}

impl Elements {
    /// The selected element, when exactly one is selected.
    pub fn selected(&self) -> Option<ElementId> {
        let mut ids = self.selection.ids().iter();
        match (ids.next(), ids.next()) {
            (Some(&id), None) => Some(id),
            _ => None,
        }
    }

    /// Every selected element.
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// Changes the selection, as a click in the browser or a view does.
    pub fn select(&mut self, ids: impl IntoIterator<Item = ElementId>, mode: Mode) {
        let before = self.selection.clone();
        self.selection.apply(ids, mode);
        if self.selection != before {
            self.error = None;
        }
    }

    /// Empties the selection, as Esc does.
    pub fn clear_selection(&mut self) {
        self.select([], Mode::Replace);
    }

    /// Forgets elements that no longer exist, such as a deleted level.
    pub fn forget_deleted(&mut self, store: &ElementStore) {
        self.selection.retain_existing(store);
    }

    pub fn fail(&mut self, message: String) {
        self.error = Some(message);
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    /// Why the last edit couldn't be made, if it couldn't.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Asks the user to confirm deleting `selected`, which takes `deleted`
    /// elements in all with the ones attached to them.
    pub fn ask_to_delete(&mut self, selected: Vec<ElementId>, deleted: usize) {
        self.confirm = Some(Deletion { selected, deleted });
    }

    pub fn is_asking(&self) -> bool {
        self.confirm.is_some()
    }
}

/// The kinds of element the browser lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Level,
    Grid,
}

impl Kind {
    fn of(view: &StoreView<'_>, id: ElementId) -> Option<Kind> {
        let registry = view.registry();
        let class_of = view.get(id)?.class();
        [(Kind::Level, level::LEVEL), (Kind::Grid, grid::GRID)]
            .into_iter()
            .find(|(_, name)| registry.is_a(class_of, &class(name)))
            .map(|(kind, _)| kind)
    }

    /// Every element of this kind, in the browser's order: levels lowest
    /// first, grids in the order they were drawn.
    fn all(self, view: &StoreView<'_>) -> Vec<ElementId> {
        match self {
            Kind::Level => level::levels(view).into_iter().map(|(id, _)| id).collect(),
            Kind::Grid => {
                let mut grids: Vec<_> = view
                    .elements()
                    .map(|e| e.id())
                    .filter(|&id| Kind::of(view, id) == Some(Kind::Grid))
                    .collect();
                grids.sort();
                grids
            }
        }
    }

    /// The message keys of an empty and of a taken name.
    fn name_problems(self) -> (&'static str, &'static str) {
        match self {
            Kind::Level => ("level-name-empty", "level-name-taken"),
            Kind::Grid => ("grid-name-empty", "grid-name-taken"),
        }
    }
}

/// A level's name or a grid's label, or an empty string if it has none.
/// Both are the `Name` property.
fn name<'a>(view: &StoreView<'a>, id: ElementId) -> &'a str {
    debug_assert_eq!(level::NAME, grid::NAME);
    match view.property(id, level::NAME) {
        Some(Value::Text(name)) => name,
        _ => "",
    }
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
    browser_list(ui, i18n, store, Kind::Level, elements, edits);
}

/// The project browser's grids, in the order they were drawn, picked and
/// deleted as levels are.
pub(super) fn browser_grids(
    ui: &mut egui::Ui,
    i18n: &I18n,
    store: &ElementStore,
    elements: &mut Elements,
    edits: &mut Vec<Edit>,
) {
    browser_list(ui, i18n, store, Kind::Grid, elements, edits);
}

fn browser_list(
    ui: &mut egui::Ui,
    i18n: &I18n,
    store: &ElementStore,
    kind: Kind,
    elements: &mut Elements,
    edits: &mut Vec<Edit>,
) {
    let view = store.view();
    let all = kind.all(&view);
    if all.is_empty() {
        ui.weak(i18n.tr("browser-none"));
    }
    for id in all {
        let picked = elements.selection.contains(id);
        let response = ui.selectable_label(picked, name(&view, id));
        if response.clicked() {
            // As in the views: Ctrl adds and Shift removes.
            let mode = ui.input(|i| Mode::from_keys(i.modifiers.command, i.modifiers.shift));
            elements.select([id], mode);
        }
        response.context_menu(|ui| {
            if ui.button(i18n.tr("action-delete")).clicked() {
                edits.push(Edit::Delete(vec![id]));
            }
        });
    }
}

/// The selected level's or grid's properties, editable, when one of them
/// is the only element selected. Returns whether it showed them; if not,
/// the panel shows the selection read-only, or with nothing selected the
/// active view's, as in Revit.
pub(super) fn properties(
    ui: &mut egui::Ui,
    i18n: &I18n,
    input: &InputFormat,
    store: &ElementStore,
    elements: &mut Elements,
    edits: &mut Vec<Edit>,
) -> bool {
    let view = store.view();
    let Some((id, kind)) = elements
        .selected()
        .and_then(|id| Some((id, Kind::of(&view, id)?)))
    else {
        return false;
    };
    let (title, name_label) = match kind {
        Kind::Level => ("properties-kind-level", "properties-name"),
        Kind::Grid => ("properties-kind-grid", "properties-label"),
    };
    ui.strong(i18n.tr(title));
    let current_name = name(&view, id);
    egui::Grid::new("properties")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            let label = i18n.tr(name_label);
            if let Some(text) = text_row(ui, &label, ("name", id), current_name, "") {
                match rename(&view, kind, id, &text) {
                    Ok(value) => edits.push(Edit::Set {
                        elements: vec![id],
                        values: vec![(level::NAME, value)],
                    }),
                    Err((key, name)) => elements.fail(i18n.tr_with(key, "name", name)),
                }
            }
            if kind != Kind::Level {
                return;
            }
            let elevation = match view.property(id, level::ELEVATION) {
                Some(Value::Real(e)) => *e,
                _ => 0.0,
            };
            let label = i18n.tr("properties-elevation");
            let shown = input.format(elevation, Dimension::LENGTH);
            if let Some(text) = text_row(ui, &label, ("level-elevation", id), &shown, "") {
                match input.parse(&text, Wants::Quantity(Dimension::LENGTH)) {
                    Ok(Input::Quantity(value)) => edits.push(Edit::Set {
                        elements: vec![id],
                        values: vec![(level::ELEVATION, value.into())],
                    }),
                    Ok(Input::Point(_) | Input::Element(_)) => {
                        unreachable!("a quantity was asked for")
                    }
                    Err(error) => elements.fail(i18n.tr(error.message())),
                }
            }
        });
    if let Some(error) = &elements.error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    true
}

/// Checks a new name for element `id`: not empty, and not another
/// element's of the same kind, as in Revit. Returns the trimmed name, or
/// the message key of the problem with the name it's about.
fn rename(
    view: &StoreView<'_>,
    kind: Kind,
    id: ElementId,
    text: &str,
) -> Result<Value, (&'static str, String)> {
    let (empty, taken_key) = kind.name_problems();
    let text = text.trim();
    if text.is_empty() {
        return Err((empty, String::new()));
    }
    let taken = kind
        .all(view)
        .into_iter()
        .any(|other| other != id && name(view, other) == text);
    if taken {
        return Err((taken_key, text.to_owned()));
    }
    Ok(Value::Text(text.to_owned()))
}

/// A property row with a text field, labelled for screen readers. The
/// field shows `current` until it's focused, or `hint` greyed out when
/// `current` is empty, keeps what's typed while it is, and returns the
/// text when the user presses Enter or leaves the field with a change. Esc
/// leaves it unchanged.
pub(super) fn text_row(
    ui: &mut egui::Ui,
    label: &str,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    current: &str,
    hint: &str,
) -> Option<String> {
    let id = ui.id().with(id_salt);
    let typing = id.with("typing");
    let label = ui.label(label);
    let mut text = ui
        .data(|d| d.get_temp::<String>(typing))
        .unwrap_or_else(|| current.to_owned());
    let response = ui
        .add(egui::TextEdit::singleline(&mut text).id(id).hint_text(hint))
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

/// Asks before a deletion that removes more than the selected elements, as
/// Revit warns. Returns the elements to delete once the user confirms.
pub(super) fn confirm_delete(
    ctx: &egui::Context,
    i18n: &I18n,
    store: &ElementStore,
    elements: &mut Elements,
) -> Option<Vec<ElementId>> {
    let deletion = elements.confirm.as_ref()?;
    let others = deletion.deleted - deletion.selected.len();
    let message = match deletion.selected.as_slice() {
        [one] => i18n.tr_args(
            "delete-also",
            [
                ("name", name(&store.view(), *one).to_owned().into()),
                ("count", others.into()),
            ],
        ),
        several => i18n.tr_args(
            "delete-also-selected",
            [("selected", several.len().into()), ("count", others.into())],
        ),
    };
    let mut confirmed = false;
    let mut cancelled = false;
    let modal = egui::Modal::new(egui::Id::new("confirm-delete")).show(ctx, |ui| {
        ui.label(message);
        ui.horizontal(|ui| {
            confirmed = ui.button(i18n.tr("action-delete")).clicked();
            cancelled = ui.button(i18n.tr("action-cancel")).clicked();
        });
    });
    let selected = deletion.selected.clone();
    if confirmed || cancelled || modal.should_close() {
        elements.confirm = None;
    }
    confirmed.then_some(selected)
}
