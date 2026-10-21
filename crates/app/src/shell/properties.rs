//! The properties panel for the selected elements (#26), drawn from the
//! [parameters](crate::parameters) their classes describe, as Revit's
//! properties palette: the type selector at the top, then the selection's
//! name and an Edit Type button, then its parameters by group. Each value
//! typed or picked is one transaction, so one undo step, for every
//! selected element.
//!
//! Edit Type opens the type editor, Revit's Type Properties dialog: its
//! changes apply to every element of the type, which it warns about, and
//! are made together when the user presses OK or Apply. Duplicate makes a
//! copy of the type to edit, which the selection takes on OK, and Rename
//! renames the type, each asking for the name in a Name dialog.

use eframe::egui;
use hito_core::{Dimension, ElementId, ElementStore, StoreView, Value};

use super::elements::{self, Edit, Elements};
use crate::command::{Input, InputFormat, Wants};
use crate::i18n::I18n;
use crate::parameters::{self, Current, Editor, Label, NameProblem, Parameter, Row, Shared};

/// The type editor, while it's open.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TypeEditor {
    /// The type shown, or the one being copied.
    type_id: ElementId,
    /// The elements selected when it opened, which take a copy on OK.
    instances: Vec<ElementId>,
    /// The name of the copy Duplicate makes, until OK or Apply makes it.
    duplicate: Option<String>,
    /// The changes since it opened or since Apply, not yet made.
    pending: Vec<(&'static Parameter, Current)>,
    /// The Name dialog, while it's open.
    naming: Option<Naming>,
}

/// The Name dialog of Duplicate or Rename.
#[derive(Debug, Clone, PartialEq)]
struct Naming {
    renaming: bool,
    text: String,
    error: Option<String>,
}

impl TypeEditor {
    fn pending(&self, parameter: &Parameter) -> Option<&Current> {
        let found = self.pending.iter().find(|(p, _)| same(p, parameter));
        found.map(|(_, value)| value)
    }

    fn set(&mut self, parameter: &'static Parameter, value: Current) {
        self.pending.retain(|(p, _)| !same(p, parameter));
        self.pending.push((parameter, value));
    }

    /// Edits `type_id` from now on: the copy Apply just made.
    pub(super) fn now_editing(&mut self, type_id: ElementId) {
        self.type_id = type_id;
        self.duplicate = None;
    }

    /// Whether Apply or OK will make a copy of the type.
    pub(super) fn is_duplicating(&self) -> bool {
        self.duplicate.is_some()
    }

    /// The name the dialog shows: the copy's, a new one, or the type's.
    fn name(&self, view: &StoreView<'_>) -> String {
        if let Some(Current::Value(Value::Text(name))) = self.pending(&parameters::TYPE_NAME) {
            return name.clone();
        }
        let stored = || parameters::type_name(view, self.type_id);
        self.duplicate.clone().unwrap_or_else(stored)
    }
}

fn same(a: &Parameter, b: &Parameter) -> bool {
    a.property == b.property && a.label == b.label
}

/// The selected elements' properties, editable. Shown when the selection
/// isn't one level or one grid, which have their own rows.
pub(super) fn selection(
    ui: &mut egui::Ui,
    i18n: &I18n,
    input: &InputFormat,
    store: &ElementStore,
    elements: &mut Elements,
    edits: &mut Vec<Edit>,
) {
    let view = store.view();
    let ids: Vec<ElementId> = elements.selection().ids().iter().copied().collect();
    let Some(first) = ids.first().and_then(|&id| view.get(id)) else {
        return;
    };

    let choice = parameters::type_choice(&view, &ids);
    if let Some(choice) = &choice
        && let Some(type_id) = type_selector(ui, i18n, choice)
    {
        edits.push(Edit::SetType {
            elements: ids.clone(),
            type_id,
        });
    }

    let class = first.class();
    let same_class = ids
        .iter()
        .all(|&id| view.get(id).is_some_and(|e| e.class() == class));
    let name = match parameters::class_label(&view, class) {
        Some(key) => i18n.tr(key),
        None => class.name().to_owned(),
    };
    let title = match (ids.len(), same_class) {
        (1, _) => name,
        (n, true) => format!("{name} ({n})"),
        (n, false) => i18n.tr_with("properties-common", "count", n),
    };
    ui.horizontal(|ui| {
        ui.strong(title);
        // Revit edits the type of a selection that shares one.
        if let Some(Shared::Same(Some(type_id))) = choice.map(|c| c.current)
            && ui.button(i18n.tr("properties-edit-type")).clicked()
        {
            elements.type_editor = Some(TypeEditor {
                type_id,
                instances: ids.clone(),
                duplicate: None,
                pending: Vec::new(),
                naming: None,
            });
        }
    });

    let rows = parameters::rows(&view, &ids);
    let mut failed = None;
    grouped(ui, i18n, &rows, "instance", |ui, row| {
        match edit_row(ui, i18n, input, &view, row, ("instance", ids[0])) {
            Some(Ok(value)) => edits.push(Edit::Change {
                elements: ids.clone(),
                changes: vec![(row.parameter, value)],
            }),
            Some(Err(message)) => failed = Some(message),
            None => {}
        }
    });
    if let Some(message) = failed {
        elements.fail(message);
    }
    if let Some(error) = elements.error() {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
}

/// Revit's type selector: a list of the types the selection can take,
/// showing its type, or "(varies)" if the elements have different ones.
/// Returns the type the user picked, if they picked another.
fn type_selector(
    ui: &mut egui::Ui,
    i18n: &I18n,
    choice: &parameters::TypeChoice,
) -> Option<ElementId> {
    let current = match choice.current {
        Shared::Same(Some(id)) => Some(id),
        _ => None,
    };
    let shown = current
        .and_then(|id| choice.types.iter().find(|(t, _)| *t == id))
        .map_or_else(|| i18n.tr("properties-varies"), |(_, name)| name.clone());
    let mut picked = None;
    ui.horizontal(|ui| {
        let label = ui.label(i18n.tr("properties-type"));
        let combo = egui::ComboBox::from_id_salt("type-selector")
            .selected_text(shown)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for (id, name) in &choice.types {
                    let selected = current == Some(*id);
                    if ui.selectable_label(selected, name).clicked() && !selected {
                        picked = Some(*id);
                    }
                }
            });
        combo.response.labelled_by(label.id);
    });
    ui.separator();
    picked
}

/// Draws `rows` under a collapsible header for each of their groups, in
/// Revit's order, with `row` drawing each one in a two-column grid.
fn grouped(
    ui: &mut egui::Ui,
    i18n: &I18n,
    rows: &[Row],
    salt: &str,
    mut row: impl FnMut(&mut egui::Ui, &Row),
) {
    let mut groups: Vec<_> = rows.iter().map(|r| r.parameter.group).collect();
    groups.dedup();
    for group in groups {
        egui::CollapsingHeader::new(i18n.tr(group.label()))
            .id_salt((salt, group))
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new((salt, group, "rows"))
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        for r in rows.iter().filter(|r| r.parameter.group == group) {
                            row(ui, r);
                        }
                    });
            });
    }
}

/// A list choice's text.
fn label_text(i18n: &I18n, label: &Label) -> String {
    match label {
        Label::Text(text) => text.clone(),
        Label::Key(key) => i18n.tr(key),
        Label::UpToLevel(level) => i18n.tr_with("parameter-up-to-level", "level", level.as_str()),
    }
}

/// A value as a text field shows it, empty if there's none.
fn as_text(input: &InputFormat, editor: Editor, value: Option<&Current>) -> String {
    match (editor, value) {
        (Editor::Quantity(dimension), Some(Current::Value(Value::Real(x)))) => {
            input.format(*x, dimension)
        }
        (_, Some(Current::Value(Value::Text(text)))) => text.clone(),
        _ => String::new(),
    }
}

/// One parameter's row: its label and its editor, linked for screen
/// readers, greyed out if it's computed. Returns a value the user entered
/// or picked, or why what they typed isn't one.
fn edit_row(
    ui: &mut egui::Ui,
    i18n: &I18n,
    input: &InputFormat,
    view: &StoreView<'_>,
    row: &Row,
    salt: (&str, ElementId),
) -> Option<Result<Current, String>> {
    let label_text_ = i18n.tr(row.parameter.label);
    let editor = row.parameter.editor;
    let (value, varies) = match &row.value {
        Shared::Same(value) => (value.as_ref(), false),
        Shared::Varies => (None, true),
    };
    let hint = match varies {
        true => i18n.tr("properties-varies"),
        false => String::new(),
    };
    match editor {
        Editor::Boolean => {
            let label = ui.label(label_text_);
            let state = match value {
                Some(Current::Value(Value::Boolean(b))) => *b,
                _ => false,
            };
            let mut checked = state;
            let checkbox = egui::Checkbox::without_text(&mut checked).indeterminate(varies);
            let response = ui
                .add_enabled(!row.computed, checkbox)
                .labelled_by(label.id);
            ui.end_row();
            response
                .changed()
                .then_some(Ok(Current::Value(Value::Boolean(checked))))
        }
        _ if editor.is_list() => {
            let label = ui.label(label_text_);
            let choices = parameters::choices(view, row.parameter);
            let shown = match value {
                _ if varies => hint,
                Some(current) => choices
                    .iter()
                    .find(|(c, _)| c == current)
                    .map(|(_, l)| label_text(i18n, l))
                    .unwrap_or_default(),
                None => String::new(),
            };
            let mut picked = None;
            ui.add_enabled_ui(!row.computed, |ui| {
                let combo = egui::ComboBox::from_id_salt((salt, row.parameter.property, "list"))
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for (choice, choice_label) in &choices {
                            let selected = !varies && value == Some(choice);
                            let text = label_text(i18n, choice_label);
                            if ui.selectable_label(selected, text).clicked() && !selected {
                                picked = Some(choice.clone());
                            }
                        }
                    });
                combo.response.labelled_by(label.id);
            });
            ui.end_row();
            picked.map(Ok)
        }
        _ if row.computed => {
            let label = ui.label(label_text_);
            let mut text = as_text(input, editor, value);
            let field = egui::TextEdit::singleline(&mut text).hint_text(hint.as_str());
            ui.add_enabled(false, field).labelled_by(label.id);
            ui.end_row();
            None
        }
        Editor::Quantity(dimension) => {
            let text = as_text(input, editor, value);
            let property = row.parameter.property;
            let typed = elements::text_row(ui, &label_text_, (salt, property), &text, &hint)?;
            Some(parse(i18n, input, &typed, dimension).map(Current::Value))
        }
        _ => {
            let text = as_text(input, editor, value);
            let property = row.parameter.property;
            let typed = elements::text_row(ui, &label_text_, (salt, property), &text, &hint)?;
            Some(Ok(Current::Value(Value::Text(typed))))
        }
    }
}

/// Reads a typed quantity, in SI units.
fn parse(
    i18n: &I18n,
    input: &InputFormat,
    text: &str,
    dimension: Dimension,
) -> Result<Value, String> {
    match input.parse(text, Wants::Quantity(dimension)) {
        Ok(Input::Quantity(value)) => Ok(value.into()),
        Ok(Input::Point(_) | Input::Element(_)) => unreachable!("a quantity was asked for"),
        Err(error) => Err(i18n.tr(error.message())),
    }
}

/// The type editor, as Revit's Type Properties dialog: the type's name
/// with Duplicate and Rename, a warning that its changes apply to every
/// element of the type, its parameters, and OK, Cancel and Apply. OK and
/// Apply make every change in one transaction.
pub(super) fn type_editor(
    ctx: &egui::Context,
    i18n: &I18n,
    input: &InputFormat,
    store: &ElementStore,
    elements: &mut Elements,
    edits: &mut Vec<Edit>,
) {
    let Some(editor) = elements.type_editor.as_mut() else {
        return;
    };
    let view: StoreView<'_> = store.view();
    let type_id = editor.type_id;
    if view.get(type_id).is_none() {
        elements.type_editor = None;
        return;
    }
    let rows = parameters::rows(&view, &[type_id]);
    let count = parameters::instance_count(&view, type_id);
    let (mut ok, mut apply, mut cancel) = (false, false, false);
    let mut failed = None;
    let modal = egui::Modal::new(egui::Id::new("type-editor")).show(ctx, |ui| {
        ui.heading(i18n.tr("type-editor-title"));
        egui::Grid::new("type-editor-name")
            .num_columns(2)
            .show(ui, |ui| {
                let label = ui.label(i18n.tr("properties-type"));
                ui.label(editor.name(&view)).labelled_by(label.id);
                ui.end_row();
            });
        ui.horizontal(|ui| {
            for (key, renaming) in [
                ("type-editor-duplicate", false),
                ("type-editor-rename", true),
            ] {
                if ui.button(i18n.tr(key)).clicked() {
                    let text = match renaming {
                        true => editor.name(&view),
                        false => parameters::copy_name(&view, type_id),
                    };
                    let error = None;
                    editor.naming = Some(Naming {
                        renaming,
                        text,
                        error,
                    });
                }
            }
        });
        let warning = match editor.duplicate {
            Some(_) => i18n.tr_with("type-editor-new-type", "count", editor.instances.len()),
            None => i18n.tr_with("type-editor-warning", "count", count),
        };
        ui.colored_label(ui.visuals().warn_fg_color, warning);
        ui.separator();
        grouped(ui, i18n, &rows, "type", |ui, row| {
            let mut row = row.clone();
            if let Some(value) = editor.pending(row.parameter) {
                row.value = Shared::Same(Some(value.clone()));
            }
            match edit_row(ui, i18n, input, &view, &row, ("type", type_id)) {
                Some(Ok(value)) => editor.set(row.parameter, value),
                Some(Err(message)) => failed = Some(message),
                None => {}
            }
        });
        if let Some(message) = &failed {
            ui.colored_label(ui.visuals().error_fg_color, message);
        }
        ui.separator();
        ui.horizontal(|ui| {
            ok = ui.button(i18n.tr("action-ok")).clicked();
            cancel = ui.button(i18n.tr("action-cancel")).clicked();
            apply = ui.button(i18n.tr("action-apply")).clicked();
        });
    });
    name_dialog(ctx, i18n, &view, editor);
    let changed = !editor.pending.is_empty() || editor.duplicate.is_some();
    if (ok || apply) && changed {
        let duplicate = editor.duplicate.clone();
        edits.push(Edit::TypeChanges {
            type_id,
            instances: match duplicate {
                Some(_) => editor.instances.clone(),
                None => Vec::new(),
            },
            duplicate,
            changes: std::mem::take(&mut editor.pending),
        });
    }
    let naming = editor.naming.is_some();
    if ok || cancel || (modal.should_close() && !naming) {
        elements.type_editor = None;
    }
    if let Some(message) = failed {
        elements.fail(message);
    }
}

/// Revit's Name dialog, over the type editor: the name for Duplicate's
/// copy or for Rename, checked to be new among the types of its class.
fn name_dialog(ctx: &egui::Context, i18n: &I18n, view: &StoreView<'_>, editor: &mut TypeEditor) {
    let Some(naming) = editor.naming.as_mut() else {
        return;
    };
    let (mut ok, mut cancel) = (false, false);
    let modal = egui::Modal::new(egui::Id::new("type-name")).show(ctx, |ui| {
        ui.heading(i18n.tr("type-name-title"));
        egui::Grid::new("type-name-grid")
            .num_columns(2)
            .show(ui, |ui| {
                let label = ui.label(i18n.tr("properties-name"));
                let field = egui::TextEdit::singleline(&mut naming.text);
                let response = ui.add(field).labelled_by(label.id);
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    ok = true;
                }
                ui.end_row();
            });
        if let Some(error) = &naming.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        ui.horizontal(|ui| {
            ok |= ui.button(i18n.tr("action-ok")).clicked();
            cancel = ui.button(i18n.tr("action-cancel")).clicked();
        });
    });
    if cancel || modal.should_close() {
        editor.naming = None;
        return;
    }
    if !ok {
        return;
    }
    // Renaming a copy not made yet renames the copy.
    let renaming_type = naming.renaming && editor.duplicate.is_none();
    match parameters::check_type_name(view, editor.type_id, &naming.text, renaming_type) {
        Ok(name) => {
            match (naming.renaming, &mut editor.duplicate) {
                (true, Some(copy)) => *copy = name,
                (true, None) => {
                    let value = Current::Value(Value::Text(name));
                    editor.set(&parameters::TYPE_NAME, value);
                }
                (false, _) => editor.duplicate = Some(name),
            }
            editor.naming = None;
        }
        Err(NameProblem::Empty) => naming.error = Some(i18n.tr("type-name-empty")),
        Err(NameProblem::Taken(name)) => {
            naming.error = Some(i18n.tr_with("type-name-taken", "name", name));
        }
    }
}
