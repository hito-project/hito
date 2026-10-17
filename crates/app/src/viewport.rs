//! The 3D viewport. It renders off-screen with wgpu into its own texture,
//! which egui shows as an image in a view tab
//! ([ADR 0015](https://github.com/hito-project/hito/blob/main/docs/decisions/0015-ui-toolkit.md)).
//!
//! It only clears to the background colour for now. The scene comes with
//! #28 and navigation with #29, as more passes on the same texture.

use eframe::egui;
use eframe::egui_wgpu::{RenderState, wgpu};

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The background of a 3D view in the light theme: white, as in Revit.
pub const BACKGROUND: [u8; 3] = [255, 255, 255];
/// The background of a 3D view in the dark theme.
pub const BACKGROUND_DARK: [u8; 3] = [42, 45, 50];

/// One 3D view's renderer. It renders again only when what it shows
/// changes, which for now is its size and the theme.
#[derive(Default)]
pub struct Viewport {
    target: Option<Target>,
    /// How many times it has rendered, so tests can check that it did.
    pub renders: u64,
}

struct Target {
    size: [u32; 2],
    background: [u8; 3],
    texture: wgpu::Texture,
    id: egui::TextureId,
}

impl Viewport {
    /// Fills `ui` with the view. Without a wgpu device (in tests, or with
    /// another renderer) it shows `no_gpu` instead.
    pub fn ui(&mut self, ui: &mut egui::Ui, render_state: Option<&RenderState>, no_gpu: &str) {
        let rect = ui.available_rect_before_wrap();
        ui.allocate_rect(rect, egui::Sense::hover());
        let Some(rs) = render_state else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                no_gpu,
                egui::FontId::default(),
                ui.visuals().weak_text_color(),
            );
            return;
        };
        let ppp = ui.pixels_per_point();
        let size = [
            (rect.width() * ppp).max(1.0) as u32,
            (rect.height() * ppp).max(1.0) as u32,
        ];
        let background = if ui.visuals().dark_mode {
            BACKGROUND_DARK
        } else {
            BACKGROUND
        };
        let id = self.render(rs, size, background);
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter().image(id, rect, uv, egui::Color32::WHITE);
    }

    fn render(&mut self, rs: &RenderState, size: [u32; 2], background: [u8; 3]) -> egui::TextureId {
        if let Some(target) = &self.target
            && target.size == size
            && target.background == background
        {
            return target.id;
        }
        let texture = rs.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("3D view"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let id = {
            let mut renderer = rs.renderer.write();
            match &self.target {
                Some(old) => {
                    renderer.update_egui_texture_from_wgpu_texture(
                        &rs.device,
                        &view,
                        wgpu::FilterMode::Linear,
                        old.id,
                    );
                    old.id
                }
                None => {
                    renderer.register_native_texture(&rs.device, &view, wgpu::FilterMode::Linear)
                }
            }
        };

        let [r, g, b] = background.map(|c| f64::from(c) / 255.0);
        let mut encoder = rs.device.create_command_encoder(&Default::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("3D view"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        rs.queue.submit([encoder.finish()]);
        self.renders += 1;
        self.target = Some(Target {
            size,
            background,
            texture,
            id,
        });
        id
    }

    /// Frees the texture. Call it when the view's tab closes.
    pub fn free(&mut self, rs: Option<&RenderState>) {
        if let (Some(target), Some(rs)) = (self.target.take(), rs) {
            rs.renderer.write().free_texture(&target.id);
            target.texture.destroy();
        }
    }
}
