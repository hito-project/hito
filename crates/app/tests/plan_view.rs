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
    let shown = |h: &Harness<'_, Shell>| h.query_all_by_label(name).count();
    let before = shown(h);
    h.get_by_label("Floor Plans").click();
    h.run_steps(2);
    if shown(h) < before {
        // It was open, and that click closed it.
        h.run_steps(60);
        h.get_by_label("Floor Plans").click();
    }
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

fn selected(h: &Harness<'_, Shell>) -> Vec<ElementId> {
    h.state().selection().ids().iter().copied().collect()
}

#[test]
fn a_plan_draws_what_its_cut_crosses_and_selects_with_the_3d_view() {
    let mut h = window();
    command_line(&mut h, "LL");
    command_line(&mut h, "3");
    esc(&mut h);
    let plan = open_plan(&mut h, "Level 1");
    let none = egui::Vec2::ZERO;
    command_line(&mut h, "GR");
    click_at(&mut h, plan, [0.0, -2.0, 0.0], none);
    click_at(&mut h, plan, [0.0, 6.0, 0.0], none);
    esc(&mut h);
    command_line(&mut h, "WA");
    click_at(&mut h, plan, [2.0, 0.0, 0.0], none);
    click_at(&mut h, plan, [8.0, 0.0, 0.0], none);
    esc(&mut h);
    let wall = walls(&h)[0];
    assert_eq!(
        h.state().viewport(plan).unwrap().drawn(),
        [wall],
        "the cut crosses the wall"
    );

    // A click on the wall selects it, in the 3D view too.
    click_at(&mut h, plan, [5.0, 0.0, 0.0], none);
    assert_eq!(selected(&h), [wall]);
    assert!(h.state().scene().selected().contains(&wall), "in 3D");
    // A click on the grid selects it, and the properties edit its label.
    click_at(&mut h, plan, [0.0, 3.0, 0.0], egui::vec2(2.0, 0.0));
    let grid = grid::find(&h.state().project().view(), "1").unwrap();
    assert_eq!(selected(&h), [grid]);
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Label")
            .is_some()
    );
    // A click on nothing clears the selection.
    click_at(&mut h, plan, [5.0, 4.0, 0.0], none);
    assert!(selected(&h).is_empty());

    // Level 2's plan cuts above the wall, which is on Level 1.
    let upper = open_plan(&mut h, "Level 2");
    h.run_steps(2);
    assert!(h.state().viewport(upper).unwrap().drawn().is_empty());
}

/// The doors and windows hosted by `wall`, in order along it, each with
/// whether it's facing-flipped.
fn openings(h: &Harness<'_, Shell>, wall: ElementId) -> Vec<(ElementId, bool)> {
    use hito_architectural::opening;
    let view = h.state().project().view();
    let mut hosted: Vec<_> = opening::hosted(&view, wall)
        .into_iter()
        .map(|id| {
            let along = opening::cut(&view, id).unwrap().from;
            let flipped = view.property(id, opening::FACING_FLIPPED) == Some(&Value::Boolean(true));
            (along, id, flipped)
        })
        .collect();
    hosted.sort_by(|a, b| a.0.total_cmp(&b.0));
    hosted.into_iter().map(|(_, id, f)| (id, f)).collect()
}

#[test]
fn doors_and_windows_are_placed_by_clicking_a_wall_in_plan_and_draw_their_symbol() {
    let mut h = window();
    let plan = open_plan(&mut h, "Level 1");
    let none = egui::Vec2::ZERO;
    command_line(&mut h, "WA");
    click_at(&mut h, plan, [0.0, 0.0, 0.0], none);
    click_at(&mut h, plan, [8.0, 0.0, 0.0], none);
    esc(&mut h);
    let wall = walls(&h)[0];

    // The wall runs along x, so its exterior is +y and its interior -y.
    // A door swings to the side of the wall it's clicked on.
    command_line(&mut h, "DR");
    click_at(&mut h, plan, [2.0, -0.05, 0.0], none);
    click_at(&mut h, plan, [4.0, 0.05, 0.0], none);
    esc(&mut h);
    command_line(&mut h, "WN");
    click_at(&mut h, plan, [6.5, 0.0, 0.0], none);
    esc(&mut h);
    let placed = openings(&h, wall);
    assert_eq!(placed.len(), 3, "two doors and a window");
    let flips: Vec<_> = placed[..2].iter().map(|o| o.1).collect();
    assert_eq!(flips, [false, true], "{placed:?}");

    // The plan draws the wall, with its gaps, and each one's symbol.
    let mut drawn = h.state().viewport(plan).unwrap().drawn();
    drawn.sort();
    let mut expected: Vec<_> = placed.iter().map(|o| o.0).chain([wall]).collect();
    expected.sort();
    assert_eq!(drawn, expected);

    // A click on a door's swing selects the door.
    click_at(&mut h, plan, [2.0, -0.5, 0.0], none);
    assert_eq!(selected(&h), [placed[0].0]);
}

fn of_class(h: &Harness<'_, Shell>, name: &str) -> Vec<ElementId> {
    let project = h.state().project();
    let class = hito_structural::class(name);
    let mut ids: Vec<_> = project
        .elements()
        .filter(|e| e.class() == &class)
        .map(|e| e.id())
        .collect();
    ids.sort();
    ids
}

#[test]
fn columns_snap_to_grid_intersections_and_beams_frame_into_columns_clicked_near() {
    use hito_structural::{beam, column};
    let mut h = window();
    command_line(&mut h, "LL");
    command_line(&mut h, "3");
    esc(&mut h);
    let plan = open_plan(&mut h, "Level 1");
    let none = egui::Vec2::ZERO;
    command_line(&mut h, "GR");
    click_at(&mut h, plan, [0.0, -3.0, 0.0], none);
    click_at(&mut h, plan, [0.0, 3.0, 0.0], none);
    esc(&mut h);

    // A column clicked a few points off where two grids cross goes on
    // both grids; one clicked away from any crossing is free.
    command_line(&mut h, "GR");
    click_at(&mut h, plan, [-3.0, 0.0, 0.0], none);
    click_at(&mut h, plan, [9.0, 0.0, 0.0], none);
    esc(&mut h);
    command_line(&mut h, "CL");
    click_at(&mut h, plan, [0.0, 0.0, 0.0], egui::vec2(5.0, -4.0));
    click_at(&mut h, plan, [6.0, 0.0, 0.0], none);
    esc(&mut h);
    let columns = of_class(&h, column::COLUMN);
    assert_eq!(columns.len(), 2);
    {
        let view = h.state().project().view();
        assert!(column::grids(&view, columns[0]).is_some(), "on the grids");
        assert_eq!(column::location(&view, columns[0]), Some([0.0, 0.0]));
        assert!(column::grids(&view, columns[1]).is_none());
    }

    // On Level 2, the beam's start is clicked just off the first column's
    // face, and its end inside the second column, off its centre. Both
    // frame into their columns, from the Level 1 columns reaching up.
    let upper = open_plan(&mut h, "Level 2");
    command_line(&mut h, "BM");
    click_at(&mut h, upper, [0.15, 0.0, 3.0], egui::vec2(6.0, 0.0));
    click_at(&mut h, upper, [6.08, 0.05, 3.0], none);
    esc(&mut h);
    let beams = of_class(&h, beam::BEAM);
    assert_eq!(beams.len(), 1);
    let view = h.state().project().view();
    assert_eq!(
        beam::columns(&view, beams[0]),
        [Some(columns[0]), Some(columns[1])]
    );
}
