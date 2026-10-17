//! Drives the main window headless through its AccessKit tree, the same tree
//! a screen reader sees (ADR 0015).
//!
//! Only `the_3d_view_renders_with_wgpu` needs a graphics device. It skips
//! itself when there's none, such as on CI runners without Mesa's lavapipe.

use std::collections::HashMap;

use eframe::egui;
use eframe::egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, HarnessBuilder};
use hito::Shell;
use hito::i18n::Locale;
use hito::shell::{Tab, ViewId};

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
    // The Structure tab shows first, with the M1 commands, disabled until #24.
    let column = h.get_by_role_and_label(Role::Button, "Column");
    assert!(column.accesskit_node().is_disabled());
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
    // kittest panics when it finds no adapter.
    let Ok(renderer) = std::panic::catch_unwind(egui_kittest::wgpu::WgpuTestRenderer::new) else {
        eprintln!("skipped: no wgpu adapter");
        return;
    };
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
