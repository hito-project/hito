//! The camera of a 3D view, and how navigation moves it. Plain maths, with
//! no egui or wgpu in it, so every move can be tested.
//!
//! The world is Z up, as in Revit and IFC. Screen positions are in egui
//! points, from the view's top-left corner.

use std::f64::consts::FRAC_PI_2;

use glam::dcamera::rh::proj::directx;
use glam::dcamera::rh::view::look_at_mat4;
use glam::{DMat4, DVec2, DVec3};
use serde::{Deserialize, Serialize};

/// The vertical field of view of a perspective view.
pub const FIELD_OF_VIEW: f64 = 50.0 * std::f64::consts::PI / 180.0;

/// How far orbiting turns the view for each point the mouse moves: a drag
/// across a 630-point-wide view turns it once round.
const ORBIT_PER_POINT: f64 = 0.01;

/// Zoom to fit leaves this much room around the model.
const FIT_MARGIN: f64 = 1.1;

/// The smallest and largest size of what a view shows, in metres: a
/// millimetre detail to a 1000 km site.
const HEIGHT_RANGE: (f64, f64) = (1e-3, 1e6);

/// How a view projects the model onto the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Projection {
    /// Parallel lines stay parallel. Revit's default 3D view is
    /// orthographic.
    #[default]
    Orthographic,
    Perspective,
}

/// A box around part of the model, aligned with the world axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: DVec3,
    pub max: DVec3,
}

impl Bounds {
    pub fn new(min: [f64; 3], max: [f64; 3]) -> Self {
        Self {
            min: min.into(),
            max: max.into(),
        }
    }

    pub fn union(self, other: Bounds) -> Bounds {
        Bounds {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    pub fn center(&self) -> DVec3 {
        (self.min + self.max) / 2.0
    }

    /// Half the diagonal: the radius of a sphere that holds the box.
    pub fn radius(&self) -> f64 {
        (self.max - self.min).length() / 2.0
    }

    pub fn corners(&self) -> [DVec3; 8] {
        let (a, b) = (self.min, self.max);
        [
            DVec3::new(a.x, a.y, a.z),
            DVec3::new(b.x, a.y, a.z),
            DVec3::new(a.x, b.y, a.z),
            DVec3::new(b.x, b.y, a.z),
            DVec3::new(a.x, a.y, b.z),
            DVec3::new(b.x, a.y, b.z),
            DVec3::new(a.x, b.y, b.z),
            DVec3::new(b.x, b.y, b.z),
        ]
    }
}

/// Where a 3D view looks from, and how much it shows.
///
/// The camera looks at its `target` from a direction given by two angles,
/// `yaw` round the vertical and `pitch` above the horizon, so the view never
/// rolls: up on the screen is always up in the world, or north when looking
/// straight down, as in Revit.
///
/// `height` is how many metres the view shows from bottom to top at the
/// target. A perspective camera stands at the distance that shows that
/// much, so switching projections keeps the model the same size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    target: DVec3,
    yaw: f64,
    pitch: f64,
    height: f64,
    projection: Projection,
}

/// The direction of the home view: from the front, the right and above,
/// the south-east isometric of Revit's default 3D view.
pub const HOME: DVec3 = DVec3::new(1.0, -1.0, 1.0);

impl Default for Camera {
    /// The home view of a 30 m area round the origin.
    fn default() -> Self {
        let mut camera = Self {
            target: DVec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            height: 30.0,
            projection: Projection::default(),
        };
        camera.look_from(HOME);
        camera
    }
}

impl Camera {
    pub fn target(&self) -> DVec3 {
        self.target
    }

    pub fn height(&self) -> f64 {
        self.height
    }

    pub fn projection(&self) -> Projection {
        self.projection
    }

    /// Switches projection, keeping the model the same size on the screen
    /// at the target.
    pub fn set_projection(&mut self, projection: Projection) {
        self.projection = projection;
    }

    /// The direction from the target to the eye.
    pub fn backward(&self) -> DVec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        DVec3::new(cp * cy, cp * sy, sp)
    }

    /// The screen's right, in the world.
    pub fn right(&self) -> DVec3 {
        let (sy, cy) = self.yaw.sin_cos();
        DVec3::new(-sy, cy, 0.0)
    }

    /// The screen's up, in the world.
    pub fn up(&self) -> DVec3 {
        self.backward().cross(self.right())
    }

    /// How far the eye is from the target. An orthographic camera has no
    /// real eye; it gets the same one as a perspective camera, so depth
    /// works the same way in both.
    pub fn distance(&self) -> f64 {
        self.height / 2.0 / (FIELD_OF_VIEW / 2.0).tan()
    }

    /// The same view with its target at height `z`: what it shows from
    /// straight above doesn't change, but its eye is above anything lower
    /// than `z`, for picking in a plan view.
    pub fn lifted(mut self, z: f64) -> Self {
        self.target.z = z;
        self
    }

    pub fn eye(&self) -> DVec3 {
        self.target + self.backward() * self.distance()
    }

    /// Turns the camera to look from `direction` (towards the target),
    /// keeping the target. Looking straight down or up, north is up on the
    /// screen.
    pub fn look_from(&mut self, direction: DVec3) {
        let d = direction.normalize_or(HOME.normalize());
        self.pitch = d.z.clamp(-1.0, 1.0).asin();
        self.yaw = if d.truncate().length() < 1e-9 {
            -FRAC_PI_2
        } else {
            d.y.atan2(d.x)
        };
    }

    /// Orbits round `pivot` by a mouse movement of `delta` points: moving
    /// right turns the model to the right, moving down tips its top
    /// towards the viewer. The pivot stays where it is on the screen.
    pub fn orbit(&mut self, pivot: DVec3, delta: DVec2) {
        let offset = self.target - pivot;
        let local = DVec3::new(
            offset.dot(self.right()),
            offset.dot(self.up()),
            offset.dot(self.backward()),
        );
        self.yaw -= delta.x * ORBIT_PER_POINT;
        self.pitch = (self.pitch + delta.y * ORBIT_PER_POINT).clamp(-FRAC_PI_2, FRAC_PI_2);
        self.target =
            pivot + self.right() * local.x + self.up() * local.y + self.backward() * local.z;
    }

    /// Pans by a mouse movement of `delta` points in a view `size` points
    /// big. What is at the target's depth follows the mouse.
    pub fn pan(&mut self, delta: DVec2, size: DVec2) {
        let per_point = self.height / size.y.max(1.0);
        self.target += (self.up() * delta.y - self.right() * delta.x) * per_point;
    }

    /// Zooms in by `factor` (out if it's less than 1) towards the point
    /// under `cursor`, which stays under the cursor.
    pub fn zoom(&mut self, cursor: DVec2, size: DVec2, factor: f64) {
        let factor = factor.clamp(self.height / HEIGHT_RANGE.1, self.height / HEIGHT_RANGE.0);
        let fixed = self.point_at(cursor, size);
        self.height /= factor;
        self.target = fixed + (self.target - fixed) / factor;
    }

    /// Zooms so all of `bounds` shows in a view `size` points big, keeping
    /// the direction the camera looks from.
    pub fn fit(&mut self, bounds: Bounds, size: DVec2) {
        let aspect = size.x.max(1.0) / size.y.max(1.0);
        let radius = bounds.radius().max(HEIGHT_RANGE.0);
        // The sphere round the box fits the narrower side of the view. A
        // perspective camera needs to stand a little further back to see a
        // sphere's edges.
        let half = (FIELD_OF_VIEW / 2.0).tan();
        let sphere = match self.projection {
            Projection::Orthographic => radius,
            Projection::Perspective => radius / (FIELD_OF_VIEW / 2.0).sin() * half,
        };
        self.target = bounds.center();
        self.height = 2.0 * sphere * FIT_MARGIN * (1.0 / aspect).max(1.0);
    }

    /// The point at the target's depth under a screen position.
    pub fn point_at(&self, screen: DVec2, size: DVec2) -> DVec3 {
        let ndc = to_ndc(screen, size);
        let half = DVec2::new(self.height / 2.0 * aspect(size), self.height / 2.0);
        self.target + self.right() * (ndc.x * half.x) + self.up() * (ndc.y * half.y)
    }

    /// The line of sight through a screen position: where it starts and its
    /// direction, which is a unit vector.
    pub fn ray(&self, screen: DVec2, size: DVec2) -> (DVec3, DVec3) {
        let at = self.point_at(screen, size);
        match self.projection {
            Projection::Orthographic => {
                let forward = -self.backward();
                (at - forward * self.distance(), forward)
            }
            Projection::Perspective => (self.eye(), (at - self.eye()).normalize()),
        }
    }

    /// Where a point of the world shows on the screen, `None` if it's
    /// behind a perspective camera.
    pub fn project(&self, point: DVec3, size: DVec2) -> Option<DVec2> {
        let clip = self.view_projection(size, DVec3::ZERO, None) * point.extend(1.0);
        (clip.w > 0.0).then(|| {
            let ndc = clip.truncate().truncate() / clip.w;
            DVec2::new((ndc.x + 1.0) / 2.0 * size.x, (1.0 - ndc.y) / 2.0 * size.y)
        })
    }

    /// The matrix from world coordinates, taken relative to `origin`, to
    /// wgpu's clip space, for a view `size` points big. The near and far
    /// planes hug `scene` when given, for the most depth precision.
    pub fn view_projection(&self, size: DVec2, origin: DVec3, scene: Option<Bounds>) -> DMat4 {
        let eye = self.eye() - origin;
        let view = look_at_mat4(eye, self.target - origin, self.up());
        let distance = self.distance();
        let (near, far) = match scene {
            Some(scene) => {
                let depth = (scene.center() - self.eye()).dot(-self.backward());
                let radius = scene.radius().max(HEIGHT_RANGE.0);
                (depth - radius * 1.01, depth + radius * 1.01)
            }
            None => (distance / 100.0, distance * 100.0),
        };
        let aspect = aspect(size);
        let projection = match self.projection {
            Projection::Orthographic => {
                let (w, h) = (self.height / 2.0 * aspect, self.height / 2.0);
                directx::orthographic(-w, w, -h, h, near, far.max(near + HEIGHT_RANGE.0))
            }
            Projection::Perspective => {
                let far = far.max(distance * 1e-3);
                let near = near.max(far * 1e-5);
                directx::perspective(FIELD_OF_VIEW, aspect, near, far.max(near * 2.0))
            }
        };
        projection * view
    }
}

fn aspect(size: DVec2) -> f64 {
    size.x.max(1.0) / size.y.max(1.0)
}

/// A screen position in normalised device coordinates: -1 to 1, y up.
fn to_ndc(screen: DVec2, size: DVec2) -> DVec2 {
    DVec2::new(
        2.0 * screen.x / size.x.max(1.0) - 1.0,
        1.0 - 2.0 * screen.y / size.y.max(1.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: DVec2 = DVec2::new(800.0, 600.0);

    fn close(a: DVec2, b: DVec2) -> bool {
        (a - b).length() < 1e-6
    }

    fn cameras() -> [Camera; 2] {
        let ortho = Camera {
            target: DVec3::new(5.0, 3.0, 1.0),
            ..Camera::default()
        };
        let mut perspective = ortho;
        perspective.set_projection(Projection::Perspective);
        [ortho, perspective]
    }

    #[test]
    fn home_looks_from_the_front_right_and_above() {
        let camera = Camera::default();
        let eye = camera.eye() - camera.target();
        assert!(eye.x > 0.0 && eye.y < 0.0 && eye.z > 0.0, "{eye}");
        // Up on the screen leans towards world up: the view doesn't roll.
        assert!(camera.up().z > 0.0);
        assert!(camera.right().z.abs() < 1e-12);
    }

    #[test]
    fn the_top_view_has_north_up_and_east_right() {
        let mut camera = Camera::default();
        camera.look_from(DVec3::Z);
        assert!((camera.up() - DVec3::Y).length() < 1e-12);
        assert!((camera.right() - DVec3::X).length() < 1e-12);
        camera.look_from(-DVec3::Y);
        assert!((camera.up() - DVec3::Z).length() < 1e-12, "front view");
        assert!((camera.right() - DVec3::X).length() < 1e-12);
    }

    #[test]
    fn the_target_shows_in_the_middle() {
        for camera in cameras() {
            let middle = camera.project(camera.target(), SIZE).unwrap();
            assert!(close(middle, SIZE / 2.0), "{middle}");
        }
    }

    #[test]
    fn zooming_keeps_the_point_under_the_cursor() {
        for mut camera in cameras() {
            let cursor = DVec2::new(600.0, 150.0);
            let point = camera.point_at(cursor, SIZE);
            let height = camera.height();
            camera.zoom(cursor, SIZE, 1.5);
            assert!((camera.height() - height / 1.5).abs() < 1e-9);
            let after = camera.project(point, SIZE).unwrap();
            assert!(close(after, cursor), "{after} {cursor}");
            camera.zoom(cursor, SIZE, 0.5);
            assert!(close(camera.project(point, SIZE).unwrap(), cursor));
        }
    }

    #[test]
    fn zoom_stops_at_a_millimetre() {
        let mut camera = Camera::default();
        for _ in 0..200 {
            camera.zoom(SIZE / 2.0, SIZE, 2.0);
        }
        assert!((camera.height() - HEIGHT_RANGE.0).abs() < 1e-12);
    }

    #[test]
    fn panning_moves_the_model_with_the_mouse() {
        for mut camera in cameras() {
            let start = DVec2::new(300.0, 200.0);
            let point = camera.point_at(start, SIZE);
            let delta = DVec2::new(40.0, -25.0);
            camera.pan(delta, SIZE);
            let after = camera.project(point, SIZE).unwrap();
            assert!(close(after, start + delta), "{after}");
        }
    }

    #[test]
    fn orbiting_keeps_the_pivot_in_place() {
        for mut camera in cameras() {
            let pivot = DVec3::new(-2.0, 7.0, 3.0);
            let before = camera.project(pivot, SIZE).unwrap();
            camera.orbit(pivot, DVec2::new(120.0, 45.0));
            let after = camera.project(pivot, SIZE).unwrap();
            assert!(close(before, after), "{before} {after}");
        }
    }

    #[test]
    fn dragging_right_turns_the_model_right_and_down_shows_its_top() {
        let mut camera = Camera::default();
        camera.look_from(-DVec3::Y);
        camera.orbit(camera.target(), DVec2::new(50.0, 0.0));
        // Turning the model to the right brings its left side into view.
        assert!(camera.backward().x < 0.0, "{}", camera.backward());
        camera.orbit(camera.target(), DVec2::new(0.0, 50.0));
        assert!(camera.backward().z > 0.0, "looking down on it");
    }

    #[test]
    fn orbiting_stops_at_straight_down() {
        let mut camera = Camera::default();
        camera.orbit(camera.target(), DVec2::new(0.0, 10_000.0));
        assert!((camera.backward() - DVec3::Z).length() < 1e-9);
        assert!(camera.up().is_finite() && camera.right().is_finite());
    }

    #[test]
    fn fit_shows_the_whole_box() {
        let bounds = Bounds::new([100.0, 200.0, 0.0], [130.0, 210.0, 9.0]);
        for mut camera in cameras() {
            for size in [SIZE, DVec2::new(300.0, 900.0)] {
                camera.fit(bounds, size);
                for corner in bounds.corners() {
                    let p = camera.project(corner, size).unwrap();
                    let inside = p.x > 0.0 && p.y > 0.0 && p.x < size.x && p.y < size.y;
                    assert!(inside, "{corner} at {p} in {size}");
                }
                assert!(close(
                    camera.project(bounds.center(), size).unwrap(),
                    size / 2.0
                ));
            }
        }
    }

    #[test]
    fn switching_projection_keeps_the_size_at_the_target() {
        let [ortho, mut perspective] = cameras();
        let edge = ortho.point_at(DVec2::new(0.0, 300.0), SIZE);
        perspective.set_projection(Projection::Perspective);
        assert!(close(
            perspective.project(edge, SIZE).unwrap(),
            DVec2::new(0.0, 300.0)
        ));
    }

    #[test]
    fn rays_go_through_the_point_under_the_cursor() {
        for camera in cameras() {
            let cursor = DVec2::new(100.0, 500.0);
            let (start, direction) = camera.ray(cursor, SIZE);
            assert!((direction.length() - 1.0).abs() < 1e-12);
            let point = camera.point_at(cursor, SIZE);
            let along = (point - start).normalize();
            assert!((along - direction).length() < 1e-9);
        }
    }

    #[test]
    fn depth_hugs_the_scene_far_from_the_origin() {
        let bounds = Bounds::new(
            [5_500_000.0, 6_100_000.0, 0.0],
            [5_500_020.0, 6_100_010.0, 6.0],
        );
        for mut camera in cameras() {
            camera.fit(bounds, SIZE);
            let origin = bounds.min;
            let matrix = camera.view_projection(SIZE, origin, Some(bounds));
            for corner in bounds.corners() {
                let clip = matrix * (corner - origin).extend(1.0);
                let depth = clip.z / clip.w;
                assert!((0.0..=1.0).contains(&depth), "{corner}: {depth}");
            }
        }
    }
}
