//! The 3D views. Each renders off-screen with wgpu into its own texture,
//! which egui shows as an image in a view tab
//! ([ADR 0015](https://github.com/hito-project/hito/blob/main/docs/decisions/0015-ui-toolkit.md)).
//!
//! The [`Scene`] holds every element's mesh, uploaded once and shared by
//! all views. Each [`Viewport`] has its own [`Camera`], moved with the mouse
//! as the [`Navigation`] setting says, and a view cube in its corner.

pub mod camera;
mod cut;
pub mod navigation;
mod pick;
pub mod plan;
mod scene;
mod view_cube;

use eframe::egui;
use eframe::egui_wgpu::{RenderState, wgpu};
use glam::{DVec2, DVec3, Vec4};
use hito_core::ElementId;

pub use camera::{Bounds, Camera, Projection};
pub use navigation::Navigation;
pub use pick::BoxKind;
pub use plan::{Plan, PlanGrid, PlanOutput};
pub use scene::Scene;

use crate::i18n::I18n;
use crate::selection::Mode;
use navigation::Motion;
use scene::{COLOR_FORMAT, DEPTH_FORMAT, SAMPLES};
use view_cube::CubeAction;

/// The background of a 3D view in the light theme: white, as in Revit.
pub const BACKGROUND: [u8; 3] = [255, 255, 255];
/// The background of a 3D view in the dark theme.
pub const BACKGROUND_DARK: [u8; 3] = [42, 45, 50];

/// How much one point of mouse-wheel scrolling zooms: a wheel notch (50
/// points in egui) zooms by about 1.2, as in Revit.
const ZOOM_PER_POINT: f64 = 0.0036;

/// One 3D view: its camera and its renderer. It renders again only when
/// what it shows changes: its size, the theme, the camera or the scene.
#[derive(Default)]
pub struct Viewport {
    camera: Camera,
    /// Whether the camera has been fitted to the model, which a view does
    /// the first time it shows one.
    framed: bool,
    /// The view's size in points when it last showed.
    size: DVec2,
    /// The elements under the pointer, nearest first, where it was when
    /// they were found, and which of them Tab has reached.
    under: Vec<ElementId>,
    under_at: Option<(egui::Pos2, Camera, u64)>,
    cycle: usize,
    /// Whether this view set the scene's pre-highlight, so it clears it
    /// when the pointer leaves.
    prehighlighting: bool,
    /// Whether the pointer was over the view in the last frame.
    hovered: bool,
    /// Where the selection box being dragged started.
    box_start: Option<egui::Pos2>,
    target: Option<Target>,
    /// Whether it's a plan view (#183), which looks straight down.
    is_plan: bool,
    /// Where the view was in the window when it last showed.
    rect: Option<egui::Rect>,
    /// What zooming to fit a plan view shows, as of its last frame.
    plan_bounds: Option<Bounds>,
    /// What a plan view draws of the elements.
    drawing: plan::Drawing,
    /// How many times it has rendered, so tests can check that it did.
    pub renders: u64,
}

/// What a click or a selection box picked in a view, and how it changes
/// the selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked {
    pub ids: Vec<ElementId>,
    pub mode: Mode,
}

/// Everything a rendered image depends on.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Shows {
    size: [u32; 2],
    background: [u8; 3],
    camera: Camera,
    scene: u64,
}

struct Target {
    shows: Shows,
    /// The resolved image egui shows.
    texture: wgpu::Texture,
    id: egui::TextureId,
    multisampled: wgpu::TextureView,
    depth: wgpu::TextureView,
    uniforms: wgpu::Buffer,
    group: wgpu::BindGroup,
}

/// What a click finds under the cursor, from a screen position in a view
/// of a size, nearest first.
type Under<'a> = &'a dyn Fn(&Scene, &Camera, DVec2, DVec2) -> Vec<ElementId>;
/// What a box between two screen corners takes.
type InBox<'a> = &'a dyn Fn(&Scene, &Camera, [DVec2; 2], BoxKind, DVec2) -> Vec<ElementId>;

/// How a view finds what a click or a box picks: the 3D views from the
/// meshes, plan views from what they draw.
struct Picker<'a> {
    /// Changes whenever what they find could.
    revision: u64,
    under: Under<'a>,
    in_box: InBox<'a>,
}

/// The shader's `View`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ViewUniforms {
    view_projection: [[f32; 4]; 4],
    light: [f32; 4],
}

impl Viewport {
    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    pub fn camera_mut(&mut self) -> &mut Camera {
        &mut self.camera
    }

    /// Takes Tab for cycling through overlapping elements while the pointer
    /// is over the view and nothing has the focus, as in Revit, so egui
    /// doesn't move the focus to the first widget instead. Call it before
    /// any widget shows in the frame. Returns whether Tab was pressed.
    pub fn claim_tab(&self, ctx: &egui::Context) -> bool {
        let tab = self.hovered
            && ctx.memory(|m| m.focused().is_none())
            && ctx.input(|i| i.key_pressed(egui::Key::Tab) && !i.modifiers.any());
        if tab {
            ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
        }
        tab
    }

    /// Zooms so the whole model shows, as Revit's Zoom to Fit. An empty
    /// model shows the area round the origin.
    pub fn zoom_to_fit(&mut self, scene: &Scene) {
        if self.is_plan {
            if let Some(bounds) = self.plan_bounds {
                self.camera.fit(bounds, self.size);
            }
            return;
        }
        let area = Bounds::new([-10.0, -10.0, 0.0], [10.0, 10.0, 3.0]);
        self.camera.fit(scene.bounds().unwrap_or(area), self.size);
    }

    /// Fills `ui` with the view of `scene`, and moves the camera as the
    /// mouse says. Without a wgpu device (in tests, or with another
    /// renderer) it shows a message in place of the model.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        scene: &mut Scene,
        navigation: Navigation,
        render_state: Option<&RenderState>,
        i18n: &I18n,
    ) -> Option<Picked> {
        let tab = self.claim_tab(ui.ctx());
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        self.hovered = response.hover_pos().is_some();
        self.rect = Some(rect);
        // The image goes under the view cube, which draws next.
        let image = ui.painter().add(egui::Shape::Noop);
        self.size = DVec2::new(rect.width().into(), rect.height().into());
        if !self.framed && !scene.is_empty() {
            self.zoom_to_fit(scene);
            self.framed = true;
        }
        self.navigate(ui, &response, scene, navigation);
        if let Some(action) = view_cube::ui(ui, rect, &self.camera, i18n) {
            self.cube(action, scene);
        }
        let picker = Picker {
            revision: scene.geometry_revision(),
            under: &pick::under,
            in_box: &pick::in_box,
        };
        let picked = self.select(ui, &response, scene, tab, &picker);

        let Some(rs) = render_state else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                i18n.tr("viewport-no-gpu"),
                egui::FontId::default(),
                ui.visuals().weak_text_color(),
            );
            return picked;
        };
        scene.prepare(rs);
        let ppp = ui.pixels_per_point();
        let shows = Shows {
            size: [
                (rect.width() * ppp).max(1.0) as u32,
                (rect.height() * ppp).max(1.0) as u32,
            ],
            background: if ui.visuals().dark_mode {
                BACKGROUND_DARK
            } else {
                BACKGROUND
            },
            camera: self.camera,
            scene: scene.revision(),
        };
        let id = self.render(rs, scene, shows);
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        let shape = egui::Shape::image(id, rect, uv, egui::Color32::WHITE);
        ui.painter().set(image, shape);
        picked
    }

    /// Selects as Revit does: a click picks the element under the cursor,
    /// or the one Tab has cycled to among those overlapping there; a drag
    /// to the right draws a window box, a drag to the left a crossing box.
    /// Ctrl adds to the selection and Shift removes from it.
    ///
    /// `picker` finds what a click and a box pick.
    fn select(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        scene: &mut Scene,
        tab: bool,
        picker: &Picker<'_>,
    ) -> Option<Picked> {
        let origin = response.rect.min;
        let local = |p: egui::Pos2| DVec2::new((p.x - origin.x).into(), (p.y - origin.y).into());
        let boxing = response.dragged_by(egui::PointerButton::Primary)
            || response.drag_stopped_by(egui::PointerButton::Primary);

        match response.hover_pos().filter(|_| !boxing) {
            Some(pointer) => {
                let at = Some((pointer, self.camera, picker.revision));
                if self.under_at != at {
                    self.under = (picker.under)(scene, &self.camera, local(pointer), self.size);
                    self.under_at = at;
                    self.cycle = 0;
                }
                if tab && self.under.len() > 1 {
                    ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
                    self.cycle = (self.cycle + 1) % self.under.len();
                }
                scene.set_prehighlight(self.under.get(self.cycle).copied());
                self.prehighlighting = true;
            }
            None => {
                self.under_at = None;
                if self.prehighlighting {
                    scene.set_prehighlight(None);
                    self.prehighlighting = false;
                }
            }
        }

        let mode = ui.input(|i| Mode::from_keys(i.modifiers.command, i.modifiers.shift));
        if response.clicked_by(egui::PointerButton::Primary) {
            let ids = self.under.get(self.cycle).copied().into_iter().collect();
            return Some(Picked { ids, mode });
        }
        if !boxing {
            return None;
        }
        if response.drag_started_by(egui::PointerButton::Primary) {
            self.box_start = ui.input(|i| i.pointer.press_origin());
        }
        let start = self.box_start?;
        let end = response.interact_pointer_pos().or(response.hover_pos())?;
        let corners = [local(start), local(end)];
        let kind = BoxKind::of_drag(corners[0], corners[1]);
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.box_start = None;
            let ids = (picker.in_box)(scene, &self.camera, corners, kind, self.size);
            return Some(Picked { ids, mode });
        }
        // Revit draws a window box with a solid line and a crossing box
        // with a dashed one.
        let painter = ui.painter_at(response.rect);
        let stroke = egui::Stroke::new(1.0, ui.visuals().strong_text_color());
        let rect = egui::Rect::from_two_pos(start, end);
        match kind {
            BoxKind::Window => {
                painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
            }
            BoxKind::Crossing => {
                let c = [
                    rect.left_top(),
                    rect.right_top(),
                    rect.right_bottom(),
                    rect.left_bottom(),
                    rect.left_top(),
                ];
                painter.extend(egui::Shape::dashed_line(&c, stroke, 4.0, 3.0));
            }
        }
        None
    }

    /// Zooms with the wheel, orbits and pans with the wheel held down, and
    /// zooms to fit on a double click of the wheel, as in Revit.
    fn navigate(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        scene: &Scene,
        navigation: Navigation,
    ) {
        let size = self.size;
        if let Some(pointer) = response.hover_pos() {
            let (scroll, pinch) = ui.input(|i| {
                // egui turns the wheel sideways while Shift is held.
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
            let delta = DVec2::new(d.x.into(), d.y.into());
            let shift = ui.input(|i| i.modifiers.shift);
            match navigation.wheel_drag(shift) {
                Motion::Orbit => self.camera.orbit(pivot(scene, &self.camera), delta),
                Motion::Pan => self.camera.pan(delta, size),
            }
        }
        if response.double_clicked_by(egui::PointerButton::Middle) {
            self.zoom_to_fit(scene);
        }
    }

    fn cube(&mut self, action: CubeAction, scene: &Scene) {
        match action {
            CubeAction::LookFrom(direction) => {
                self.camera.look_from(direction);
                self.zoom_to_fit(scene);
            }
            CubeAction::Home => {
                self.camera.look_from(camera::HOME);
                self.zoom_to_fit(scene);
            }
            CubeAction::Orbit(delta) => {
                let pivot = pivot(scene, &self.camera);
                self.camera.orbit(pivot, delta);
            }
            CubeAction::SetProjection(projection) => self.camera.set_projection(projection),
        }
    }

    fn render(&mut self, rs: &RenderState, scene: &Scene, shows: Shows) -> egui::TextureId {
        if let Some(target) = &self.target
            && target.shows == shows
        {
            return target.id;
        }
        let Some(gpu) = scene.gpu() else {
            unreachable!("the scene is prepared before views render");
        };
        if self
            .target
            .as_ref()
            .is_none_or(|t| t.shows.size != shows.size)
        {
            self.resize(rs, &gpu.view_layout, shows);
        }
        let target = self.target.as_mut().expect("made above");
        target.shows = shows;

        let camera = &shows.camera;
        let size = DVec2::new(shows.size[0].into(), shows.size[1].into());
        let matrix = camera.view_projection(size, scene.origin(), scene.bounds());
        // From over the viewer's left shoulder.
        let light = (camera.backward() * 0.8 + camera.up() * 0.5 - camera.right() * 0.3)
            .normalize()
            .as_vec3();
        let uniforms = ViewUniforms {
            view_projection: matrix.as_mat4().to_cols_array_2d(),
            light: Vec4::from((light, 0.0)).into(),
        };
        rs.queue
            .write_buffer(&target.uniforms, 0, bytemuck::bytes_of(&uniforms));

        let [r, g, b] = shows.background.map(|c| f64::from(c) / 255.0);
        let resolved = target.texture.create_view(&Default::default());
        let mut encoder = rs.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("3D view"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.multisampled,
                    depth_slice: None,
                    resolve_target: Some(&resolved),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &target.group, &[]);
            gpu.draw(&mut pass, false);
            gpu.draw(&mut pass, true);
        }
        rs.queue.submit([encoder.finish()]);
        self.renders += 1;
        target.id
    }

    /// Makes the textures for a new size, keeping the egui texture's ID.
    fn resize(&mut self, rs: &RenderState, layout: &wgpu::BindGroupLayout, shows: Shows) {
        let device = &rs.device;
        let texture = |label, format, samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: shows.size[0],
                    height: shows.size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let resolved = texture(
            "3D view",
            COLOR_FORMAT,
            1,
            attachment | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let multisampled = texture("3D view samples", COLOR_FORMAT, SAMPLES, attachment)
            .create_view(&Default::default());
        let depth = texture("3D view depth", DEPTH_FORMAT, SAMPLES, attachment)
            .create_view(&Default::default());
        let view = resolved.create_view(&Default::default());
        let id = {
            let mut renderer = rs.renderer.write();
            match &self.target {
                Some(old) => {
                    renderer.update_egui_texture_from_wgpu_texture(
                        device,
                        &view,
                        wgpu::FilterMode::Linear,
                        old.id,
                    );
                    old.id
                }
                None => renderer.register_native_texture(device, &view, wgpu::FilterMode::Linear),
            }
        };
        let (uniforms, group) = match self.target.take() {
            Some(old) => {
                old.texture.destroy();
                (old.uniforms, old.group)
            }
            None => {
                let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("3D view"),
                    size: size_of::<ViewUniforms>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("3D view"),
                    layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniforms.as_entire_binding(),
                    }],
                });
                (uniforms, group)
            }
        };
        self.target = Some(Target {
            shows,
            texture: resolved,
            id,
            multisampled,
            depth,
            uniforms,
            group,
        });
    }

    /// Frees the texture. Call it when the view's tab closes.
    pub fn free(&mut self, rs: Option<&RenderState>) {
        if let (Some(target), Some(rs)) = (self.target.take(), rs) {
            rs.renderer.write().free_texture(&target.id);
            target.texture.destroy();
        }
    }
}

/// What orbiting turns round, as in Revit: the middle of the selection,
/// or of the model when nothing is selected, or the camera's target when
/// the view is empty.
fn pivot(scene: &Scene, camera: &Camera) -> DVec3 {
    scene
        .selection_bounds()
        .or_else(|| scene.bounds())
        .map_or(camera.target(), |bounds| bounds.center())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Locale;
    use crate::selection::Selection;
    use egui::accesskit::Role;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use hito_core::ElementId;

    /// A view on its own, with the scene it shows.
    struct View {
        viewport: Viewport,
        scene: Scene,
        navigation: Navigation,
        i18n: I18n,
        render_state: Option<RenderState>,
        rect: egui::Rect,
        selection: Selection,
        /// The columns: at the origin, 6 m to its right, and 3 m behind it.
        ids: [ElementId; 3],
    }

    /// Three columns: two 6 m apart, and one 3 m behind the first.
    fn view(render_state: Option<RenderState>) -> Harness<'static, View> {
        let mut scene = Scene::default();
        let ids = [(); 3].map(|_| ElementId::generate());
        scene.insert(ids[0], scene::tests::column(0.0, 0.3));
        scene.insert(ids[1], scene::tests::column(6.0, 0.3));
        let behind = scene::tests::column(0.0, 0.3);
        let behind = hito_geometry::Mesh {
            positions: behind
                .positions
                .iter()
                .map(|p| [p[0], p[1] + 3.0, p[2]])
                .collect(),
            triangles: behind.triangles.clone(),
        };
        scene.insert(ids[2], std::sync::Arc::new(behind));
        let view = View {
            viewport: Viewport::default(),
            scene,
            navigation: Navigation::default(),
            i18n: I18n::new(Locale::En),
            render_state,
            rect: egui::Rect::NOTHING,
            selection: Selection::default(),
            ids,
        };
        let mut builder = Harness::builder()
            .with_size(egui::vec2(800.0, 600.0))
            .with_theme(egui::Theme::Light)
            .with_step_dt(0.05);
        if let Some(rs) = &view.render_state {
            let renderer = egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs.clone());
            builder = builder.renderer(renderer);
        }
        let mut h = builder.build_ui_state(
            |ui, v: &mut View| {
                v.rect = ui.available_rect_before_wrap();
                let rs = v.render_state.as_ref();
                v.scene.set_selected(v.selection.ids());
                if let Some(picked) = v.viewport.ui(ui, &mut v.scene, v.navigation, rs, &v.i18n) {
                    v.selection.apply(picked.ids, picked.mode);
                }
            },
            view,
        );
        h.run_steps(2);
        h
    }

    fn camera(h: &Harness<'_, View>) -> Camera {
        *h.state().viewport.camera()
    }

    fn button(h: &Harness<'_, View>, button: egui::PointerButton, pos: egui::Pos2, pressed: bool) {
        h.event(egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }

    /// Drags with the mouse wheel held down, from the middle of the view.
    fn wheel_drag(h: &mut Harness<'_, View>, delta: egui::Vec2, modifiers: egui::Modifiers) {
        let start = h.state().rect.center();
        h.event(egui::Event::ModifiersChanged(modifiers));
        h.hover_at(start);
        h.run_steps(1);
        button(h, egui::PointerButton::Middle, start, true);
        h.run_steps(1);
        for step in 1..=4 {
            h.hover_at(start + delta * step as f32 / 4.0);
            h.run_steps(1);
        }
        button(h, egui::PointerButton::Middle, start + delta, false);
        h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        h.run_steps(2);
    }

    #[test]
    fn a_new_view_shows_the_whole_model_from_home() {
        let h = view(None);
        let camera = camera(&h);
        let bounds = h.state().scene.bounds().unwrap();
        assert!((camera.target() - bounds.center()).length() < 1e-9);
        assert!((camera.backward() - camera::HOME.normalize()).length() < 1e-9);
    }

    #[test]
    fn the_wheel_orbits_by_default_and_shift_pans() {
        let mut h = view(None);
        let before = camera(&h);
        wheel_drag(&mut h, egui::vec2(100.0, 0.0), egui::Modifiers::NONE);
        let orbited = camera(&h);
        assert_ne!(orbited.backward(), before.backward(), "orbited");
        assert!((orbited.target() - before.target()).length() < 1e-9);

        wheel_drag(&mut h, egui::vec2(50.0, 30.0), egui::Modifiers::SHIFT);
        let panned = camera(&h);
        assert_eq!(panned.backward(), orbited.backward(), "panned");
        assert_ne!(panned.target(), orbited.target());
    }

    #[test]
    fn revit_navigation_pans_with_the_wheel_and_orbits_with_shift() {
        let mut h = view(None);
        h.state_mut().navigation = Navigation::Revit;
        let before = camera(&h);
        wheel_drag(&mut h, egui::vec2(100.0, 0.0), egui::Modifiers::NONE);
        let panned = camera(&h);
        assert_eq!(panned.backward(), before.backward());
        assert_ne!(panned.target(), before.target());
        wheel_drag(&mut h, egui::vec2(100.0, 0.0), egui::Modifiers::SHIFT);
        assert_ne!(camera(&h).backward(), panned.backward(), "orbited");
    }

    #[test]
    fn the_wheel_zooms_to_the_cursor() {
        let mut h = view(None);
        let rect = h.state().rect;
        let cursor = rect.min + egui::vec2(200.0, 150.0);
        let size = DVec2::new(rect.width().into(), rect.height().into());
        let at = DVec2::new(200.0, 150.0);
        let point = camera(&h).point_at(at, size);
        let height = camera(&h).height();
        h.hover_at(cursor);
        h.run_steps(1);
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 100.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(20);
        let camera = camera(&h);
        assert!(camera.height() < height * 0.9, "zoomed in");
        let after = camera.project(point, size).unwrap();
        assert!((after - at).length() < 1e-6, "{after}");
    }

    #[test]
    fn a_double_click_on_the_wheel_zooms_to_fit() {
        let mut h = view(None);
        let fitted = camera(&h);
        wheel_drag(&mut h, egui::vec2(80.0, 40.0), egui::Modifiers::SHIFT);
        assert_ne!(camera(&h), fitted);
        let middle = h.state().rect.center();
        for _ in 0..2 {
            button(&h, egui::PointerButton::Middle, middle, true);
            button(&h, egui::PointerButton::Middle, middle, false);
            h.run_steps(1);
        }
        h.run_steps(1);
        assert!((camera(&h).target() - fitted.target()).length() < 1e-9);
    }

    #[test]
    fn the_view_cube_looks_from_a_face_and_goes_home() {
        let mut h = view(None);
        let cube = view_cube::Cube::new(h.state().rect, &camera(&h));
        let top = cube.face_center(DVec3::Z);
        h.hover_at(top);
        h.run_steps(1);
        button(&h, egui::PointerButton::Primary, top, true);
        button(&h, egui::PointerButton::Primary, top, false);
        h.run_steps(2);
        let camera = camera(&h);
        assert!((camera.backward() - DVec3::Z).length() < 1e-12, "top view");
        assert!((camera.up() - DVec3::Y).length() < 1e-12, "north up");

        h.get_by_role_and_label(Role::Button, "Home").click();
        h.run_steps(2);
        let home = camera::HOME.normalize();
        assert!((self::camera(&h).backward() - home).length() < 1e-12);
    }

    #[test]
    fn the_view_cube_menu_switches_to_perspective() {
        let mut h = view(None);
        assert_eq!(camera(&h).projection(), Projection::Orthographic);
        let cube = view_cube::Cube::new(h.state().rect, &camera(&h));
        let front = cube.face_center(DVec3::NEG_Y);
        h.hover_at(front);
        h.run_steps(1);
        button(&h, egui::PointerButton::Secondary, front, true);
        button(&h, egui::PointerButton::Secondary, front, false);
        h.run_steps(2);
        h.get_by_role_and_label(Role::RadioButton, "Perspective")
            .click();
        h.run_steps(2);
        assert_eq!(camera(&h).projection(), Projection::Perspective);
    }

    /// Turns the view to look from the front, fitted to the model.
    fn from_the_front(h: &mut Harness<'_, View>) {
        let v = h.state_mut();
        v.viewport.camera_mut().look_from(-DVec3::Y);
        v.viewport.zoom_to_fit(&v.scene);
        h.run_steps(1);
    }

    /// Where a point of the model shows in the window.
    fn on_screen(h: &Harness<'_, View>, point: DVec3) -> egui::Pos2 {
        let rect = h.state().rect;
        let size = DVec2::new(rect.width().into(), rect.height().into());
        let p = h.state().viewport.camera().project(point, size).unwrap();
        rect.min + egui::vec2(p.x as f32, p.y as f32)
    }

    fn click(h: &mut Harness<'_, View>, pos: egui::Pos2, modifiers: egui::Modifiers) {
        h.event(egui::Event::ModifiersChanged(modifiers));
        h.hover_at(pos);
        h.run_steps(1);
        button(h, egui::PointerButton::Primary, pos, true);
        button(h, egui::PointerButton::Primary, pos, false);
        h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        // Let the click age, so the next one isn't a double click.
        h.run_steps(12);
    }

    fn drag_box(h: &mut Harness<'_, View>, from: egui::Pos2, to: egui::Pos2) {
        h.hover_at(from);
        h.run_steps(1);
        button(h, egui::PointerButton::Primary, from, true);
        h.run_steps(1);
        for step in 1..=4 {
            h.hover_at(from + (to - from) * step as f32 / 4.0);
            h.run_steps(1);
        }
        button(h, egui::PointerButton::Primary, to, false);
        h.run_steps(2);
    }

    fn selected(h: &Harness<'_, View>) -> Vec<ElementId> {
        h.state().selection.ids().iter().copied().collect()
    }

    fn sorted(mut ids: Vec<ElementId>) -> Vec<ElementId> {
        ids.sort();
        ids
    }

    #[test]
    fn click_selects_ctrl_adds_shift_removes_and_empty_space_clears() {
        let mut h = view(None);
        from_the_front(&mut h);
        let [first, second, _] = h.state().ids;
        let at_first = on_screen(&h, DVec3::new(0.0, 0.0, 1.5));
        let at_second = on_screen(&h, DVec3::new(6.0, 0.0, 1.5));

        click(&mut h, at_first, egui::Modifiers::NONE);
        assert_eq!(selected(&h), [first]);
        click(&mut h, at_second, egui::Modifiers::NONE);
        assert_eq!(selected(&h), [second], "a click replaces");
        click(&mut h, at_first, egui::Modifiers::COMMAND);
        assert_eq!(selected(&h), sorted(vec![first, second]), "Ctrl adds");
        click(&mut h, at_second, egui::Modifiers::SHIFT);
        assert_eq!(selected(&h), [first], "Shift removes");
        let empty = on_screen(&h, DVec3::new(3.0, 0.0, 1.5));
        click(&mut h, empty, egui::Modifiers::NONE);
        assert!(selected(&h).is_empty(), "empty space clears");
    }

    #[test]
    fn tab_cycles_through_overlapping_elements() {
        let mut h = view(None);
        from_the_front(&mut h);
        let [first, _, behind] = h.state().ids;
        let at = on_screen(&h, DVec3::new(0.0, 0.0, 1.5));
        h.hover_at(at);
        h.run_steps(2);
        assert_eq!(h.state().scene.prehighlight(), Some(first), "the nearest");
        h.key_press(egui::Key::Tab);
        h.run_steps(2);
        assert_eq!(h.state().scene.prehighlight(), Some(behind), "Tab");
        click(&mut h, at, egui::Modifiers::NONE);
        assert_eq!(selected(&h), [behind]);
        h.key_press(egui::Key::Tab);
        h.run_steps(2);
        assert_eq!(h.state().scene.prehighlight(), Some(first), "round again");
    }

    #[test]
    fn window_boxes_take_whole_elements_and_crossing_boxes_touched_ones() {
        let mut h = view(None);
        from_the_front(&mut h);
        let [first, second, behind] = h.state().ids;
        // Left to right round the first column and the one behind it.
        let a = on_screen(&h, DVec3::new(-1.0, 0.0, 3.5));
        let b = on_screen(&h, DVec3::new(1.0, 0.0, -0.5));
        drag_box(&mut h, a, b);
        assert_eq!(selected(&h), sorted(vec![first, behind]), "window");

        // A window round only the second column's middle takes nothing.
        let a = on_screen(&h, DVec3::new(5.0, 0.0, 2.0));
        let b = on_screen(&h, DVec3::new(7.0, 0.0, 1.0));
        drag_box(&mut h, a, b);
        assert!(selected(&h).is_empty(), "nothing wholly inside");
        // The same box right to left crosses it.
        drag_box(&mut h, b, a);
        assert_eq!(selected(&h), [second], "crossing");
    }

    #[test]
    fn orbiting_turns_round_the_selection() {
        let mut h = view(None);
        let second = h.state().ids[1];
        h.state_mut()
            .selection
            .apply([second], crate::selection::Mode::Replace);
        h.run_steps(1);
        let center = DVec3::new(6.0, 0.0, 1.5);
        let before = on_screen(&h, center);
        wheel_drag(&mut h, egui::vec2(120.0, 40.0), egui::Modifiers::NONE);
        let after = on_screen(&h, center);
        assert!((after - before).length() < 0.01, "{before:?} {after:?}");
    }

    /// With a wgpu device, the columns show shaded, with dark edges, and
    /// the view renders again only when something changes.
    #[test]
    fn elements_show_shaded_with_their_edges() {
        use egui_kittest::wgpu::{create_render_state, default_wgpu_setup};
        let options = eframe::egui_wgpu::RendererOptions::PREDICTABLE;
        let Ok(rs) =
            std::panic::catch_unwind(|| create_render_state(default_wgpu_setup(), options))
        else {
            let required = std::env::var("HITO_REQUIRE_GPU").is_ok_and(|v| !v.is_empty());
            assert!(
                !required,
                "HITO_REQUIRE_GPU is set but no wgpu adapter was found"
            );
            println!("skipped: no wgpu adapter");
            return;
        };
        let mut h = view(Some(rs));
        let renders = h.state().viewport.renders;
        assert!(renders > 0, "rendered");
        h.run_steps(3);
        assert_eq!(h.state().viewport.renders, renders, "nothing changed");

        // From the front, the columns stand at either side of the view.
        let camera = h.state_mut().viewport.camera_mut();
        camera.look_from(-DVec3::Y);
        h.run_steps(1);
        assert_eq!(h.state().viewport.renders, renders + 1, "the camera moved");
        let v = h.state_mut();
        v.viewport.zoom_to_fit(&v.scene);
        let rect = v.rect;
        h.run_steps(1);

        let image = h.render().expect("render");
        let size = DVec2::new(rect.width().into(), rect.height().into());
        let camera = h.state().viewport.camera();
        let at = |p: DVec3| {
            let s = camera.project(p, size).unwrap();
            (
                rect.min.x as u32 + s.x as u32,
                rect.min.y as u32 + s.y as u32,
            )
        };
        let pixel = |(x, y): (u32, u32)| image.get_pixel(x, y).0;
        let between = pixel(at(DVec3::new(3.0, 0.0, 1.5)));
        assert_eq!(&between[..3], &BACKGROUND, "between the columns");
        let (x, y) = at(DVec3::new(0.0, 0.0, 1.5));
        let face = pixel((x, y));
        assert!(
            face[..3].iter().all(|&c| (120..250).contains(&c)),
            "shaded {face:?}"
        );
        let dark =
            (x.saturating_sub(15)..x + 15).any(|x| pixel((x, y))[..3].iter().all(|&c| c < 90));
        assert!(dark, "an edge on the column's side");

        // Selected, it shows blue.
        let first = h.state().ids[0];
        let v = h.state_mut();
        v.selection.apply([first], crate::selection::Mode::Replace);
        h.run_steps(2);
        let image = h.render().expect("render");
        let [r, g, b, _] = image.get_pixel(x, y).0;
        assert!(
            b > r.saturating_add(40) && b > g,
            "selected {:?}",
            [r, g, b]
        );
    }
}
