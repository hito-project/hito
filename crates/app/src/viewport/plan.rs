//! Plan views (#183): a level seen from above, as Revit's floor plans.
//!
//! A plan view is a [`Viewport`] whose camera looks straight down, north
//! up, and never orbits. It shows the grids with their labels, drawn with
//! egui's painter as lines, which stay crisp at any zoom.
//!
//! While a placement tool asks for a point, a click gives it the point
//! under the cursor, on the level, snapped to the nearest grid
//! intersection within [`SNAP_RADIUS`]: the same answer as typing the
//! point on the command line, so every tool takes clicks.

use eframe::egui;
use glam::{DVec2, DVec3};
use hito_core::ElementId;

use super::camera::Bounds;
use super::{Scene, Viewport, ZOOM_PER_POINT};
use crate::command::Wants;

/// How near a grid intersection the cursor snaps to it, in points, as
/// Revit's snaps catch within a few pixels.
pub const SNAP_RADIUS: f64 = 10.0;

/// The radius of a grid's bubble, in points.
const BUBBLE_RADIUS: f32 = 11.0;

/// The size of the cross that marks a snapped point, in points.
const SNAP_MARK: f32 = 6.0;

/// What a plan view shows besides the elements, read from the model each
/// frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// The level's elevation, in metres: clicked points are on it.
    pub elevation: f64,
    pub grids: Vec<PlanGrid>,
    /// Where grids cross, which clicks snap to.
    pub snaps: Vec<[f64; 2]>,
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
}

/// What the cursor points at in a plan: a point on the level, and whether
/// it snapped to a grid intersection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pointed {
    pub point: [f64; 3],
    pub snapped: bool,
}

impl Plan {
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
}

fn min2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0].min(b[0]), a[1].min(b[1])]
}

fn max2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0].max(b[0]), a[1].max(b[1])]
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

    /// Fills `ui` with the plan, moves the camera as the mouse says, and
    /// takes a click as a point when `wants` asks for one.
    pub fn plan_ui(
        &mut self,
        ui: &mut egui::Ui,
        scene: &Scene,
        plan: &Plan,
        wants: Option<Wants>,
    ) -> PlanOutput {
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

        let painter = ui.painter_at(rect);
        let visuals = ui.visuals();
        let background = if visuals.dark_mode {
            super::BACKGROUND_DARK
        } else {
            super::BACKGROUND
        };
        let [r, g, b] = background;
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(r, g, b));
        let ink = visuals.strong_text_color();
        self.draw_grids(&painter, plan, ink);

        let mut output = PlanOutput::default();
        let pointing = matches!(wants, Some(Wants::Point | Wants::PointFrom(_)));
        if !pointing {
            return output;
        }
        let Some(hover) = response.hover_pos() else {
            return output;
        };
        let Some(at) = self.pointed(plan, hover) else {
            return output;
        };
        let accent = visuals.selection.stroke.color;
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
        if response.clicked_by(egui::PointerButton::Primary) {
            output.point = Some(at.point);
        }
        output
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

    /// Each grid as a dashed line, Revit's center line, with its label in
    /// a bubble at its end.
    fn draw_grids(&self, painter: &egui::Painter, plan: &Plan, ink: egui::Color32) {
        let z = plan.elevation;
        let stroke = egui::Stroke::new(1.0, ink);
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
            let center = b + along * BUBBLE_RADIUS;
            painter.extend(egui::Shape::dashed_line(&[a, b], stroke, 12.0, 4.0));
            painter.circle_stroke(center, BUBBLE_RADIUS, stroke);
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
/// points at on the plan: the nearest grid intersection within
/// [`SNAP_RADIUS`], or else the point under it, on the level.
fn pointed(viewport: &Viewport, plan: &Plan, local: DVec2) -> Pointed {
    let (camera, size, z) = (&viewport.camera, viewport.size, plan.elevation);
    let nearest = plan
        .snaps
        .iter()
        .filter_map(|&[x, y]| {
            let screen = camera.project(DVec3::new(x, y, z), size)?;
            Some((screen.distance(local), [x, y, z]))
        })
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
    fn zoom_to_fit_shows_the_grids() {
        let (viewport, plan) = plan_view();
        for grid in &plan.grids {
            for [x, y] in [grid.start, grid.end] {
                let p = screen(&viewport, x, y);
                assert!(p.x > 0.0 && p.x < SIZE.x && p.y > 0.0 && p.y < SIZE.y);
            }
        }
    }
}
