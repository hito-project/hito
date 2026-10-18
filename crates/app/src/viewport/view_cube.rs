//! The view cube in a 3D view's top-right corner, as Revit's ViewCube: it
//! turns with the model, a click on a face, edge or corner looks from that
//! side, dragging it orbits, and the house above it goes to the home view.
//! Its context menu switches between perspective and orthographic.

use eframe::egui;
use glam::{DVec2, DVec3};

use super::camera::{Camera, Projection};
use crate::i18n::I18n;

/// The cube's half width, in points.
const HALF: f32 = 28.0;
/// The space the cube takes in the corner, and its margin from the edges.
const AREA: f32 = 2.0 * 1.8 * HALF;
const MARGIN: f32 = 8.0;
/// Where a face is split into its corner, edge and middle parts, in face
/// coordinates from -1 to 1. Clicking near a face's edge looks from that
/// edge, as on Revit's ViewCube.
const SPLITS: [f64; 4] = [-1.0, -0.6, 0.6, 1.0];

/// A face of the cube: its outward normal, two axes along it (with
/// `a × b = normal`), and the message key of its label.
struct Face {
    normal: DVec3,
    a: DVec3,
    b: DVec3,
    label: &'static str,
}

const FACES: [Face; 6] = [
    face(DVec3::Z, DVec3::X, DVec3::Y, "view-cube-top"),
    face(DVec3::NEG_Z, DVec3::Y, DVec3::X, "view-cube-bottom"),
    face(DVec3::NEG_Y, DVec3::X, DVec3::Z, "view-cube-front"),
    face(DVec3::Y, DVec3::Z, DVec3::X, "view-cube-back"),
    face(DVec3::X, DVec3::Y, DVec3::Z, "view-cube-right"),
    face(DVec3::NEG_X, DVec3::Z, DVec3::Y, "view-cube-left"),
];

const fn face(normal: DVec3, a: DVec3, b: DVec3, label: &'static str) -> Face {
    Face {
        normal,
        a,
        b,
        label,
    }
}

/// What the user did with the view cube.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum CubeAction {
    /// Look from this direction, and zoom to fit, as Revit does by default.
    LookFrom(DVec3),
    Home,
    /// Orbit by a mouse movement, in points.
    Orbit(DVec2),
    SetProjection(Projection),
}

/// A part of a face the pointer is on: the face, and which of its 3 × 3
/// parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Part {
    face: usize,
    i: usize,
    j: usize,
}

impl Part {
    /// The direction to look from: the face's, tilted towards the edge or
    /// corner the part is on.
    pub fn direction(self) -> DVec3 {
        let f = &FACES[self.face];
        let step = |k: usize| k as f64 - 1.0;
        (f.normal + f.a * step(self.i) + f.b * step(self.j)).normalize()
    }
}

/// The cube as it shows for a camera: where it is on the screen and how it
/// projects.
pub(super) struct Cube {
    center: egui::Pos2,
    right: DVec3,
    up: DVec3,
    toward_viewer: DVec3,
}

impl Cube {
    /// The cube in the top-right corner of a view.
    pub fn new(view: egui::Rect, camera: &Camera) -> Self {
        Self {
            center: egui::pos2(
                view.right() - MARGIN - AREA / 2.0,
                view.top() + MARGIN + AREA / 2.0,
            ),
            right: camera.right(),
            up: camera.up(),
            toward_viewer: camera.backward(),
        }
    }

    /// The area it takes, which the view leaves to it.
    pub fn rect(&self) -> egui::Rect {
        egui::Rect::from_center_size(self.center, egui::Vec2::splat(AREA))
    }

    /// Where a point of the unit cube shows on the screen.
    fn screen(&self, p: DVec3) -> egui::Pos2 {
        let v = egui::vec2(p.dot(self.right) as f32, -p.dot(self.up) as f32);
        self.center + v * HALF
    }

    /// Whether a face is turned towards the viewer, and how much: 0 edge on,
    /// 1 straight on.
    fn facing(&self, face: &Face) -> f64 {
        face.normal.dot(self.toward_viewer)
    }

    /// The part of a face under a screen position.
    pub fn part_at(&self, pos: egui::Pos2) -> Option<Part> {
        let q = (pos - self.center) / HALF;
        let q = DVec2::new(f64::from(q.x), f64::from(-q.y));
        let flat = |v: DVec3| DVec2::new(v.dot(self.right), v.dot(self.up));
        FACES.iter().enumerate().find_map(|(index, f)| {
            if self.facing(f) < 0.02 {
                return None;
            }
            // Solve q = n + u a + v b on the screen for the face
            // coordinates u and v.
            let (n, a, b) = (flat(f.normal), flat(f.a), flat(f.b));
            let det = a.perp_dot(b);
            if det.abs() < 1e-9 {
                return None;
            }
            let d = q - n;
            let (u, v) = (d.perp_dot(b) / det, a.perp_dot(d) / det);
            let part = |x: f64| SPLITS.windows(2).position(|w| x >= w[0] && x <= w[1]);
            Some(Part {
                face: index,
                i: part(u)?,
                j: part(v)?,
            })
        })
    }

    /// The screen corners of a part of a face.
    fn outline(&self, face: &Face, u: [f64; 2], v: [f64; 2]) -> Vec<egui::Pos2> {
        [(u[0], v[0]), (u[1], v[0]), (u[1], v[1]), (u[0], v[1])]
            .into_iter()
            .map(|(u, v)| self.screen(face.normal + face.a * u + face.b * v))
            .collect()
    }

    /// Where a face's middle shows, for tests.
    #[cfg(test)]
    pub fn face_center(&self, normal: DVec3) -> egui::Pos2 {
        self.screen(normal)
    }
}

/// Draws the view cube over a view and returns what the user did with it.
pub(super) fn ui(
    ui: &mut egui::Ui,
    view: egui::Rect,
    camera: &Camera,
    i18n: &I18n,
) -> Option<CubeAction> {
    let cube = Cube::new(view, camera);
    let id = ui.id().with("view-cube");
    let response = ui.interact(cube.rect(), id, egui::Sense::click_and_drag());
    let hovered = response.hover_pos().and_then(|p| cube.part_at(p));
    let mut action = None;

    let painter = ui.painter_at(view);
    let visuals = ui.visuals();
    let fill = visuals.widgets.inactive.weak_bg_fill;
    let highlight = visuals.selection.bg_fill;
    let stroke = egui::Stroke::new(1.0, visuals.widgets.noninteractive.fg_stroke.color);
    let text = visuals.strong_text_color();
    for (index, f) in FACES.iter().enumerate() {
        let facing = cube.facing(f);
        if facing < 0.02 {
            continue;
        }
        painter.add(egui::Shape::convex_polygon(
            cube.outline(f, [-1.0, 1.0], [-1.0, 1.0]),
            fill,
            stroke,
        ));
        if let Some(part) = hovered.filter(|p| p.face == index) {
            let u = [SPLITS[part.i], SPLITS[part.i + 1]];
            let v = [SPLITS[part.j], SPLITS[part.j + 1]];
            painter.add(egui::Shape::convex_polygon(
                cube.outline(f, u, v),
                highlight,
                egui::Stroke::NONE,
            ));
        }
        if facing > 0.35 {
            let size = 9.0 * (0.6 + 0.4 * facing as f32);
            painter.text(
                cube.screen(f.normal),
                egui::Align2::CENTER_CENTER,
                i18n.tr(f.label),
                egui::FontId::proportional(size),
                text,
            );
        }
    }

    let home_rect = egui::Rect::from_min_size(cube.rect().min, egui::Vec2::splat(18.0));
    let label = i18n.tr("view-cube-home");
    let home = ui
        .put(home_rect, egui::Button::new("🏠").small().frame(false))
        .on_hover_text(&label);
    home.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
    if home.clicked() {
        action = Some(CubeAction::Home);
    }

    if response.dragged_by(egui::PointerButton::Primary) {
        let d = response.drag_delta();
        action = Some(CubeAction::Orbit(DVec2::new(d.x.into(), d.y.into())));
    } else if response.clicked()
        && let Some(part) = hovered
    {
        action = Some(CubeAction::LookFrom(part.direction()));
    }
    response.context_menu(|ui| {
        if ui.button(i18n.tr("view-cube-go-home")).clicked() {
            action = Some(CubeAction::Home);
        }
        ui.separator();
        for (projection, key) in [
            (Projection::Perspective, "view-perspective"),
            (Projection::Orthographic, "view-orthographic"),
        ] {
            if ui
                .radio(camera.projection() == projection, i18n.tr(key))
                .clicked()
            {
                action = Some(CubeAction::SetProjection(projection));
                ui.close();
            }
        }
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(direction: DVec3) -> Cube {
        let mut camera = Camera::default();
        camera.look_from(direction);
        let view = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        Cube::new(view, &camera)
    }

    #[test]
    fn faces_are_right_handed() {
        for f in &FACES {
            assert_eq!(f.a.cross(f.b), f.normal, "{}", f.label);
        }
    }

    #[test]
    fn clicking_a_face_looks_from_it() {
        let cube = cube(crate::viewport::camera::HOME);
        for normal in [DVec3::Z, DVec3::NEG_Y, DVec3::X] {
            let part = cube.part_at(cube.face_center(normal)).expect("on the cube");
            assert_eq!(part.direction(), normal);
        }
        // The back, left and bottom are hidden from the home view.
        let back = cube.screen(DVec3::Y);
        let part = cube.part_at(back).map(|p| p.direction());
        assert_ne!(part, Some(DVec3::Y));
    }

    #[test]
    fn near_a_corner_looks_from_the_corner() {
        let cube = cube(DVec3::Z);
        // Seen from the top, north up: the top face's top-right corner is
        // the north-east corner of the cube.
        let corner = cube.screen(DVec3::new(0.9, 0.9, 1.0));
        let direction = cube.part_at(corner).unwrap().direction();
        assert!((direction - DVec3::ONE.normalize()).length() < 1e-12);
        let edge = cube.screen(DVec3::new(0.0, -0.9, 1.0));
        let direction = cube.part_at(edge).unwrap().direction();
        assert!((direction - DVec3::new(0.0, -1.0, 1.0).normalize()).length() < 1e-12);
    }

    #[test]
    fn outside_the_cube_is_nothing() {
        let cube = cube(DVec3::Z);
        assert_eq!(cube.part_at(cube.screen(DVec3::new(1.5, 0.0, 1.0))), None);
    }
}
