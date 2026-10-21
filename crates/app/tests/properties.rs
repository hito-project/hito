//! The properties panel (#26), driven headless through the window's
//! AccessKit tree, as a screen reader would: the selection's parameters,
//! each value labelled, the type selector and the type editor.

use eframe::egui;
use eframe::egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use hito::Shell;
use hito::i18n::Locale;
use hito::selection::Mode;
use hito_core::{ElementId, Value};
use hito_structural::{class, column, section};

/// The window in English, with no saved layout.
fn window() -> Harness<'static, Shell> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_step_dt(0.05)
        .build_eframe(|_cc| Shell::restore(None, Locale::En));
    h.run_steps(2);
    h
}

/// The command line's text field: the lowest text field in the window,
/// since the command line runs along its bottom edge. The properties panel
/// and the options bar show text fields of their own above it.
fn command_input<'h>(h: &'h Harness<'_, Shell>) -> egui_kittest::Node<'h> {
    let inputs = h.get_all_by_role(Role::TextInput);
    let lowest = inputs.max_by(|a, b| a.rect().bottom().total_cmp(&b.rect().bottom()));
    lowest.expect("the command line")
}

fn command_line(h: &mut Harness<'_, Shell>, text: &str) {
    command_input(h).focus();
    h.run_steps(1);
    command_input(h).type_text(text);
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
}

/// Places columns of the generic type at these points on Level 1, and
/// selects them.
fn columns(h: &mut Harness<'_, Shell>, points: &[&str]) -> Vec<ElementId> {
    command_line(h, "CL");
    for point in points {
        command_line(h, point);
    }
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    let ids = column::columns(&h.state().project().view());
    h.state_mut().select(ids.iter().copied(), Mode::Replace);
    h.run_steps(2);
    ids
}

fn real(h: &Harness<'_, Shell>, id: ElementId, property: &str) -> f64 {
    match h.state().project().property(id, property) {
        Some(Value::Real(x)) => *x,
        other => panic!("{property}: {other:?}"),
    }
}

/// The text of the property field labelled `label`.
fn field(h: &Harness<'_, Shell>, label: &str) -> String {
    let field = h.get_by_role_and_label(Role::TextInput, label);
    field.value().unwrap_or_default()
}

/// Replaces the text of the property field labelled `label` and presses
/// Enter.
fn set_field(h: &mut Harness<'_, Shell>, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label).focus();
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.get_by_role_and_label(Role::TextInput, label)
        .type_text(text);
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
}

fn shows(h: &Harness<'_, Shell>, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

#[test]
fn a_columns_parameters_are_grouped_labelled_and_edited() {
    let mut h = window();
    let ids = columns(&mut h, &["0,0"]);
    let id = ids[0];
    assert!(shows(&h, "Column"), "the title");
    assert!(shows(&h, "Constraints"), "Revit's group");
    assert_eq!(field(&h, "Base Offset"), "0 m");
    assert_eq!(field(&h, "Unconnected Height"), "3 m");
    assert_eq!(field(&h, "Rotation"), "0.00°", "in degrees");

    set_field(&mut h, "Base Offset", "20 cm");
    assert!((real(&h, id, column::BASE_OFFSET) - 0.2).abs() < 1e-12);
    assert_eq!(field(&h, "Base Offset"), "0.2 m");
    set_field(&mut h, "Rotation", "90");
    let rotation = real(&h, id, column::ROTATION);
    assert!((rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-12);

    // Not a length: nothing changes, and the panel says why.
    set_field(&mut h, "Unconnected Height", "tall");
    assert_eq!(real(&h, id, column::HEIGHT), 3.0);
    assert!(
        shows(&h, "Enter a value, such as 3.5 or 350 cm"),
        "says why"
    );
}

#[test]
fn several_elements_show_what_varies_and_take_one_value() {
    let mut h = window();
    let ids = columns(&mut h, &["0,0", "5,0"]);
    assert!(shows(&h, "Column (2)"));
    set_field(&mut h, "Base Offset", "10 cm");
    for &id in &ids {
        assert!((real(&h, id, column::BASE_OFFSET) - 0.1).abs() < 1e-12);
    }

    // Give one of them another offset: the field is empty, with "(varies)".
    h.state_mut().select([ids[0]], Mode::Replace);
    h.run_steps(2);
    set_field(&mut h, "Base Offset", "30 cm");
    h.state_mut().select(ids.iter().copied(), Mode::Replace);
    h.run_steps(2);
    assert_eq!(field(&h, "Base Offset"), "");
    assert_eq!(field(&h, "Unconnected Height"), "3 m", "the same in both");
}

/// The selected column's type.
fn type_of(h: &Harness<'_, Shell>, id: ElementId) -> ElementId {
    h.state().project().get(id).unwrap().type_id().unwrap()
}

/// Adds a 40 cm round column type to the project.
fn round_type(h: &mut Harness<'_, Shell>) -> ElementId {
    let shell = h.state_mut();
    let project = shell.project_mut();
    let definitions = project
        .models()
        .find(|m| m.kind() == hito_core::ModelKind::Definition)
        .unwrap()
        .id();
    let mut tx = project.transaction();
    let round = section::Section::Circle { diameter: 0.4 };
    let id = section::insert(
        &mut tx,
        definitions,
        column::COLUMN_TYPE,
        "Round 400",
        round,
        None,
    )
    .unwrap();
    tx.commit().unwrap();
    id
}

#[test]
fn the_type_selector_changes_the_selections_type() {
    let mut h = window();
    let ids = columns(&mut h, &["0,0", "5,0"]);
    let round = round_type(&mut h);
    h.run_steps(2);
    let generic = type_of(&h, ids[0]);
    let generic_name = hito::parameters::type_name(&h.state().project().view(), generic);

    let selector = h.get_by_role_and_label(Role::ComboBox, "Type");
    assert_eq!(selector.value().as_deref(), Some(generic_name.as_str()));
    selector.click();
    h.run_steps(2);
    h.get_by_label("Round 400").click();
    h.run_steps(2);
    for &id in &ids {
        assert_eq!(type_of(&h, id), round, "both change");
    }
    let registry = h.state().project().registry();
    let round_class = h.state().project().get(round).unwrap().class().clone();
    assert!(registry.is_a(&round_class, &class(column::COLUMN_TYPE)));
}

#[test]
fn the_type_editor_warns_and_changes_every_instance_on_ok() {
    let mut h = window();
    let ids = columns(&mut h, &["0,0", "5,0"]);
    h.state_mut().select([ids[0]], Mode::Replace);
    h.run_steps(2);
    let column_type = type_of(&h, ids[0]);
    let width = |h: &Harness<'_, Shell>| real(h, column_type, section::WIDTH);
    let before = width(&h);

    h.get_by_role_and_label(Role::Button, "Edit Type").click();
    h.run_steps(2);
    assert!(shows(&h, "Type Properties"));
    assert!(shows(
        &h,
        "Changes apply to all \u{2068}2\u{2069} elements of this type."
    ));
    set_field(&mut h, "Width (b)", "45 cm");
    assert_eq!(width(&h), before, "not until OK");
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run_steps(2);
    assert_eq!(width(&h), before, "cancelled");
    assert!(!shows(&h, "Type Properties"));

    h.get_by_role_and_label(Role::Button, "Edit Type").click();
    h.run_steps(2);
    set_field(&mut h, "Width (b)", "45 cm");
    set_field(&mut h, "Depth (h)", "60 cm");
    h.get_by_role_and_label(Role::Button, "OK").click();
    h.run_steps(2);
    assert!((width(&h) - 0.45).abs() < 1e-12);
    assert!((real(&h, column_type, section::DEPTH) - 0.6).abs() < 1e-12);
    for &id in &ids {
        let section = section::of(&h.state().project().view(), id).unwrap();
        assert_eq!(
            section,
            section::Section::Rectangle {
                width: 0.45,
                depth: 0.6
            }
        );
    }
}
