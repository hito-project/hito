//! Drives the main window headless through its AccessKit tree, the same tree
//! a screen reader sees (ADR 0015).
//!
//! Only `the_3d_view_renders_with_wgpu` needs a graphics device. It skips
//! itself when there's none, unless `HITO_REQUIRE_GPU` is set, as it is on
//! every CI runner (ci.yml).

use std::collections::HashMap;

use eframe::egui;
use eframe::egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, HarnessBuilder};
use hito::Shell;
use hito::command::{Command, Input, Next, Prompt, Run, Tool, Wants};
use hito::i18n::Locale;
use hito::selection::Mode;
use hito::shell::{Tab, Theme, ViewId};
use hito::viewport::Navigation;
use hito_core::{Dimension, Model, ModelKind, StoreError, Transaction};

/// kittest runs one step per input event. Short steps keep the two clicks
/// of a double-click within egui's double-click delay.
fn builder() -> HarnessBuilder<Shell> {
    Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_step_dt(0.05)
}

/// The window in English, with no saved layout.
fn window() -> Harness<'static, Shell> {
    window_from(None)
}

fn window_from(storage: Option<&Storage>) -> Harness<'static, Shell> {
    let storage = storage.cloned();
    let mut h = builder()
        .build_eframe(move |_cc| Shell::restore(storage.as_ref().map(|s| s as _), Locale::En));
    h.run_steps(2);
    h
}

/// eframe's storage, in memory.
#[derive(Default, Clone)]
struct Storage(HashMap<String, String>);

impl eframe::Storage for Storage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }

    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }

    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }

    fn flush(&mut self) {}
}

fn click(h: &mut Harness<'_, Shell>, label: &str) {
    h.get_by_role_and_label(Role::Button, label).click();
    h.run_steps(2);
}

/// Whether some widget has this text. A ribbon tab and a group can share a
/// name, as Revit's Structure tab and group do.
fn shows(h: &Harness<'_, Shell>, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

fn ribbon_tab(h: &mut Harness<'_, Shell>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(2);
}

#[test]
fn ribbon_has_discipline_tabs_with_groups() {
    let mut h = window();
    // The Structure tab shows first, with the M1 commands. Those built so
    // far, such as Column, are enabled.
    let column = h.get_by_role_and_label(Role::Button, "Column");
    assert!(!column.accesskit_node().is_disabled());
    assert!(shows(&h, "Datum"));

    ribbon_tab(&mut h, "View");
    assert!(h.query_by_role_and_label(Role::Button, "Column").is_none());
    assert!(shows(&h, "Windows"));
    assert!(
        !h.get_by_role_and_label(Role::Button, "Tile Views")
            .accesskit_node()
            .is_disabled()
    );

    ribbon_tab(&mut h, "Manage");
    assert!(shows(&h, "Settings"));
}

#[test]
fn opens_with_revit_layout() {
    let h = window();
    let layout = h.state().layout();
    assert!(layout.is_open(Tab::Properties));
    assert!(layout.is_open(Tab::ProjectBrowser));
    assert_eq!(layout.open_views(), [ViewId::DEFAULT_3D]);
    // The project browser lists the views, levels and element types.
    for label in ["Views (all)", "3D Views", "{3D}", "Levels", "Element types"] {
        assert!(shows(&h, label), "{label:?} in the browser");
    }
    // The properties show the active view's.
    assert!(shows(&h, "View Name"));
}

#[test]
fn several_views_open_as_tabs_or_tiles() {
    let mut h = window();
    ribbon_tab(&mut h, "View");
    click(&mut h, "3D View");
    assert_eq!(
        h.state().layout().active_view(),
        Some(ViewId(1)),
        "a new view is active"
    );
    click(&mut h, "3D View");
    assert_eq!(h.state().layout().open_views().len(), 3);
    assert_eq!(h.state().layout().view_areas(), 1);

    click(&mut h, "Tile Views");
    assert_eq!(h.state().layout().view_areas(), 3);
    let active = h.state().layout().active_view();
    assert_eq!(active, Some(ViewId(2)), "tiling keeps the active view");
    click(&mut h, "Tab Views");
    assert_eq!(h.state().layout().view_areas(), 1);

    click(&mut h, "Close Inactive");
    assert_eq!(h.state().layout().open_views().len(), 1);
    // The closed views stay in the project browser, and open again from it.
    let name = h.state().i18n().tr_with("view-3d-numbered", "number", 1);
    // Let the earlier clicks age, or egui counts a triple click.
    h.run_steps(60);
    let entry = h.get_by_label(&name);
    entry.click();
    h.run_steps(1);
    h.get_by_label(&name).click();
    h.run_steps(2);
    assert_eq!(h.state().layout().open_views().len(), 2);
}

#[test]
fn panels_close_and_reopen_from_the_user_interface_menu() {
    let mut h = window();
    ribbon_tab(&mut h, "View");
    click(&mut h, "User Interface");
    h.get_by_role_and_label(Role::CheckBox, "Properties")
        .click();
    h.run_steps(2);
    assert!(!h.state().layout().is_open(Tab::Properties));
    assert!(!shows(&h, "View Name"));

    click(&mut h, "User Interface");
    h.get_by_role_and_label(Role::CheckBox, "Properties")
        .click();
    h.run_steps(2);
    assert!(h.state().layout().is_open(Tab::Properties));
}

#[test]
fn the_layout_is_remembered_between_sessions() {
    let mut h = window();
    ribbon_tab(&mut h, "View");
    click(&mut h, "3D View");
    click(&mut h, "Tile Views");
    click(&mut h, "User Interface");
    h.get_by_role_and_label(Role::CheckBox, "Project Browser")
        .click();
    h.run_steps(2);

    let mut storage = Storage::default();
    eframe::App::save(h.state_mut(), &mut storage);
    drop(h);

    let h = window_from(Some(&storage));
    let layout = h.state().layout();
    assert_eq!(layout.open_views().len(), 2);
    assert_eq!(layout.view_areas(), 2);
    assert!(!layout.is_open(Tab::ProjectBrowser));
    assert!(layout.is_open(Tab::Properties));
}

#[test]
fn the_theme_is_light_by_default_switches_and_is_remembered() {
    let mut h = window();
    assert_eq!(h.state().theme(), Theme::Light);
    assert_eq!(h.ctx.theme(), egui::Theme::Light);

    ribbon_tab(&mut h, "View");
    click(&mut h, "User Interface");
    h.get_by_role_and_label(Role::RadioButton, "Dark").click();
    h.run_steps(2);
    assert_eq!(h.state().theme(), Theme::Dark);
    assert_eq!(h.ctx.theme(), egui::Theme::Dark);

    let mut storage = Storage::default();
    eframe::App::save(h.state_mut(), &mut storage);
    drop(h);
    let h = window_from(Some(&storage));
    assert_eq!(h.state().theme(), Theme::Dark);
    assert_eq!(h.ctx.theme(), egui::Theme::Dark);
}

/// The wheel orbits by default, as the founding user asked, and Revit's
/// bindings are one click away (ADR 0025).
#[test]
fn the_wheel_orbits_by_default_and_revits_navigation_is_one_click_away() {
    let mut h = window();
    assert_eq!(h.state().navigation(), Navigation::WheelOrbits);

    ribbon_tab(&mut h, "View");
    click(&mut h, "User Interface");
    let revit = "Revit: wheel pans, Shift + wheel orbits";
    h.get_by_role_and_label(Role::RadioButton, revit).click();
    h.run_steps(2);
    assert_eq!(h.state().navigation(), Navigation::Revit);

    let mut storage = Storage::default();
    eframe::App::save(h.state_mut(), &mut storage);
    drop(h);
    let h = window_from(Some(&storage));
    assert_eq!(h.state().navigation(), Navigation::Revit);
}

#[test]
fn the_wheel_navigates_the_3d_view_and_zf_zooms_to_fit() {
    let mut h = window();
    let target = |h: &Harness<'_, Shell>| {
        let viewport = h.state().viewport(ViewId::DEFAULT_3D).expect("shown");
        viewport.camera().target()
    };
    let start = target(&h);

    // Shift and the wheel pan the view.
    let middle = egui::pos2(1280.0 * 3.0 / 5.0, 400.0);
    let wheel = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Middle,
        pressed,
        modifiers: egui::Modifiers::SHIFT,
    };
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
    h.hover_at(middle);
    h.run_steps(1);
    h.event(wheel(middle, true));
    h.run_steps(1);
    for step in 1..=4 {
        h.hover_at(middle + egui::vec2(30.0, 20.0) * step as f32);
        h.run_steps(1);
    }
    h.event(wheel(middle + egui::vec2(120.0, 80.0), false));
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.remove_cursor();
    h.run_steps(2);
    assert_ne!(target(&h), start, "panned");

    // Revit's shortcut, typed with nothing focused.
    h.event(egui::Event::Text("Z".into()));
    h.event(egui::Event::Text("F".into()));
    h.run_steps(2);
    assert_eq!(h.state().commands().message(), None, "ZF ran");
    let fitted = target(&h);
    assert!(fitted.x.abs() < 1e-9 && fitted.y.abs() < 1e-9, "{fitted}");
}

#[test]
fn an_unreadable_saved_layout_falls_back_to_the_default() {
    let mut storage = Storage::default();
    eframe::Storage::set_string(&mut storage, "layout", "not a layout".into());
    let h = window_from(Some(&storage));
    assert_eq!(h.state().layout().open_views(), [ViewId::DEFAULT_3D]);
    assert!(h.state().layout().is_open(Tab::ProjectBrowser));
}

#[test]
fn text_is_in_spanish_by_default() {
    let mut h = builder().build_eframe(|cc| Shell::new(cc));
    h.run_steps(2);
    for label in [
        "Estructura",
        "Vista",
        "Gestionar",
        "Columna",
        "Fundaciones",
        "Base aislada",
        "Niveles",
    ] {
        assert!(shows(&h, label), "{label:?}");
    }
}

/// With a wgpu device, a view tab shows what the viewport renders off-screen.
#[test]
fn the_3d_view_renders_with_wgpu() {
    use egui_kittest::wgpu::{WgpuTestRenderer, create_render_state, default_wgpu_setup};

    // kittest panics when it finds no adapter.
    let options = eframe::egui_wgpu::RendererOptions::PREDICTABLE;
    let Ok(render_state) =
        std::panic::catch_unwind(|| create_render_state(default_wgpu_setup(), options))
    else {
        // CI sets this on every runner, all of which have an adapter, so a
        // missing adapter there is a failure, not a skip.
        let required = std::env::var("HITO_REQUIRE_GPU").is_ok_and(|v| !v.is_empty());
        assert!(
            !required,
            "HITO_REQUIRE_GPU is set but no wgpu adapter was found"
        );
        println!("skipped: no wgpu adapter");
        return;
    };
    let info = render_state.adapter.get_info();
    println!("rendering with {} ({:?})", info.name, info.backend);
    let renderer = WgpuTestRenderer::from_render_state(render_state);
    let mut h = builder()
        .renderer(renderer)
        .build_eframe(|_cc| Shell::restore(None, Locale::En));
    h.run_steps(3);
    let renders = h
        .state()
        .viewport(ViewId::DEFAULT_3D)
        .expect("shown")
        .renders;
    assert!(renders > 0, "the viewport rendered");
    h.run_steps(3);
    let again = h.state().viewport(ViewId::DEFAULT_3D).unwrap().renders;
    assert_eq!(
        again, renders,
        "it renders again only when something changes"
    );

    // The middle of the view area has the viewport's background.
    let image = h.render().expect("render");
    let (w, ht) = image.dimensions();
    let pixel = image.get_pixel(w * 3 / 5, ht / 2).0;
    let background = hito::viewport::BACKGROUND;
    let close = (0..3).all(|i| pixel[i].abs_diff(background[i]) <= 8);
    assert!(close, "pixel {pixel:?}, background {background:?}");
}

/// Types `text` in the command line and presses Enter.
fn command_line(h: &mut Harness<'_, Shell>, text: &str) {
    h.get_by_role(Role::TextInput).focus();
    h.run_steps(1);
    h.get_by_role(Role::TextInput).type_text(text);
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
}

#[test]
fn command_line_autocompletes_and_runs() {
    let mut h = window();
    // A language-neutral name.
    command_line(&mut h, "VIEW3D");
    assert_eq!(h.state().layout().open_views().len(), 2);

    // A partial label lists the matches with their names and aliases; Tab
    // runs the highlighted one.
    h.get_by_role(Role::TextInput).focus();
    h.run_steps(1);
    h.get_by_role(Role::TextInput).type_text("tile");
    h.run_steps(2);
    assert!(shows(&h, "Tile Views   TILEVIEWS   WT"), "completion");
    h.key_press(egui::Key::Tab);
    h.run_steps(2);
    assert_eq!(h.state().layout().view_areas(), 2);

    // An alias, and a partial name that runs its first completion.
    command_line(&mut h, "tab");
    assert_eq!(h.state().layout().view_areas(), 1);
    command_line(&mut h, "wt");
    assert_eq!(h.state().layout().view_areas(), 2);

    command_line(&mut h, "xyzzy");
    assert!(h.state().commands().message().unwrap().contains("xyzzy"));
}

#[test]
fn accent_insensitive_completion() {
    let mut h = builder().build_eframe(|cc| Shell::new(cc));
    h.run_steps(2);
    command_line(&mut h, "vista 3d");
    // Spanish labels, whatever the case and the accents.
    h.get_by_role(Role::TextInput).focus();
    h.run_steps(1);
    h.get_by_role(Role::TextInput).type_text("MOSÁI");
    h.run_steps(2);
    let completion = "Vistas en mosaico   TILEVIEWS   WT";
    assert!(shows(&h, completion), "completion");
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().layout().view_areas(), 2);
}

#[test]
fn two_letter_shortcuts_when_nothing_has_focus() {
    let mut h = window();
    h.event(egui::Event::Text("P".into()));
    h.event(egui::Event::Text("P".into()));
    h.run_steps(2);
    assert!(!h.state().layout().is_open(Tab::Properties), "PP");

    // A command that isn't available yet says so.
    h.event(egui::Event::Text("u".into()));
    h.event(egui::Event::Text("n".into()));
    h.run_steps(2);
    let message = h.state().commands().message().unwrap_or_default();
    assert!(message.contains("Project Units"), "{message:?}");

    // The ribbon's tooltip names the alias.
    ribbon_tab(&mut h, "View");
    h.get_by_role_and_label(Role::Button, "Tile Views").hover();
    h.run_steps(30);
    // Fluent marks its arguments with invisible isolation characters.
    assert!(h.query_by_label_contains("WT").is_some(), "tooltip");
}

/// A tool for the tests: asks for a point and a length, then adds a model.
#[derive(Default)]
struct Probe {
    point: Option<[f64; 3]>,
}

impl Tool for Probe {
    fn prompt(&self) -> Prompt {
        let (message, wants) = match self.point {
            None => ("probe-point", Wants::Point),
            Some(_) => ("probe-length", Wants::Quantity(Dimension::LENGTH)),
        };
        Prompt { message, wants }
    }

    fn input(&mut self, input: Input) -> Next {
        match input {
            Input::Point(point) => {
                self.point = Some(point);
                Next::Ask
            }
            Input::Quantity(_) => Next::Done,
        }
    }

    fn commit(self: Box<Self>, tx: &mut Transaction<'_>) -> Result<(), StoreError> {
        tx.insert_model(Model::new(ModelKind::Physical)).map(drop)
    }
}

/// The window with the probe tool registered as `PROBE`, alias `PR`.
fn window_with_probe() -> Harness<'static, Shell> {
    window_with_probe_in(Locale::En)
}

fn window_with_probe_in(locale: Locale) -> Harness<'static, Shell> {
    let mut h = builder().build_eframe(move |_cc| Shell::restore(None, locale));
    h.run_steps(2);
    let probe = Command {
        name: "PROBE",
        label: "probe",
        alias: Some("PR"),
        run: Run::Tool(|| Box::new(Probe::default())),
    };
    let registry = h.state_mut().commands_mut().registry_mut();
    registry.register(probe).unwrap();
    h
}

/// The message key of the running command's prompt.
fn prompt(h: &Harness<'_, Shell>) -> Option<&'static str> {
    h.state().commands().prompt().map(|p| p.message)
}

#[test]
fn commands_prompt_on_the_command_line() {
    let mut h = window_with_probe();
    // Started by its shortcut, the tool takes the command line's focus.
    h.event(egui::Event::Text("PR".into()));
    h.run_steps(2);
    assert_eq!(prompt(&h), Some("probe-point"));
    assert!(shows(&h, "⟨probe-point⟩"), "the prompt is the label");

    h.get_by_role(Role::TextInput).type_text("1,2");
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert_eq!(prompt(&h), Some("probe-length"));

    // The answer must be a length.
    h.get_by_role(Role::TextInput).type_text("2 kN");
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert_eq!(prompt(&h), Some("probe-length"));
    assert!(h.state().commands().message().is_some());

    h.get_by_role(Role::TextInput).type_text("3 m");
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert_eq!(prompt(&h), None);
    assert_eq!(
        h.state().project().models().count(),
        3,
        "the probe added a model to the new project's physical and definition models"
    );
}

/// The command line reads numbers as AutoCAD does in every language, even
/// where the UI writes decimals with a comma.
#[test]
fn input_uses_a_decimal_point_in_spanish_too() {
    let mut h = window_with_probe_in(Locale::EsAr);
    command_line(&mut h, "probe");
    command_line(&mut h, "3.5,2");
    assert_eq!(prompt(&h), Some("probe-length"));
    command_line(&mut h, "3,5");
    assert_eq!(prompt(&h), Some("probe-length"), "a comma isn't a decimal");
    command_line(&mut h, "1.5 m");
    assert_eq!(prompt(&h), None);
    assert_eq!(
        h.state().project().models().count(),
        3,
        "the probe added a model to the new project's physical and definition models"
    );
}

#[test]
fn esc_cancels_a_command() {
    let mut h = window_with_probe();
    command_line(&mut h, "probe");
    command_line(&mut h, "1,2");
    assert_eq!(prompt(&h), Some("probe-length"));
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert_eq!(prompt(&h), None);
    assert_eq!(h.state().commands().message(), Some("*Cancel*"));
    assert_eq!(
        h.state().project().models().count(),
        2,
        "only the new project's physical and definition models"
    );

    // A ribbon button cancels the running command and runs its own.
    command_line(&mut h, "probe");
    ribbon_tab(&mut h, "View");
    click(&mut h, "3D View");
    assert_eq!(prompt(&h), None);
    assert_eq!(h.state().layout().open_views().len(), 2);
}

/// The browser picks into the selection the views share (#30). The
/// properties panel edits one level, shows several read-only, as Revit
/// does, and Esc clears the selection.
#[test]
fn the_properties_show_the_selection_and_esc_clears_it() {
    let mut h = window();
    add_level(&mut h, "3");
    h.get_by_label("Level 1").click();
    h.run_steps(2);
    assert_eq!(h.state().selection().len(), 1, "the browser selects");
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Elevation")
            .is_some()
    );

    // Ctrl adds, as in the views.
    let second = level_id(&h, "Level 2");
    h.state_mut().select([second], Mode::Add);
    h.run_steps(2);
    assert_eq!(h.state().selection().len(), 2);
    assert!(shows(&h, "Level (2)"), "several, read-only");
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Elevation")
            .is_none()
    );

    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(h.state().selection().is_empty());
    assert!(shows(&h, "View Name"), "back to the view's properties");
}

/// The ID of the level called `name`.
fn level_id(h: &Harness<'_, Shell>, name: &str) -> hito_core::ElementId {
    let name = hito_core::Value::Text(name.into());
    let project = h.state().project();
    let level = project
        .elements()
        .find(|e| e.properties().get(hito_structural::level::NAME) == Some(&name));
    level.expect("the level").id()
}

/// Delete removes every selected element in one go.
#[test]
fn delete_removes_the_selection() {
    let mut h = window();
    add_level(&mut h, "3");
    let before = h.state().project().elements().count();
    let (first, second) = (level_id(&h, "Level 1"), level_id(&h, "Level 2"));
    h.state_mut().select([first, second], Mode::Replace);
    h.run_steps(1);
    h.key_press(egui::Key::Delete);
    h.run_steps(2);
    let project = h.state().project();
    assert!(project.get(first).is_none() && project.get(second).is_none());
    assert_eq!(project.elements().count(), before - 2);
    assert!(h.state().selection().is_empty());
}

/// The elevation of the level called `name`, if there is one.
fn level(h: &Harness<'_, Shell>, name: &str) -> Option<f64> {
    use hito_core::Value;
    use hito_structural::level::{ELEVATION, NAME};
    let project = h.state().project();
    let name = Value::Text(name.into());
    let level = project
        .elements()
        .find(|e| e.properties().get(NAME) == Some(&name))?;
    match project.property(level.id(), ELEVATION) {
        Some(Value::Real(elevation)) => Some(*elevation),
        other => panic!("elevation {other:?}"),
    }
}

/// Replaces the text of the property field labelled `label` and presses
/// Enter.
fn set_property(h: &mut Harness<'_, Shell>, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label).focus();
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.get_by_role_and_label(Role::TextInput, label)
        .type_text(text);
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
}

/// Adds a level at `elevation` with the Level command, then ends it.
fn add_level(h: &mut Harness<'_, Shell>, elevation: &str) {
    command_line(h, "LL");
    command_line(h, elevation);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
}

#[test]
fn a_new_project_starts_with_one_level() {
    let h = window();
    assert_eq!(level(&h, "Level 1"), Some(0.0));
    assert_eq!(level(&h, "Level 2"), None);
    assert!(shows(&h, "Level 1"), "in the browser");
}

#[test]
fn levels_are_renamed_and_moved_in_the_properties() {
    let mut h = window();
    add_level(&mut h, "3");
    h.get_by_label("Level 2").click();
    h.run_steps(2);
    let elevation = h.get_by_role_and_label(Role::TextInput, "Elevation");
    assert_eq!(elevation.value().as_deref(), Some("3 m"));

    set_property(&mut h, "Elevation", "320 cm");
    assert_eq!(level(&h, "Level 2"), Some(3.2));
    set_property(&mut h, "Elevation", "three");
    assert_eq!(level(&h, "Level 2"), Some(3.2), "not a length");

    // Names are unique, as in Revit.
    set_property(&mut h, "Name", "Level 1");
    assert!(shows(
        &h,
        "Another level is already called \u{2068}Level 1\u{2069}"
    ));
    assert_eq!(level(&h, "Level 2"), Some(3.2));
    set_property(&mut h, "Name", "Primer piso");
    assert_eq!(level(&h, "Primer piso"), Some(3.2));
    assert!(shows(&h, "Primer piso"), "the browser shows the new name");
}

#[test]
fn levels_are_added_with_their_command_until_esc() {
    let mut h = window();
    h.event(egui::Event::Text("LL".into()));
    h.run_steps(2);
    assert_eq!(prompt(&h), Some("level-prompt-elevation"));
    h.get_by_role(Role::TextInput).type_text("3");
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    h.get_by_role(Role::TextInput).type_text("6 m");
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert_eq!(prompt(&h), Some("level-prompt-elevation"), "still active");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert_eq!(prompt(&h), None);
    // The first new level after the default one is Level 2.
    assert_eq!(level(&h, "Level 2"), Some(3.0));
    assert_eq!(level(&h, "Level 3"), Some(6.0));
    assert!(shows(&h, "Level 3"), "listed in the browser");
}

#[test]
fn levels_are_deleted_with_the_delete_key_or_their_menu() {
    let mut h = window();
    add_level(&mut h, "3");
    h.get_by_label("Level 2").click();
    h.run_steps(2);
    h.key_press(egui::Key::Delete);
    h.run_steps(2);
    assert_eq!(level(&h, "Level 2"), None);
    assert!(!shows(&h, "Level 2"));

    h.get_by_label("Level 1").click_secondary();
    h.run_steps(2);
    click(&mut h, "Delete");
    assert_eq!(level(&h, "Level 1"), None);
    let empty = h.query_all_by_label("(none)").count();
    assert_eq!(empty, 2, "no levels left, and no grids");
}

/// The line of the grid labelled `label`, if there is one.
fn grid(h: &Harness<'_, Shell>, label: &str) -> Option<hito_structural::grid::GridLine> {
    let project = h.state().project();
    let id = hito_structural::grid::find(&project.view(), label)?;
    hito_structural::grid::GridLine::of(&project.view(), id)
}

/// Types `text` in the focused command line and presses Enter.
fn answer(h: &mut Harness<'_, Shell>, text: &str) {
    h.get_by_role(Role::TextInput).type_text(text);
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
}

#[test]
fn grids_are_drawn_with_their_command_and_labelled_automatically() {
    use hito_structural::grid::GridLine;
    let mut h = window();
    assert!(shows(&h, "Grids"), "the browser lists grids");
    h.event(egui::Event::Text("GR".into()));
    h.run_steps(2);
    assert_eq!(prompt(&h), Some("grid-prompt-start"));
    answer(&mut h, "0,0");
    assert_eq!(prompt(&h), Some("grid-prompt-end"));
    answer(&mut h, "0,10");
    answer(&mut h, "6,0");
    answer(&mut h, "6,10");
    assert_eq!(prompt(&h), Some("grid-prompt-start"), "still active");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);

    assert_eq!(grid(&h, "1"), Some(GridLine::new([0.0, 0.0], [0.0, 10.0])));
    assert_eq!(grid(&h, "2"), Some(GridLine::new([6.0, 0.0], [6.0, 10.0])));
}

#[test]
fn grid_labels_are_edited_in_the_properties_and_grids_deleted() {
    let mut h = window();
    command_line(&mut h, "GR");
    for point in ["0,0", "0,10", "6,0", "6,10"] {
        answer(&mut h, point);
    }
    h.key_press(egui::Key::Escape);
    h.run_steps(2);

    h.get_by_label("1").click();
    h.run_steps(2);
    assert!(shows(&h, "Grid"), "the properties show a grid");
    set_property(&mut h, "Label", "A");
    assert!(grid(&h, "A").is_some() && grid(&h, "1").is_none());
    // Labels are unique, as in Revit.
    h.get_by_label("2").click();
    h.run_steps(2);
    set_property(&mut h, "Label", "A");
    assert!(shows(
        &h,
        "Another grid is already labelled \u{2068}A\u{2069}"
    ));
    assert!(grid(&h, "2").is_some());

    h.key_press(egui::Key::Delete);
    h.run_steps(2);
    assert!(grid(&h, "2").is_none());
    assert!(grid(&h, "A").is_some());
}

/// The walls in the project, oldest first.
fn walls(h: &Harness<'_, Shell>) -> Vec<hito_core::ElementId> {
    let project = h.state().project();
    let class = hito_architectural::class(hito_architectural::wall::WALL);
    let mut walls: Vec<_> = project
        .elements()
        .filter(|e| project.registry().is_a(e.class(), &class))
        .map(|e| e.id())
        .collect();
    walls.sort();
    walls
}

#[test]
fn walls_are_drawn_as_a_chain_with_typed_lengths() {
    let mut h = window();
    command_line(&mut h, "WA");
    assert_eq!(prompt(&h), Some("wall-prompt-start"));
    command_line(&mut h, "0,0");
    command_line(&mut h, "@5<0");
    command_line(&mut h, "@4<90");
    assert_eq!(prompt(&h), Some("wall-prompt-next"), "still drawing");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert_eq!(prompt(&h), None);

    let walls = walls(&h);
    assert_eq!(walls.len(), 2);
    let view = h.state().project().view();
    let (_, end) = hito_architectural::wall::line(&view, walls[1]).unwrap();
    assert!((end[0] - 5.0).abs() < 1e-9 && (end[1] - 4.0).abs() < 1e-9);
    let corner = hito_architectural::footprint::End::End;
    let joined = hito_architectural::join::joined_at(&view, walls[0], corner);
    assert_eq!(joined, [walls[1]], "the corner joined");
}

#[test]
fn drawn_walls_show_in_the_3d_view() {
    let mut h = window();
    command_line(&mut h, "WA");
    command_line(&mut h, "0,0");
    command_line(&mut h, "@5<0");
    command_line(&mut h, "@4<90");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);

    let walls = walls(&h);
    assert_eq!(walls.len(), 2);
    for wall in walls {
        assert!(
            h.state().scene().mesh(wall).is_some(),
            "a mesh for {wall:?}"
        );
    }
}
