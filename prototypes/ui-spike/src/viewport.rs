//! An embedded wgpu 3D viewport. It renders off-screen, with its own depth
//! buffer, into a texture that egui shows as an image. This is the pattern a
//! real BIM viewport needs: our own passes (depth, MSAA, picking, section
//! boxes), with egui only compositing the result.

use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu;
use glam::{Mat4, Vec3};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 4], // clip space, transformed on the CPU to keep the prototype short
    color: [f32; 4],
}

const SHADER: &str = r#"
struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) color: vec4<f32> };
@vertex fn vs(@location(0) pos: vec4<f32>, @location(1) color: vec4<f32>) -> VsOut {
    var o: VsOut; o.pos = pos; o.color = color; return o;
}
@fragment fn fs(i: VsOut) -> @location(0) vec4<f32> { return i.color; }
"#;

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// A few "elements": a slab, four columns and two beams, as boxes.
fn scene() -> Vec<(Vec3, Vec3, [f32; 4])> {
    let concrete = [0.72, 0.72, 0.70, 1.0];
    let column = [0.55, 0.62, 0.75, 1.0];
    let beam = [0.80, 0.60, 0.45, 1.0];
    let mut v = vec![(Vec3::new(-3.0, -2.0, 3.0), Vec3::new(3.0, 2.0, 3.2), concrete)];
    for (x, y) in [(-2.8, -1.8), (2.8, -1.8), (-2.8, 1.8), (2.8, 1.8)] {
        v.push((Vec3::new(x - 0.15, y - 0.15, 0.0), Vec3::new(x + 0.15, y + 0.15, 3.0), column));
    }
    for y in [-1.8, 1.8] {
        v.push((Vec3::new(-2.8, y - 0.1, 2.6), Vec3::new(2.8, y + 0.1, 3.0), beam));
    }
    v
}

fn box_triangles(min: Vec3, max: Vec3, color: [f32; 4], out: &mut Vec<(Vec3, [f32; 4])>) {
    let c = |x: bool, y: bool, z: bool| {
        Vec3::new(if x { max.x } else { min.x }, if y { max.y } else { min.y }, if z { max.z } else { min.z })
    };
    // (face corners, shade) for simple directional shading
    let faces = [
        ([c(false, false, true), c(true, false, true), c(true, true, true), c(false, true, true)], 1.0),
        ([c(false, false, false), c(false, true, false), c(true, true, false), c(true, false, false)], 0.5),
        ([c(false, false, false), c(true, false, false), c(true, false, true), c(false, false, true)], 0.8),
        ([c(false, true, false), c(false, true, true), c(true, true, true), c(true, true, false)], 0.7),
        ([c(false, false, false), c(false, false, true), c(false, true, true), c(false, true, false)], 0.6),
        ([c(true, false, false), c(true, true, false), c(true, true, true), c(true, false, true)], 0.9),
    ];
    for (q, s) in faces {
        let col = [color[0] * s, color[1] * s, color[2] * s, 1.0];
        for i in [0, 1, 2, 0, 2, 3] {
            out.push((q[i], col));
        }
    }
}

pub struct Viewport {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    gpu: Option<Gpu>,
    /// How many frames were rendered, so tests can check it actually ran.
    pub frames_rendered: u64,
}

struct Gpu {
    pipeline: wgpu::RenderPipeline,
    vbuf: wgpu::Buffer,
    vcount: u32,
    size: [u32; 2],
    color: wgpu::Texture,
    depth: wgpu::Texture,
    texture_id: egui::TextureId,
}

impl Default for Viewport {
    fn default() -> Self {
        Self { yaw: 0.7, pitch: 0.5, distance: 14.0, gpu: None, frames_rendered: 0 }
    }
}

impl Viewport {
    fn vertices(&self, aspect: f32) -> Vec<Vertex> {
        let target = Vec3::new(0.0, 0.0, 1.5);
        let eye = target
            + self.distance
                * Vec3::new(self.yaw.cos() * self.pitch.cos(), self.yaw.sin() * self.pitch.cos(), self.pitch.sin());
        let view = Mat4::look_at_rh(eye, target, Vec3::Z);
        let proj = Mat4::perspective_rh(45f32.to_radians(), aspect, 0.1, 500.0);
        let mvp = proj * view;
        let mut tris = vec![];
        for (min, max, col) in scene() {
            box_triangles(min, max, col, &mut tris);
        }
        tris.iter()
            .map(|(p, c)| Vertex { pos: (mvp * p.extend(1.0)).to_array(), color: *c })
            .collect()
    }

    fn textures(device: &wgpu::Device, size: [u32; 2]) -> (wgpu::Texture, wgpu::Texture) {
        let extent = wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 };
        let mk = |format, usage, label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        (
            mk(
                COLOR_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                "viewport color",
            ),
            mk(DEPTH_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT, "viewport depth"),
        )
    }

    fn init(rs: &egui_wgpu::RenderState, size: [u32; 2], vcount: usize) -> Gpu {
        let device = &rs.device;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewport"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attrs,
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(COLOR_FORMAT.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport vertices"),
            size: (vcount * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (color, depth) = Self::textures(device, size);
        let texture_id = rs.renderer.write().register_native_texture(
            device,
            &color.create_view(&Default::default()),
            wgpu::FilterMode::Linear,
        );
        Gpu { pipeline, vbuf, vcount: vcount as u32, size, color, depth, texture_id }
    }

    fn render(&mut self, rs: &egui_wgpu::RenderState, size: [u32; 2]) -> egui::TextureId {
        let verts = self.vertices(size[0] as f32 / size[1] as f32);
        let gpu = self.gpu.get_or_insert_with(|| Self::init(rs, size, verts.len()));
        if gpu.size != size {
            let (color, depth) = Self::textures(&rs.device, size);
            rs.renderer.write().update_egui_texture_from_wgpu_texture(
                &rs.device,
                &color.create_view(&Default::default()),
                wgpu::FilterMode::Linear,
                gpu.texture_id,
            );
            gpu.color = color;
            gpu.depth = depth;
            gpu.size = size;
        }
        rs.queue.write_buffer(&gpu.vbuf, 0, bytemuck::cast_slice(&verts));
        let color_view = gpu.color.create_view(&Default::default());
        let depth_view = gpu.depth.create_view(&Default::default());
        let mut enc = rs.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewport"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.93, g: 0.94, b: 0.96, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
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
            pass.set_pipeline(&gpu.pipeline);
            pass.set_vertex_buffer(0, gpu.vbuf.slice(..));
            pass.draw(0..gpu.vcount, 0..1);
        }
        rs.queue.submit([enc.finish()]);
        self.frames_rendered += 1;
        gpu.texture_id
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, rs: Option<&egui_wgpu::RenderState>, hint: &str) {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::drag());
        if response.dragged() {
            let d = response.drag_delta();
            self.yaw -= d.x * 0.01;
            self.pitch = (self.pitch + d.y * 0.01).clamp(-1.4, 1.4);
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            self.distance = (self.distance * (1.0 - scroll * 0.002)).clamp(2.0, 200.0);
        }
        let ppp = ui.pixels_per_point();
        let size = [(rect.width() * ppp).max(1.0) as u32, (rect.height() * ppp).max(1.0) as u32];
        match rs {
            Some(rs) => {
                let tex = self.render(rs, size);
                ui.painter().image(
                    tex,
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            None => {
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "no wgpu", Default::default(), egui::Color32::RED);
            }
        }
        ui.painter().text(
            rect.left_bottom() + egui::vec2(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            hint,
            egui::FontId::proportional(12.0),
            egui::Color32::DARK_GRAY,
        );
    }
}
