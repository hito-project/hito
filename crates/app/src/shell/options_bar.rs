//! The options bar under the ribbon, as Revit's: the running tool's values,
//! such as a wall's height and location line, changed by clicking
//! ([`Tool::options`](crate::command::Tool::options)).
//!
//! It keeps its height with no tool running, or a tool with no options,
//! so the views don't move when a tool starts.

use eframe::egui;

use crate::command::{Choice, Input, InputFormat, OptionValue, ToolOption, Wants};
use crate::i18n::I18n;

/// The bar's height, enough for one row of fields.
pub(super) const HEIGHT: f32 = 30.0;

/// Shows the options. Returns the one the user changed, with its new
/// value.
pub(super) fn ui(
    ui: &mut egui::Ui,
    i18n: &I18n,
    input: &InputFormat,
    options: &[ToolOption],
) -> Option<(&'static str, OptionValue)> {
    let error_id = egui::Id::new("options-bar-error");
    let mut changed = None;
    ui.horizontal_centered(|ui| {
        for option in options {
            ui.add_enabled_ui(option.enabled, |ui| {
                let label = ui.label(i18n.tr(option.key));
                let new = match &option.value {
                    OptionValue::Quantity { value, dimension } => {
                        let shown = input.format(*value, *dimension);
                        quantity(ui, label.id, option.key, &shown).and_then(|text| {
                            match input.parse(&text, Wants::Quantity(*dimension)) {
                                Ok(Input::Quantity(value)) => Some(OptionValue::Quantity {
                                    value,
                                    dimension: *dimension,
                                }),
                                Ok(_) => None,
                                Err(error) => {
                                    ui.data_mut(|d| d.insert_temp(error_id, error.message()));
                                    None
                                }
                            }
                        })
                    }
                    OptionValue::Choice { selected, choices } => {
                        choice(ui, i18n, label.id, option.key, *selected, choices).map(|selected| {
                            OptionValue::Choice {
                                selected,
                                choices: choices.clone(),
                            }
                        })
                    }
                };
                if let Some(value) = new {
                    ui.data_mut(|d| d.remove::<&'static str>(error_id));
                    changed = Some((option.key, value));
                }
            });
            ui.add_space(8.0);
        }
        if options.is_empty() {
            ui.data_mut(|d| d.remove::<&'static str>(error_id));
        }
        if let Some(error) = ui.data(|d| d.get_temp::<&'static str>(error_id)) {
            ui.colored_label(ui.visuals().error_fg_color, i18n.tr(error));
        }
    });
    changed
}

/// A quantity's field, labelled for screen readers. It shows `current`
/// until it's focused, keeps what's typed while it is, and returns the
/// text when the user presses Enter or leaves the field with a change, as
/// the properties' fields do. Esc leaves it unchanged.
fn quantity(ui: &mut egui::Ui, label: egui::Id, key: &str, current: &str) -> Option<String> {
    let id = egui::Id::new(("option", key));
    let typing = id.with("typing");
    let mut text = ui
        .data(|d| d.get_temp::<String>(typing))
        .unwrap_or_else(|| current.to_owned());
    let edit = egui::TextEdit::singleline(&mut text)
        .id(id)
        .desired_width(72.0);
    let response = ui.add(edit).labelled_by(label);
    if response.has_focus() {
        ui.data_mut(|d| d.insert_temp(typing, text));
        return None;
    }
    ui.data_mut(|d| d.remove::<String>(typing));
    let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
    (response.lost_focus() && !escaped && text != current).then_some(text)
}

/// A choice's drop-down list. Returns the index picked, if it changed.
fn choice(
    ui: &mut egui::Ui,
    i18n: &I18n,
    label: egui::Id,
    key: &str,
    selected: usize,
    choices: &[Choice],
) -> Option<usize> {
    let name = |choice: &Choice| match choice {
        Choice::Key(key) => i18n.tr(key),
        Choice::Element { name, .. } => name.clone(),
    };
    let shown = choices.get(selected).map(name).unwrap_or_default();
    let mut picked = None;
    let response = egui::ComboBox::from_id_salt(("option", key))
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (index, choice) in choices.iter().enumerate() {
                if ui
                    .selectable_label(index == selected, name(choice))
                    .clicked()
                    && index != selected
                {
                    picked = Some(index);
                }
            }
        });
    response.response.labelled_by(label);
    picked
}
