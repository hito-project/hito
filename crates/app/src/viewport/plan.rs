//! Plan views (#183): a level seen from above, as Revit's floor plans.
//!
//! A plan view is a [`Viewport`] whose camera looks straight down, north
//! up, and never orbits. It draws with egui's painter, in lines that stay
//! crisp at any zoom:
//!
//! - every element the plane [`CUT_HEIGHT`] above the level cuts, as a
//!   heavy outline where it's cut;
//! - the level's other elements, such as a slab or a beam, as thin lines:
//!   their edges seen from above;
//! - the grids, with their labels in bubbles.
//!
//! Both come from the elements' meshes, so any element with geometry
//! shows in plan without code of its own. Doors and windows are the
//! exception: as in Revit, they draw their plan [`Symbol`] (the jambs, the
//! leaf and its swing, the window's lines) instead of where the cut
//! crosses their 3D body. The wall's own cut shows the gap, since the
//! opening is cut out of the wall's mesh.
//!
//! A click selects as in the 3D views, in the same selection. While a
//! placement tool asks for a point, a click gives it the point under the
//! cursor instead, on the level, snapped to the nearest grid intersection
//! or column centre within [`SNAP_RADIUS`]: the same answer as typing the
//! point on the command line, so every tool takes clicks.

use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

use eframe::egui;
use glam::{DVec2, DVec3};
use hito_core::ElementId;

use super::camera::{Bounds, Camera};
use super::cut::{self, Segment};
use super::{BoxKind, Picked, Picker, Scene, Viewport, ZOOM_PER_POINT, pick, scene};
use crate::command::Wants;

/// How far above its level a plan cuts the model: 1.2 m, the cut plane of
/// Revit's metric templates.
pub const CUT_HEIGHT: f64 = 1.2;

/// How near a grid intersection or a column the cursor snaps to it, in
/// points, as Revit's snaps catch within a few pixels.
pub const SNAP_RADIUS: f64 = 10.0;

/// How near a line a click picks it, in points.
const PICK_RADIUS: f64 = 4.0;

/// The radius of a grid's bubble, in points.
const BUBBLE_RADIUS: f32 = 11.0;

/// The size of the cross that marks a snapped point, in points.
const SNAP_MARK: f32 = 6.0;

/// The width of cut lines and of the lines seen beyond them, in points.
const CUT_WIDTH: f32 = 2.0;
const BEYOND_WIDTH: f32 = 1.0;

/// What a plan view shows besides the elements' meshes, read from the
/// model each frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// The level's elevation, in metres: clicked points are on it.
    pub elevation: f64,
    pub grids: Vec<PlanGrid>,
    /// Where grids cross, which clicks snap to.
    pub snaps: Vec<[f64; 2]>,
    /// The centres of the columns that stand on the level or reach up to
    /// it, which a click on or near a column snaps to, so a beam's end
    /// frames into the column.
    pub centres: Vec<Centre>,
    /// The elements on the level, which show from above even where the
    /// cut misses them.
    pub on_level: BTreeSet<ElementId>,
    /// The elements that draw a symbol instead of their mesh, such as doors
    /// and windows. An empty symbol hides the element.
    pub symbols: Vec<Symbol>,
}

/// A point a click snaps to when the cursor is on or near an outline
/// round it, such as a column's centre and its section.
#[derive(Debug, Clone, PartialEq)]
pub struct Centre {
    pub point: [f64; 2],
    /// Closed, in plan.
    pub outline: Vec<[f64; 2]>,
}

/// What an element draws in plan instead of its mesh: a door's or a
/// window's plan symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub id: ElementId,
    /// Lines drawn as cut, heavy.
    pub cut: Vec<Segment>,
    /// Lines drawn thin.
    pub thin: Vec<Segment>,
}

/// A grid as a plan view draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanGrid {
    pub id: ElementId,
    pub label: String,
    pub start: [f64; 2],
    pub end: [f64; 2],
}

/// What happened in a plan view this frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlanOutput {
    /// A point clicked for the running tool, on the level, snapped.
    pub point: Option<[f64; 3]>,
    /// What a click or a box selected, when no tool asks for a point.
    pub picked: Option<Picked>,
}

/// What the cursor points at in a plan: a point on the level, and whether
/// it snapped to a grid intersection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pointed {
    pub point: [f64; 3],
    pub snapped: bool,
}

/// What a plan view draws of the elements, worked out again only when the
/// meshes, the level or the cut change.
#[derive(Debug, Default)]
pub(super) struct Drawing {
    key: Option<u64>,
    elements: Vec<Drawn>,
}

/// One element's lines in plan.
#[derive(Debug, Clone, PartialEq)]
struct Drawn {
    id: ElementId,
    /// Where the cut plane cuts it.
    cut: Vec<Segment>,
    /// Its edges seen from above, if the cut misses it.
    beyond: Vec<Segment>,
}

impl Plan {
    /// The height the plan cuts the model at.
    pub fn cut_height(&self) -> f64 {
        self.elevation + CUT_HEIGHT
    }

    /// The box round the grids, on the level, `None` if there are none.
    fn bounds(&self) -> Option<Bounds> {
        let z = self.elevation;
        self.grids
            .iter()
            .map(|g| {
                let (lo, hi) = (min2(g.start, g.end), max2(g.start, g.end));
                Bounds::new([lo[0], lo[1], z], [hi[0], hi[1], z])
            })
            .reduce(Bounds::union)
    }

    /// Changes whenever what the plan shows besides the meshes does.
    fn key(&self, scene: &Scene) -> u64 {
        let mut h = std::hash::DefaultHasher::new();
        scene.geometry_revision().hash(&mut h);
        self.elevation.to_bits().hash(&mut h);
        self.on_level.hash(&mut h);
        for symbol in &self.symbols {
            symbol.id.hash(&mut h);
            for lines in [&symbol.cut, &symbol.thin] {
                lines.len().hash(&mut h);
                for p in lines.iter().flatten() {
                    p.x.to_bits().hash(&mut h);
                    p.y.to_bits().hash(&mut h);
                }
            }
        }
        for grid in &self.grids {
            grid.id.hash(&mut h);
            for x in [grid.start, grid.end].into_iter().flatten() {
                x.to_bits().hash(&mut h);
            }
        }
        h.finish()
    }
}

fn min2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0].min(b[0]), a[1].min(b[1])]
}

fn max2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0].max(b[0]), a[1].max(b[1])]
}

impl Drawing {
    /// Works out the elements' lines again if the scene or the plan
    /// changed since the last time.
    fn update(&mut self, scene: &Scene, plan: &Plan) {
        let key = plan.key(scene);
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        let z = plan.cut_height();
        self.elements = scene
            .meshes()
            .filter_map(|(id, mesh)| {
                if let Some(symbol) = plan.symbols.iter().find(|s| s.id == id) {
                    let (cut, beyond) = (symbol.cut.clone(), symbol.thin.clone());
                    let shown = !cut.is_empty() || !beyond.is_empty();
                    return shown.then_some(Drawn { id, cut, beyond });
                }
                let cut = cut::section(mesh, z);
                let beyond = if cut.is_empty() && plan.on_level.contains(&id) {
                    cut::projection(mesh)
                } else {
                    Vec::new()
                };
                (!cut.is_empty() || !beyond.is_empty()).then_some(Drawn { id, cut, beyond })
            })
            .collect();
    }

    fn get(&self, id: ElementId) -> Option<&Drawn> {
        self.elements.iter().find(|d| d.id == id)
    }
}

impl Viewport {
    /// A plan view: looking straight down, north up.
    pub fn plan() -> Self {
        let mut viewport = Viewport {
            is_plan: true,
            ..Viewport::default()
        };
        viewport.camera.look_from(DVec3::Z);
        viewport
    }

    pub fn is_plan(&self) -> bool {
        self.is_plan
    }

    /// Where a point of the model shows in the window, once the view has
    /// shown.
    pub fn to_screen(&self, point: [f64; 3]) -> Option<egui::Pos2> {
        let rect = self.rect?;
        let p = self.camera.project(point.into(), self.size)?;
        Some(rect.min + egui::vec2(p.x as f32, p.y as f32))
    }

    /// What a cursor at `screen` (in the window) points at on the plan.
    pub fn pointed(&self, plan: &Plan, screen: egui::Pos2) -> Option<Pointed> {
        let rect = self.rect?;
        let local = DVec2::new(
            (screen.x - rect.min.x).into(),
            (screen.y - rect.min.y).into(),
        );
        Some(pointed(self, plan, local))
    }

    /// The elements a plan view draws, as of its last frame.
    pub fn drawn(&self) -> Vec<ElementId> {
        self.drawing.elements.iter().map(|d| d.id).collect()
    }

    /// Fills `ui` with the plan and moves the camera as the mouse says. A
    /// click is a point when `wants` asks for one, and selects otherwise.
    pub fn plan_ui(
        &mut self,
        ui: &mut egui::Ui,
        scene: &mut Scene,
        plan: &Plan,
        wants: Option<Wants>,
    ) -> PlanOutput {
        let tab = self.claim_tab(ui.ctx());
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        self.hovered = response.hover_pos().is_some();
        self.rect = Some(rect);
        self.size = DVec2::new(rect.width().into(), rect.height().into());
        self.plan_bounds = Some(plan_bounds(scene, plan));
        if !self.framed {
            self.zoom_to_fit(scene);
            self.framed = true;
        }
        self.navigate_plan(ui, &response);
        self.drawing.update(scene, plan);

        let painter = ui.painter_at(rect);
        let [r, g, b] = if ui.visuals().dark_mode {
            super::BACKGROUND_DARK
        } else {
            super::BACKGROUND
        };
        // Drawn first, so the selection box goes on top.
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(r, g, b));
        let shapes = painter.add(egui::Shape::Noop);

        let pointing = matches!(wants, Some(Wants::Point | Wants::PointFrom(_)));
        let mut output = PlanOutput::default();
        if pointing {
            if self.prehighlighting {
                scene.set_prehighlight(None);
                self.prehighlighting = false;
            }
            output.point = self.point_ui(ui, &response, &painter, plan, wants);
        } else {
            output.picked = self.select_in_plan(ui, &response, scene, plan, tab);
        }
        let mut lines = Vec::new();
        self.draw_elements(&mut lines, ui, scene, plan);
        self.draw_grids(&mut lines, &painter, ui, scene, plan);
        painter.set(shapes, egui::Shape::Vec(lines));
        output
    }

    /// While a tool asks for a point: the snap mark and the rubber band
    /// under the cursor, and the point a click gives.
    fn point_ui(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        painter: &egui::Painter,
        plan: &Plan,
        wants: Option<Wants>,
    ) -> Option<[f64; 3]> {
        let hover = response.hover_pos()?;
        let at = self.pointed(plan, hover)?;
        let accent = ui.visuals().selection.stroke.color;
        let cursor = self.to_screen(at.point).unwrap_or(hover);
        if let Some(Wants::PointFrom(base)) = wants
            && let Some(from) = self.to_screen([base[0], base[1], plan.elevation])
        {
            painter.line_segment([from, cursor], egui::Stroke::new(1.0, accent));
        }
        if at.snapped {
            let d = SNAP_MARK;
            let stroke = egui::Stroke::new(1.5, accent);
            painter.line_segment(
                [cursor + egui::vec2(-d, -d), cursor + egui::vec2(d, d)],
                stroke,
            );
            painter.line_segment(
                [cursor + egui::vec2(-d, d), cursor + egui::vec2(d, -d)],
                stroke,
            );
        }
        response
            .clicked_by(egui::PointerButton::Primary)
            .then_some(at.point)
    }

    /// Selects as the 3D views do, among what the plan draws: a cut
    /// element by clicking inside its outline, anything else by clicking
    /// on its lines.
    fn select_in_plan(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        scene: &mut Scene,
        plan: &Plan,
        tab: bool,
    ) -> Option<Picked> {
        let drawing = std::mem::take(&mut self.drawing);
        // Seen from above everything, so a ray from the eye meets the
        // tallest element first.
        let top = scene.bounds().map_or(0.0, |b| b.max.z) + 1.0;
        let revision = drawing.key.unwrap_or_default();
        let z = plan.elevation;
        let on_screen = |camera: &Camera, size: DVec2, [a, b]: Segment| {
            let a = camera.project(a.extend(z), size)?;
            let b = camera.project(b.extend(z), size)?;
            Some([a, b])
        };
        let near_lines = |camera: &Camera, size, segments: &[Segment], at: DVec2| {
            segments.iter().any(|&s| {
                on_screen(camera, size, s).is_some_and(|s| cut::distance(at, s) <= PICK_RADIUS)
            })
        };
        let grid_line = |camera: &Camera, size, g: &PlanGrid| {
            on_screen(camera, size, [g.start.into(), g.end.into()])
        };
        let under = |scene: &Scene, camera: &Camera, at: DVec2, size: DVec2| {
            let lifted = camera.lifted(top);
            let mut ids: Vec<ElementId> = pick::under(scene, &lifted, at, size)
                .into_iter()
                .filter(|&id| drawing.get(id).is_some_and(|d| !d.cut.is_empty()))
                .collect();
            for drawn in &drawing.elements {
                let lines = [&drawn.cut, &drawn.beyond];
                if !ids.contains(&drawn.id) && lines.iter().any(|l| near_lines(camera, size, l, at))
                {
                    ids.push(drawn.id);
                }
            }
            for grid in &plan.grids {
                if grid_line(camera, size, grid)
                    .is_some_and(|l| cut::distance(at, l) <= PICK_RADIUS)
                {
                    ids.push(grid.id);
                }
            }
            ids
        };
        let in_box =
            |scene: &Scene, camera: &Camera, corners: [DVec2; 2], kind: BoxKind, size: DVec2| {
                let lifted = camera.lifted(top);
                let mut ids: Vec<ElementId> = pick::in_box(scene, &lifted, corners, kind, size)
                    .into_iter()
                    .filter(|&id| drawing.get(id).is_some())
                    .collect();
                let (lo, hi) = (corners[0].min(corners[1]), corners[0].max(corners[1]));
                let inside = |p: DVec2| p.cmpge(lo).all() && p.cmple(hi).all();
                for grid in &plan.grids {
                    let Some(line) = grid_line(camera, size, grid) else {
                        continue;
                    };
                    let taken = match kind {
                        BoxKind::Window => line.iter().all(|&p| inside(p)),
                        BoxKind::Crossing => cut::meets_box(line, lo, hi),
                    };
                    if taken {
                        ids.push(grid.id);
                    }
                }
                ids
            };
        let picker = Picker {
            revision,
            under: &under,
            in_box: &in_box,
        };
        let picked = self.select(ui, response, scene, tab, &picker);
        self.drawing = drawing;
        picked
    }

    /// Zooms with the wheel, pans with the wheel held down (a plan doesn't
    /// orbit, so Shift makes no difference), and zooms to fit on a double
    /// click of the wheel, as in Revit.
    fn navigate_plan(&mut self, ui: &egui::Ui, response: &egui::Response) {
        let size = self.size;
        if let Some(pointer) = response.hover_pos() {
            let (scroll, pinch) = ui.input(|i| {
                let d = i.smooth_scroll_delta;
                (d.x + d.y, i.zoom_delta())
            });
            let factor = (f64::from(scroll) * ZOOM_PER_POINT).exp() * f64::from(pinch);
            if factor != 1.0 {
                let at = pointer - response.rect.min;
                self.camera
                    .zoom(DVec2::new(at.x.into(), at.y.into()), size, factor);
            }
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            let d = response.drag_delta();
            self.camera.pan(DVec2::new(d.x.into(), d.y.into()), size);
        }
        if response.double_clicked_by(egui::PointerButton::Middle)
            && let Some(bounds) = self.plan_bounds
        {
            self.camera.fit(bounds, size);
        }
    }

    /// The colour an element or a grid draws in: Revit's selection blue if
    /// it's selected, a lighter blue if a click would select it.
    fn ink(&self, ui: &egui::Ui, scene: &Scene, id: ElementId) -> egui::Color32 {
        let rgb = |c: [f32; 4]| egui::Rgba::from_rgb(c[0], c[1], c[2]).into();
        if scene.selected().contains(&id) {
            rgb(scene::SELECTED_EDGE)
        } else if scene.prehighlight() == Some(id) {
            rgb(scene::PREHIGHLIGHT_EDGE)
        } else {
            ui.visuals().strong_text_color()
        }
    }

    /// Each element's cut outline, heavy, and the edges of the level's
    /// other elements, thin.
    fn draw_elements(
        &self,
        lines: &mut Vec<egui::Shape>,
        ui: &egui::Ui,
        scene: &Scene,
        plan: &Plan,
    ) {
        let z = plan.elevation;
        let screen = |p: DVec2| self.to_screen([p.x, p.y, z]);
        for drawn in &self.drawing.elements {
            let ink = self.ink(ui, scene, drawn.id);
            for (segments, width) in [(&drawn.beyond, BEYOND_WIDTH), (&drawn.cut, CUT_WIDTH)] {
                let stroke = egui::Stroke::new(width, ink);
                for &[a, b] in segments {
                    if let (Some(a), Some(b)) = (screen(a), screen(b)) {
                        lines.push(egui::Shape::line_segment([a, b], stroke));
                    }
                }
            }
        }
    }

    /// Each grid as a dashed line, Revit's center line, with its label in
    /// a bubble at its end.
    fn draw_grids(
        &self,
        lines: &mut Vec<egui::Shape>,
        painter: &egui::Painter,
        ui: &egui::Ui,
        scene: &Scene,
        plan: &Plan,
    ) {
        let z = plan.elevation;
        for grid in &plan.grids {
            let (Some(a), Some(b)) = (
                self.to_screen([grid.start[0], grid.start[1], z]),
                self.to_screen([grid.end[0], grid.end[1], z]),
            ) else {
                continue;
            };
            let along = (b - a).normalized();
            if !along.x.is_finite() {
                continue;
            }
            let ink = self.ink(ui, scene, grid.id);
            let stroke = egui::Stroke::new(1.0, ink);
            let center = b + along * BUBBLE_RADIUS;
            lines.extend(egui::Shape::dashed_line(&[a, b], stroke, 12.0, 4.0));
            lines.push(egui::Shape::circle_stroke(center, BUBBLE_RADIUS, stroke));
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                &grid.label,
                egui::FontId::proportional(12.0),
                ink,
            );
        }
    }
}

/// What a cursor at `local` (points from the view's top-left corner)
/// points at on the plan: the nearest grid intersection or column centre
/// within [`SNAP_RADIUS`], or else the point under it, on the level. A
/// column is as near as its outline, so anywhere on it is nearest of all.
fn pointed(viewport: &Viewport, plan: &Plan, local: DVec2) -> Pointed {
    let (camera, size, z) = (&viewport.camera, viewport.size, plan.elevation);
    let on_screen = |[x, y]: [f64; 2]| camera.project(DVec3::new(x, y, z), size);
    let crossings = plan.snaps.iter().filter_map(|&[x, y]| {
        let screen = on_screen([x, y])?;
        Some((screen.distance(local), [x, y, z]))
    });
    let centres = plan.centres.iter().filter_map(|centre| {
        let outline: Option<Vec<DVec2>> = centre.outline.iter().map(|&p| on_screen(p)).collect();
        let [x, y] = centre.point;
        Some((distance_to_outline(local, &outline?), [x, y, z]))
    });
    let nearest = crossings
        .chain(centres)
        .filter(|(distance, _)| *distance <= SNAP_RADIUS)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    match nearest {
        Some((_, point)) => Pointed {
            point,
            snapped: true,
        },
        None => {
            let p = camera.point_at(local, size);
            Pointed {
                point: [p.x, p.y, z],
                snapped: false,
            }
        }
    }
}

/// How far `point` is from a closed outline: 0 inside it, or else the
/// distance to its nearest side.
fn distance_to_outline(point: DVec2, outline: &[DVec2]) -> f64 {
    let sides = || outline.iter().zip(outline.iter().cycle().skip(1));
    // Even-odd: a ray to the right crosses the outline an odd number of
    // times from inside.
    let crossings = sides()
        .filter(|&(a, b)| {
            (a.y > point.y) != (b.y > point.y)
                && point.x < a.x + (point.y - a.y) / (b.y - a.y) * (b.x - a.x)
        })
        .count();
    if crossings % 2 == 1 {
        return 0.0;
    }
    sides()
        .map(|(&a, &b)| cut::distance(point, [a, b]))
        .fold(f64::INFINITY, f64::min)
}

/// What zooming to fit a plan shows: the elements and the grids, seen
/// from above, or the area round the origin if there's nothing.
fn plan_bounds(scene: &Scene, plan: &Plan) -> Bounds {
    let z = plan.elevation;
    let flat = |b: Bounds| Bounds::new([b.min.x, b.min.y, z], [b.max.x, b.max.y, z]);
    let elements = scene.bounds().map(flat);
    match (elements, plan.bounds()) {
        (Some(a), Some(b)) => a.union(b),
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => Bounds::new([-10.0, -10.0, z], [10.0, 10.0, z]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewport::scene::tests::{column, mesh};
    use hito_geometry::{Frame, Profile, Solid};

    const SIZE: DVec2 = DVec2::new(800.0, 600.0);

    /// Two grids crossing at (6, 0), seen whole from above, at 3 m.
    fn plan_view() -> (Viewport, Plan) {
        let plan = Plan {
            elevation: 3.0,
            grids: vec![
                PlanGrid {
                    id: ElementId::generate(),
                    label: "1".into(),
                    start: [0.0, 0.0],
                    end: [12.0, 0.0],
                },
                PlanGrid {
                    id: ElementId::generate(),
                    label: "A".into(),
                    start: [6.0, -6.0],
                    end: [6.0, 6.0],
                },
            ],
            snaps: vec![[6.0, 0.0]],
            centres: Vec::new(),
            on_level: BTreeSet::new(),
            symbols: Vec::new(),
        };
        let mut viewport = Viewport::plan();
        viewport.size = SIZE;
        viewport.rect = Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(800.0, 600.0),
        ));
        viewport
            .camera
            .fit(plan_bounds(&Scene::default(), &plan), SIZE);
        (viewport, plan)
    }

    fn screen(viewport: &Viewport, x: f64, y: f64) -> DVec2 {
        viewport
            .camera
            .project(DVec3::new(x, y, 3.0), SIZE)
            .unwrap()
    }

    #[test]
    fn a_plan_looks_down_with_north_up() {
        let (viewport, _) = plan_view();
        let (east, north) = (screen(&viewport, 1.0, 0.0), screen(&viewport, 0.0, 1.0));
        let origin = screen(&viewport, 0.0, 0.0);
        assert!(east.x > origin.x && (east.y - origin.y).abs() < 1e-9);
        assert!(north.y < origin.y && (north.x - origin.x).abs() < 1e-9);
    }

    #[test]
    fn the_cursor_points_on_the_level() {
        let (viewport, plan) = plan_view();
        let at = pointed(&viewport, &plan, screen(&viewport, 2.5, 4.0));
        assert!(!at.snapped);
        let [x, y, z] = at.point;
        assert!((x - 2.5).abs() < 1e-9 && (y - 4.0).abs() < 1e-9, "{x} {y}");
        assert_eq!(z, 3.0, "on the level");
    }

    #[test]
    fn near_a_grid_intersection_it_snaps_to_it() {
        let (viewport, plan) = plan_view();
        let crossing = screen(&viewport, 6.0, 0.0);
        let near = pointed(&viewport, &plan, crossing + DVec2::new(6.0, -5.0));
        assert_eq!(
            near,
            Pointed {
                point: [6.0, 0.0, 3.0],
                snapped: true
            }
        );
        let far = pointed(
            &viewport,
            &plan,
            crossing + DVec2::new(SNAP_RADIUS + 1.0, 0.0),
        );
        assert!(!far.snapped);
    }

    #[test]
    fn on_or_near_a_column_it_snaps_to_its_centre() {
        let (viewport, mut plan) = plan_view();
        // A 40 cm column centred at (2, 3), off any grid intersection.
        let [x, y, h] = [2.0, 3.0, 0.2];
        plan.centres.push(Centre {
            point: [x, y],
            outline: vec![
                [x - h, y - h],
                [x + h, y - h],
                [x + h, y + h],
                [x - h, y + h],
            ],
        });
        let centre = Pointed {
            point: [x, y, 3.0],
            snapped: true,
        };
        let inside = screen(&viewport, x + 0.15, y - 0.1);
        assert_eq!(pointed(&viewport, &plan, inside), centre, "on it");
        let face = screen(&viewport, x + h, y);
        let near = face + DVec2::new(SNAP_RADIUS - 1.0, 0.0);
        assert_eq!(pointed(&viewport, &plan, near), centre, "near its face");
        let far = face + DVec2::new(SNAP_RADIUS + 1.0, 0.0);
        assert!(!pointed(&viewport, &plan, far).snapped);
    }

    #[test]
    fn distance_to_an_outline_is_nothing_inside_it() {
        let square = [(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)].map(DVec2::from);
        assert_eq!(distance_to_outline(DVec2::new(1.0, 1.5), &square), 0.0);
        assert_eq!(distance_to_outline(DVec2::new(3.0, 1.0), &square), 1.0);
        assert_eq!(
            distance_to_outline(DVec2::new(-1.0, 3.0), &square),
            2f64.sqrt()
        );
    }

    #[test]
    fn zoom_to_fit_shows_the_grids() {
        let (viewport, plan) = plan_view();
        for grid in &plan.grids {
            for [x, y] in [grid.start, grid.end] {
                let p = screen(&viewport, x, y);
                assert!(p.x > 0.0 && p.x < SIZE.x && p.y > 0.0 && p.y < SIZE.y);
            }
        }
    }

    /// A 15 cm slab from (0, 0) to (6, 4), its top at `top`.
    fn slab(top: f64) -> std::sync::Arc<hito_geometry::Mesh> {
        mesh(Solid::Extrusion {
            profile: Profile::Rectangle {
                width: 6.0,
                height: 4.0,
            },
            frame: Frame::at([3.0, 2.0, top - 0.15]),
            depth: 0.15,
        })
    }

    #[test]
    fn the_cut_outlines_what_it_crosses_and_the_level_shows_from_above() {
        // A column through the cut, a slab on the level below it, a slab
        // on another level, and a column wholly above the cut.
        let mut scene = Scene::default();
        let [standing, on_level, other, above] = [(); 4].map(|_| ElementId::generate());
        scene.insert(standing, column(0.0, 0.4));
        scene.insert(on_level, slab(0.0));
        scene.insert(other, slab(-3.0));
        let high = column(2.0, 0.4);
        let high = hito_geometry::Mesh {
            positions: high
                .positions
                .iter()
                .map(|p| [p[0], p[1], p[2] + 2.0])
                .collect(),
            triangles: high.triangles.clone(),
        };
        scene.insert(above, std::sync::Arc::new(high));
        let plan = Plan {
            on_level: [standing, on_level].into(),
            ..Plan::default()
        };
        let mut drawing = Drawing::default();
        drawing.update(&scene, &plan);
        let column = drawing.get(standing).unwrap();
        assert_eq!(column.cut.len(), 8, "its square, two triangles a side");
        assert!(column.beyond.is_empty());
        let slab = drawing.get(on_level).unwrap();
        assert!(slab.cut.is_empty() && !slab.beyond.is_empty());
        assert!(drawing.get(other).is_none(), "another level's, uncut");
        assert!(drawing.get(above).is_none(), "above the cut");

        let key = drawing.key;
        drawing.update(&scene, &plan);
        assert_eq!(drawing.key, key, "nothing changed");
        scene.remove(standing);
        drawing.update(&scene, &plan);
        assert!(drawing.get(standing).is_none(), "follows the scene");
    }

    #[test]
    fn a_symbol_draws_instead_of_the_mesh_and_an_empty_one_hides_it() {
        let mut scene = Scene::default();
        let [door, high] = [(); 2].map(|_| ElementId::generate());
        scene.insert(door, column(0.0, 0.4));
        scene.insert(high, column(3.0, 0.4));
        let leaf = [DVec2::ZERO, DVec2::new(0.0, -0.8)];
        let jamb = [DVec2::ZERO, DVec2::new(0.05, 0.0)];
        let mut plan = Plan {
            symbols: vec![
                Symbol {
                    id: door,
                    cut: vec![jamb],
                    thin: vec![leaf],
                },
                Symbol {
                    id: high,
                    cut: Vec::new(),
                    thin: Vec::new(),
                },
            ],
            ..Plan::default()
        };
        let mut drawing = Drawing::default();
        drawing.update(&scene, &plan);
        let drawn = drawing.get(door).unwrap();
        assert_eq!(
            (&drawn.cut[..], &drawn.beyond[..]),
            (&[jamb][..], &[leaf][..])
        );
        assert!(drawing.get(high).is_none(), "the cut misses it");

        // Flipping the door changes its symbol, and the drawing follows.
        plan.symbols[0].thin = vec![[DVec2::ZERO, DVec2::new(0.0, 0.8)]];
        drawing.update(&scene, &plan);
        assert_eq!(drawing.get(door).unwrap().beyond, plan.symbols[0].thin);
    }
}
