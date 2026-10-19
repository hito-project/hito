//! Plan views (#183), driven headless through the window's AccessKit tree:
//! each level's plan opens from the project browser, and clicks in it give
//! the placement tools their points.

use eframe::egui;
use eframe::egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use hito::Shell;
use hito::i18n::Locale;
use hito::shell::ViewId;
use hito_core::{ElementId, Value};
use hito_structural::{grid, level};

/// The window in English, with no saved layout.
fn window() -> Harness<'static, Shell> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_step_dt(0.05)
        .build_eframe(|_cc| Shell::restore(None, Locale::En));
    h.run_steps(2);
    h
}

fn command_line(h: &mut Harness<'_, Shell>, text: &str) {
    h.get_by_role(Role::TextInput).focus();
    h.run_steps(1);
    h.get_by_role(Role::TextInput).type_text(text);
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
}

fn esc(h: &mut Harness<'_, Shell>) {
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
}

fn level_id(h: &Harness<'_, Shell>, name: &str) -> ElementId {
    let name = Value::Text(name.into());
    let project = h.state().project();
    project
        .elements()
        .find(|e| e.properties().get(level::NAME) == Some(&name))
        .map(|e| e.id())
        .expect("the level exists")
}

/// Opens a level's plan from the project browser's Floor Plans, as in
/// Revit: a double click on its name. Floor Plans start closed.
fn open_plan(h: &mut Harness<'_, Shell>, name: &str) -> ViewId {
    h.get_by_label("Floor Plans").click();
    // Let the click age, or egui counts the next two as a triple click.
    h.run_steps(60);
    // Floor Plans come before Levels in the browser.
    for _ in 0..2 {
        h.query_all_by_label(name).next().expect("listed").click();
        h.run_steps(1);
    }
    h.run_steps(2);
    let level = level_id(h, name);
    h.state().layout().plan_of(level).expect("a plan view")
}

/// Clicks in a view where a point of the model shows.
fn click_at(h: &mut Harness<'_, Shell>, view: ViewId, point: [f64; 3], nudge: egui::Vec2) {
    let viewport = h.state().viewport(view).expect("shown");
    let pos = viewport.to_screen(point).expect("on screen") + nudge;
    h.hover_at(pos);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    // Let the click age, so the next one isn't a double click.
    h.run_steps(12);
}

fn grids(h: &Harness<'_, Shell>) -> Vec<(String, grid::GridLine)> {
    let project = h.state().project();
    let view = project.view();
    let class = hito_structural::class(grid::GRID);
    let mut ids: Vec<_> = project
        .elements()
        .filter(|e| e.class() == &class)
        .map(|e| e.id())
        .collect();
    ids.sort();
    ids.into_iter()
        .map(|id| match project.property(id, grid::NAME) {
            Some(Value::Text(label)) => (label.clone(), grid::GridLine::of(&view, id).unwrap()),
            other => panic!("{other:?}"),
        })
        .collect()
}

fn walls(h: &Harness<'_, Shell>) -> Vec<ElementId> {
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

fn close(a: [f64; 2], b: [f64; 2], tolerance: f64) -> bool {
    (a[0] - b[0]).abs() < tolerance && (a[1] - b[1]).abs() < tolerance
}

#[test]
fn each_level_has_a_plan_view_that_opens_as_a_tab() {
    let mut h = window();
    command_line(&mut h, "LL");
    command_line(&mut h, "3");
    esc(&mut h);
    assert_eq!(h.state().layout().plans().len(), 2, "one per level");

    let plan = open_plan(&mut h, "Level 2");
    let layout = h.state().layout();
    assert_eq!(layout.active_view(), Some(plan));
    assert!(layout.open_views().contains(&plan));
    assert!(h.state().viewport(plan).is_some_and(|v| v.is_plan()));
    assert!(
        h.query_by_label("Floor Plan").is_some(),
        "the properties show the view"
    );

    // Deleting the level deletes its plan, and closes its tab.
    let second = level_id(&h, "Level 2");
    h.state_mut()
        .select([second], hito::selection::Mode::Replace);
    h.run_steps(1);
    h.key_press(egui::Key::Delete);
    h.run_steps(2);
    let layout = h.state().layout();
    assert_eq!(layout.plans().len(), 1);
    assert!(!layout.open_views().contains(&plan));
}

#[test]
fn grids_and_walls_are_placed_by_clicking_in_a_plan_snapping_to_grid_intersections() {
    let mut h = window();
    let plan = open_plan(&mut h, "Level 1");
    // The grids' ends are clicked off any intersection.
    command_line(&mut h, "GR");
    let none = egui::Vec2::ZERO;
    click_at(&mut h, plan, [0.0, -2.0, 0.0], none);
    click_at(&mut h, plan, [0.0, 8.0, 0.0], none);
    click_at(&mut h, plan, [-2.0, 0.0, 0.0], none);
    click_at(&mut h, plan, [8.0, 0.0, 0.0], none);
    esc(&mut h);
    let drawn = grids(&h);
    assert_eq!(drawn.len(), 2);
    assert_eq!(drawn[0].0, "1");
    assert!(close(drawn[0].1.start, [0.0, -2.0], 0.05), "{:?}", drawn[0]);
    assert!(close(drawn[1].1.end, [8.0, 0.0], 0.05), "{:?}", drawn[1]);

    // A click a few points off where they cross snaps to it exactly.
    command_line(&mut h, "WA");
    let crossing = drawn[0].1.intersection(&drawn[1].1).unwrap();
    click_at(
        &mut h,
        plan,
        [crossing[0], crossing[1], 0.0],
        egui::vec2(4.0, -3.0),
    );
    click_at(&mut h, plan, [5.0, 0.0, 0.0], egui::vec2(0.0, 30.0));
    esc(&mut h);
    let walls = walls(&h);
    assert_eq!(walls.len(), 1);
    let view = h.state().project().view();
    let (start, end) = hito_architectural::wall::line(&view, walls[0]).unwrap();
    assert_eq!(start, crossing, "snapped");
    assert!(end[1] < -0.5, "not snapped: {end:?}");
}

#[test]
fn walls_clicked_in_a_plan_go_on_its_level() {
    let mut h = window();
    command_line(&mut h, "LL");
    command_line(&mut h, "3");
    esc(&mut h);
    let second = level_id(&h, "Level 2");
    // Started before the plan is open, the tool moves to the plan's level.
    command_line(&mut h, "WA");
    let plan = open_plan(&mut h, "Level 2");
    let none = egui::Vec2::ZERO;
    click_at(&mut h, plan, [0.0, 0.0, 3.0], none);
    click_at(&mut h, plan, [4.0, 0.0, 3.0], none);
    esc(&mut h);
    let walls = walls(&h);
    assert_eq!(walls.len(), 1);
    let view = h.state().project().view();
    assert_eq!(level::level_of(&view, walls[0]), Some(second));
}
