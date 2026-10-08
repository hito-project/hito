//! Headless checks of the riskiest needs. They drive the app through the
//! AccessKit tree (the same tree a screen reader sees), with a real wgpu device.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use ui_spike::{HitoApp, commands::CommandId};

fn harness() -> Harness<'static, HitoApp> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .wgpu()
        .build_eframe(|cc| HitoApp::new(cc));
    h.run_steps(4);
    h
}

fn results_dir() -> std::path::PathBuf {
    let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("results");
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn ribbon_button_is_accessible_and_runs_its_command() {
    let mut h = harness();
    h.get_by_role_and_label(Role::Button, "Pilar").click();
    h.run_steps(2);
    assert_eq!(h.state().log, vec![CommandId::Column]);
}

#[test]
fn command_line_autocompletes_and_runs() {
    let mut h = harness();
    h.get_by_role(Role::TextInput).focus();
    h.run_steps(1);
    h.get_by_role(Role::TextInput).type_text("vi");
    h.run_steps(2);
    // The popup lists the match with its global name and shortcut.
    assert!(h.query_by_label("Viga   BEAM   BM").is_some(), "completion popup shows 'Viga'");
    h.key_press(egui::Key::Tab);
    h.run_steps(2);
    assert_eq!(h.state().log, vec![CommandId::Beam]);

    // Global (language-neutral) names and shortcuts work in the command line too.
    h.get_by_role(Role::TextInput).focus();
    h.run_steps(1);
    h.get_by_role(Role::TextInput).type_text("column");
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().log, vec![CommandId::Beam, CommandId::Column]);
}

#[test]
fn accent_insensitive_completion() {
    let mut h = harness();
    // Switch to the View tab of the ribbon, where the language group is.
    h.get_by_label("Vista").click();
    h.run_steps(2);
    h.get_by_role(Role::TextInput).focus();
    h.run_steps(1);
    h.get_by_role(Role::TextInput).type_text("mover");
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().log, vec![CommandId::Move]);
}

#[test]
fn two_letter_shortcuts_when_nothing_has_focus() {
    let mut h = harness();
    h.event(egui::Event::Text("C".into()));
    h.event(egui::Event::Text("L".into()));
    h.run_steps(1);
    h.event(egui::Event::Text("b".into()));
    h.event(egui::Event::Text("m".into()));
    h.run_steps(1);
    assert_eq!(h.state().log, vec![CommandId::Column, CommandId::Beam]);
}

#[test]
fn property_grid_values_are_named_for_screen_readers() {
    let h = harness();
    let height = h.get_by_role_and_label(Role::SpinButton, "Altura");
    println!("Altura node value: {:?}", height.value());
    assert!(height.value().unwrap_or_default().contains('3'));
}

#[test]
fn language_switch_relabels_everything() {
    let mut h = harness();
    h.get_by_label("Vista").click();
    h.run_steps(2);
    h.get_by_label("English").click();
    h.run_steps(3);
    assert!(h.query_by_role_and_label(Role::SpinButton, "Height").is_some());
    assert!(h.query_by_role_and_label(Role::SpinButton, "Altura").is_none());
    h.get_by_label("Structure").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Column").click();
    h.run_steps(2);
    assert_eq!(h.state().log, vec![CommandId::Column]);
}

#[test]
fn viewport_renders_with_wgpu_inside_a_dock_tab() {
    let mut h = harness();
    assert!(h.state().viewport.frames_rendered > 0, "viewport rendered at least once");
    let img = h.render().expect("render");
    img.save(results_dir().join("screenshot.png")).unwrap();
    // The 3D tab sits in the middle of the window; some pixel there must not be the clear colour.
    let (w, ht) = img.dimensions();
    let bg = [237u8, 240, 245];
    let mut drawn = 0;
    for y in (ht / 4..ht * 3 / 4).step_by(4) {
        for x in (w / 4..w * 3 / 4).step_by(4) {
            let p = img.get_pixel(x, y).0;
            if (0..3).any(|i| (p[i] as i32 - bg[i] as i32).abs() > 30) {
                drawn += 1;
            }
        }
    }
    assert!(drawn > 100, "the scene is visible in the viewport ({drawn} samples)");

    // Orbit: dragging changes the camera and triggers a new render.
    let before = (h.state().viewport.yaw, h.state().viewport.frames_rendered);
    let c = egui::pos2(640.0, 400.0);
    h.drag_at(c);
    h.run_steps(1);
    h.hover_at(c + egui::vec2(60.0, 0.0));
    h.run_steps(1);
    h.drop_at(c + egui::vec2(60.0, 0.0));
    h.run_steps(2);
    assert_ne!(h.state().viewport.yaw, before.0);
    assert!(h.state().viewport.frames_rendered > before.1);
}

#[test]
fn text_shaping_report() {
    let mut h = harness();
    let ctx = h.ctx.clone();
    h.run_steps(1);
    let report = ui_spike::text::measure(&ctx);
    let mut out = String::from("sample\tchars\tglyphs\ttext\n");
    for m in &report {
        out += &format!("{}\t{}\t{}\t{}\n", m.name, m.chars, m.glyphs, m.text);
    }
    println!("{out}");
    std::fs::write(results_dir().join("text-shaping.tsv"), &out).unwrap();
    let es = &report[0];
    assert_eq!(es.chars, es.glyphs, "Spanish: one glyph per character");
}

/// Renders the text samples to `results/text-shaping.png` so the RTL output can be inspected by eye.
#[test]
fn text_shaping_screenshot() {
    let fonts: std::rc::Rc<std::cell::RefCell<Vec<&'static str>>> = Default::default();
    let f = fonts.clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 240.0))
        .wgpu()
        .build_ui(move |ui| ui_spike::text::ui(ui, &f.borrow()));
    *fonts.borrow_mut() = ui_spike::text::install_fallback_fonts(&h.ctx);
    h.run_steps(4);
    h.render().expect("render").save(results_dir().join("text-shaping.png")).unwrap();
}

/// Finding, not a goal: egui_dock 0.21 exposes its tabs to AccessKit with
/// role `Unknown` and no name, so a screen reader can't tell "Properties"
/// from "3D view". This test fails once that's fixed upstream (or by us).
#[test]
fn finding_dock_tabs_have_no_accessible_name() {
    let h = harness();
    for title in ["Propiedades", "Vista 3D", "Navegador de proyectos"] {
        assert!(h.query_by_label(title).is_none(), "dock tab {title:?} is now named");
    }
    let unnamed = h.query_all_by_role(Role::Unknown).count();
    println!("unnamed nodes with role Unknown: {unnamed}");
    assert!(unnamed >= 5);
}
