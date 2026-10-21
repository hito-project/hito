//! The properties panel (#26), driven headless through the window's
//! AccessKit tree, as a screen reader would: the selection's parameters,
//! each value labelled, the type selector and the type editor.

use eframe::egui;
use eframe::egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
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

/// Adds a level at `elevation` with the Level command, then ends it.
fn add_level(h: &mut Harness<'_, Shell>, elevation: &str) {
    command_line(h, "LL");
    command_line(h, elevation);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
}

/// Opens the list labelled `list` and picks `choice` in it.
fn pick(h: &mut Harness<'_, Shell>, list: &str, choice: &str) {
    h.get_by_role_and_label(Role::ComboBox, list).click();
    h.run_steps(2);
    h.get_by_label(choice).click();
    h.run_steps(2);
}

/// Draws a wall from `start` to `end` on Level 1 and selects it.
fn wall(h: &mut Harness<'_, Shell>, start: &str, end: &str) -> ElementId {
    use hito_architectural::wall;
    command_line(h, "WA");
    command_line(h, start);
    command_line(h, end);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    let project = h.state().project();
    let class = hito_architectural::class(wall::WALL);
    let id = project
        .elements()
        .find(|e| project.registry().is_a(e.class(), &class))
        .expect("the wall")
        .id();
    h.state_mut().select([id], Mode::Replace);
    h.run_steps(2);
    id
}

#[test]
fn a_walls_constraints_are_picked_from_lists() {
    use hito_architectural::wall::{self, LocationLine};
    let mut h = window();
    add_level(&mut h, "3");
    let id = wall(&mut h, "0,0", "5,0");
    // The Wall tool runs it up to the next level, as Revit does.
    let up_to = "Up to level: \u{2068}Level 2\u{2069}";
    let combo = h.get_by_role_and_label(Role::ComboBox, "Top Constraint");
    assert_eq!(combo.value().as_deref(), Some(up_to));

    pick(&mut h, "Top Constraint", "Unconnected");
    let view = h.state().project().view();
    assert_eq!(wall::top_level(&view, id), None);
    assert_eq!(wall::elevations(&view, id), Some((0.0, 3.0)), "same height");
    let height = h.get_by_role_and_label(Role::TextInput, "Unconnected Height");
    let typed = !height.accesskit_node().is_disabled();
    assert!(typed, "typed once unconnected");

    pick(&mut h, "Top Constraint", up_to);
    let view = h.state().project().view();
    assert!(wall::top_level(&view, id).is_some());
    assert_eq!(wall::elevations(&view, id), Some((0.0, 3.0)));
    let height = h.get_by_role_and_label(Role::TextInput, "Unconnected Height");
    assert!(
        height.accesskit_node().is_disabled(),
        "computed from the top level"
    );

    let faces = |h: &Harness<'_, Shell>| {
        let view = h.state().project().view();
        wall::interior_face(&view, id).unwrap()
    };
    let before = faces(&h);
    pick(&mut h, "Location Line", "Finish Face: Exterior");
    let view = h.state().project().view();
    assert_eq!(
        wall::location_line(&view, id),
        LocationLine::FinishFaceExterior
    );
    let after = faces(&h);
    for (a, b) in [(before.0, after.0), (before.1, after.1)] {
        assert!((a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
    }
}

#[test]
fn a_column_shows_its_base_level_and_is_unconnected_at_first() {
    let mut h = window();
    let ids = columns(&mut h, &["0,0"]);
    let base = h.get_by_role_and_label(Role::ComboBox, "Base Level");
    assert_eq!(base.value().as_deref(), Some("Level 1"));
    let top = h.get_by_role_and_label(Role::ComboBox, "Top Level");
    assert_eq!(top.value().as_deref(), Some("Unconnected"));
    let height = h.get_by_role_and_label(Role::TextInput, "Unconnected Height");
    assert!(
        !height.accesskit_node().is_disabled(),
        "typed while unconnected"
    );
    assert_eq!(column::top_level(&h.state().project().view(), ids[0]), None);
}

/// The column type called `name`, if any.
fn type_named(h: &Harness<'_, Shell>, name: &str) -> Option<ElementId> {
    let view = h.state().project().view();
    let types = class(column::COLUMN_TYPE);
    view.elements()
        .filter(|e| view.registry().is_a(e.class(), &types))
        .find(|e| hito::parameters::type_name(&view, e.id()) == name)
        .map(|e| e.id())
}

#[test]
fn duplicate_makes_a_new_type_the_selection_takes_on_ok() {
    let mut h = window();
    let ids = columns(&mut h, &["0,0", "5,0"]);
    h.state_mut().select([ids[0]], Mode::Replace);
    h.run_steps(2);
    let generic = type_of(&h, ids[0]);
    let generic_width = real(&h, generic, section::WIDTH);

    h.get_by_role_and_label(Role::Button, "Edit Type").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Duplicate...")
        .click();
    h.run_steps(2);
    let suggested = field(&h, "Name");
    assert!(suggested.ends_with(" 2"), "{suggested}");
    // Enter in the Name dialog is its OK.
    set_field(&mut h, "Name", "C 20 x 40");
    assert!(shows(
        &h,
        "A new type. On OK, the selected element takes it."
    ));
    set_field(&mut h, "Width (b)", "20 cm");
    assert_eq!(type_named(&h, "C 20 x 40"), None, "not until OK");
    h.get_by_role_and_label(Role::Button, "OK").click();
    h.run_steps(2);

    let copy = type_named(&h, "C 20 x 40").expect("the copy");
    assert!((real(&h, copy, section::WIDTH) - 0.2).abs() < 1e-12);
    assert_eq!(type_of(&h, ids[0]), copy, "the selected column takes it");
    assert_eq!(type_of(&h, ids[1]), generic, "the other keeps its type");
    assert_eq!(real(&h, generic, section::WIDTH), generic_width);
}

#[test]
fn rename_checks_the_name_and_renames_on_ok() {
    let mut h = window();
    let ids = columns(&mut h, &["0,0"]);
    let round = round_type(&mut h);
    let generic = type_of(&h, ids[0]);
    h.run_steps(2);

    h.get_by_role_and_label(Role::Button, "Edit Type").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Rename...").click();
    h.run_steps(2);
    set_field(&mut h, "Name", "Round 400");
    assert!(shows(
        &h,
        "Another type is already called \u{2068}Round 400\u{2069}"
    ));
    set_field(&mut h, "Name", "Square 300");
    h.get_by_role_and_label(Role::Button, "OK").click();
    h.run_steps(2);
    let view = h.state().project().view();
    assert_eq!(hito::parameters::type_name(&view, generic), "Square 300");
    assert_eq!(hito::parameters::type_name(&view, round), "Round 400");
}

#[test]
fn a_types_structural_material_is_picked_in_the_type_editor() {
    use hito_structural::material;
    let mut h = window();
    let ids = columns(&mut h, &["0,0"]);
    let column_type = type_of(&h, ids[0]);
    let h30 = {
        let project = h.state_mut().project_mut();
        let definitions = project
            .models()
            .find(|m| m.kind() == hito_core::ModelKind::Definition)
            .unwrap()
            .id();
        let mut tx = project.transaction();
        let concrete = hito_core::Element::new(class(material::CONCRETE), definitions)
            .with_property(material::NAME, "H-30")
            .with_property(material::COMPRESSIVE_STRENGTH, 30e6);
        let id = tx.insert(concrete).unwrap();
        tx.commit().unwrap();
        id
    };
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Edit Type").click();
    h.run_steps(2);
    let list = h.get_by_role_and_label(Role::ComboBox, "Structural Material");
    assert_eq!(list.value().as_deref(), Some("Concrete"));
    pick(&mut h, "Structural Material", "H-30");
    h.get_by_role_and_label(Role::Button, "OK").click();
    h.run_steps(2);
    let view = h.state().project().view();
    assert_eq!(section::material_of(&view, column_type), Some(h30));
}
