//! The properties panel for the selected elements (#26), drawn from the
//! [parameters](crate::parameters) their classes describe, as Revit's
//! properties palette: the type selector at the top, then the selection's
//! name and an Edit Type button, then its parameters by group. Each value
//! typed is one transaction, so one undo step, for every selected element.
//!
//! Edit Type opens the type editor, Revit's Type Properties dialog: its
//! changes apply to every element of the type, which it warns about, and
//! are made together when the user presses OK or Apply.

use eframe::egui;
use hito_core::{Dimension, ElementId, ElementStore, StoreView, Value};

use super::elements::{self, Edit, Elements};
use crate::command::{Input, InputFormat, Wants};
use crate::i18n::I18n;
use crate::parameters::{self, Editor, Row, Shared};

/// The type editor, while it's open.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TypeEditor {
    type_id: ElementId,
    /// The values changed since it opened or since Apply, not yet made.
    pending: Vec<(&'static str, Value)>,
}

impl TypeEditor {
    fn pending(&self, property: &str) -> Option<&Value> {
        let found = self.pending.iter().find(|(p, _)| *p == property);
        found.map(|(_, value)| value)
    }

    fn set(&mut self, property: &'static str, value: Value) {
        self.pending.retain(|(p, _)| *p != property);
        self.pending.push((property, value));
    }
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
                pending: Vec::new(),
            });
        }
    });

    let rows = parameters::rows(&view, &ids);
    let mut set = |property, value| {
        edits.push(Edit::Set {
            elements: ids.clone(),
            values: vec![(property, value)],
        });
    };
    let mut failed = None;
    grouped(ui, i18n, &rows, "instance", |ui, row| {
        let shown = Shown::of(input, i18n, row);
        match edit_row(ui, i18n, input, row, &shown, ("instance", ids[0])) {
            Some(Ok(value)) => set(row.parameter.property, value),
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
/// showing its type, or nothing if the elements have different ones.
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

/// A row's value as the panel shows it.
enum Shown {
    /// As text, empty if it varies, with the hint shown when it's empty.
    Text { text: String, hint: String },
    /// A check box's state, `None` if it varies.
    Check(Option<bool>),
}

impl Shown {
    fn of(input: &InputFormat, i18n: &I18n, row: &Row) -> Self {
        match (row.parameter.editor, &row.value) {
            (Editor::Boolean, Shared::Same(value)) => {
                Shown::Check(Some(matches!(value, Some(Value::Boolean(true)))))
            }
            (Editor::Boolean, Shared::Varies) => Shown::Check(None),
            (Editor::Quantity(dimension), Shared::Same(value)) => Shown::Text {
                text: match value {
                    Some(Value::Real(x)) => input.format(*x, dimension),
                    _ => String::new(),
                },
                hint: String::new(),
            },
            (Editor::Quantity(_), Shared::Varies) => Shown::Text {
                text: String::new(),
                hint: i18n.tr("properties-varies"),
            },
        }
    }
}

/// One parameter's row: its label and its editor, linked for screen
/// readers, or a greyed-out value if it's computed. Returns a value the
/// user entered, or why what they typed isn't one.
fn edit_row(
    ui: &mut egui::Ui,
    i18n: &I18n,
    input: &InputFormat,
    row: &Row,
    shown: &Shown,
    salt: (&str, ElementId),
) -> Option<Result<Value, String>> {
    let label = i18n.tr(row.parameter.label);
    let property = row.parameter.property;
    match (shown, row.parameter.editor) {
        (Shown::Check(state), _) => {
            let label = ui.label(label);
            let mut checked = state.unwrap_or(false);
            let checkbox =
                egui::Checkbox::without_text(&mut checked).indeterminate(state.is_none());
            let response = ui
                .add_enabled(!row.computed, checkbox)
                .labelled_by(label.id);
            ui.end_row();
            response.changed().then_some(Ok(Value::Boolean(checked)))
        }
        (Shown::Text { text, hint }, _) if row.computed => {
            let label = ui.label(label);
            let mut text = text.clone();
            let field = egui::TextEdit::singleline(&mut text).hint_text(hint.as_str());
            ui.add_enabled(false, field).labelled_by(label.id);
            ui.end_row();
            None
        }
        (Shown::Text { text, hint }, Editor::Quantity(dimension)) => {
            let typed = elements::text_row(ui, &label, (salt, property), text, hint)?;
            Some(parse(i18n, input, &typed, dimension))
        }
        (Shown::Text { .. }, Editor::Boolean) => unreachable!("booleans show a check box"),
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

/// The type editor, as Revit's Type Properties dialog: the type's name,
/// a warning that its changes apply to every element of the type, its
/// parameters, and OK, Cancel and Apply. OK and Apply make every change
/// in one transaction.
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
                ui.label(parameters::type_name(&view, type_id))
                    .labelled_by(label.id);
                ui.end_row();
            });
        ui.colored_label(
            ui.visuals().warn_fg_color,
            i18n.tr_with("type-editor-warning", "count", count),
        );
        ui.separator();
        grouped(ui, i18n, &rows, "type", |ui, row| {
            let mut row = row.clone();
            if let Some(value) = editor.pending(row.parameter.property) {
                row.value = Shared::Same(Some(value.clone()));
            }
            let shown = Shown::of(input, i18n, &row);
            match edit_row(ui, i18n, input, &row, &shown, ("type", type_id)) {
                Some(Ok(value)) => editor.set(row.parameter.property, value),
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
    if (ok || apply) && !editor.pending.is_empty() {
        edits.push(Edit::Set {
            elements: vec![type_id],
            values: std::mem::take(&mut editor.pending),
        });
    }
    if ok || cancel || modal.should_close() {
        elements.type_editor = None;
    }
    if let Some(message) = failed {
        elements.fail(message);
    }
}
