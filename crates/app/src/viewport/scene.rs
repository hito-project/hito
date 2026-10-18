//! What the 3D views show: every element's mesh from the geometry cache,
//! uploaded once to the graphics device and shared by every view.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use eframe::egui_wgpu::{RenderState, wgpu};
use glam::{DVec3, Vec3};
use hito_core::ElementId;
use hito_geometry::{GeometryCache, Mesh, Regenerated};
use wgpu::util::DeviceExt;

use super::camera::Bounds;

pub(super) const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub(super) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Multisampling smooths the edges' lines. Four samples are supported for
/// these formats on every device.
pub(super) const SAMPLES: u32 = 4;

/// The colour of elements' faces: a light grey, as Revit's Shaded style
/// shows concrete.
const FACE: [f32; 4] = [0.80, 0.80, 0.78, 1.0];
/// The colour of elements' edges.
const EDGE: [f32; 4] = [0.08, 0.08, 0.08, 1.0];
/// Selected elements show in Revit's selection blue, faces tinted.
const SELECTED_FACE: [f32; 4] = [0.55, 0.68, 0.95, 1.0];
const SELECTED_EDGE: [f32; 4] = [0.0, 0.23, 0.74, 1.0];
/// The element a click would select has blue edges, as Revit's
/// pre-selection highlight.
const PREHIGHLIGHT_EDGE: [f32; 4] = [0.0, 0.47, 0.95, 1.0];

/// Edges between faces that meet at more than this angle are drawn. Lower
/// angles are a curved surface's facets, or a flat face's triangles.
const CREASE: f32 = 0.35; // radians, 20°

/// The meshes the 3D views show, by element.
#[derive(Default)]
pub struct Scene {
    elements: BTreeMap<ElementId, Arc<Mesh>>,
    /// Where vertex positions are measured from on the device. Positions
    /// are `f32` there, so they're kept small: a model far from the origin
    /// keeps its precision.
    origin: Option<DVec3>,
    /// What shows highlighted: the selection, and the element a click
    /// would select.
    selected: BTreeSet<ElementId>,
    prehighlight: Option<ElementId>,
    revision: u64,
    geometry_revision: u64,
    gpu: Option<Gpu>,
}

impl Scene {
    /// Shows `mesh` for the element, in place of what it showed before.
    pub fn insert(&mut self, id: ElementId, mesh: Arc<Mesh>) {
        if mesh.is_empty() {
            return self.remove(id);
        }
        if self.origin.is_none() {
            self.origin = mesh.positions.first().map(|&p| DVec3::from(p).round());
        }
        self.elements.insert(id, mesh);
        self.geometry_revision += 1;
        self.changed();
    }

    pub fn remove(&mut self, id: ElementId) {
        if self.elements.remove(&id).is_some() {
            self.geometry_revision += 1;
            self.changed();
        }
    }

    /// Takes what the geometry cache generated again.
    pub fn update(&mut self, cache: &GeometryCache, regenerated: &Regenerated) {
        for &id in &regenerated.updated {
            match cache.mesh(id) {
                Some(mesh) => self.insert(id, mesh.clone()),
                None => self.remove(id),
            }
        }
        for &id in &regenerated.removed {
            self.remove(id);
        }
    }

    /// Shows these elements selected.
    pub fn set_selected(&mut self, selected: &BTreeSet<ElementId>) {
        if self.selected != *selected {
            self.selected.clone_from(selected);
            self.changed();
        }
    }

    pub fn selected(&self) -> &BTreeSet<ElementId> {
        &self.selected
    }

    /// Shows the element a click would select.
    pub fn set_prehighlight(&mut self, id: Option<ElementId>) {
        if self.prehighlight != id {
            self.prehighlight = id;
            self.changed();
        }
    }

    pub fn prehighlight(&self) -> Option<ElementId> {
        self.prehighlight
    }

    /// The box round the selected elements that show, `None` if none do.
    pub fn selection_bounds(&self) -> Option<Bounds> {
        self.selected
            .iter()
            .filter_map(|id| self.elements.get(id)?.bounds())
            .map(|(min, max)| Bounds::new(min, max))
            .reduce(Bounds::union)
    }

    /// Every element's mesh, by element.
    pub fn meshes(&self) -> impl Iterator<Item = (ElementId, &Arc<Mesh>)> {
        self.elements.iter().map(|(&id, mesh)| (id, mesh))
    }

    /// The element's colours: selected, about to be, or neither.
    fn colors(&self, id: ElementId) -> ElementColors {
        let (face, edge) = if self.selected.contains(&id) {
            (SELECTED_FACE, SELECTED_EDGE)
        } else if self.prehighlight == Some(id) {
            (FACE, PREHIGHLIGHT_EDGE)
        } else {
            (FACE, EDGE)
        };
        ElementColors { face, edge }
    }

    /// The element's mesh, if it shows.
    pub fn mesh(&self, id: ElementId) -> Option<&Arc<Mesh>> {
        self.elements.get(&id)
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// The box round everything shown, `None` if nothing is.
    pub fn bounds(&self) -> Option<Bounds> {
        self.elements
            .values()
            .filter_map(|mesh| mesh.bounds())
            .map(|(min, max)| Bounds::new(min, max))
            .reduce(Bounds::union)
    }

    /// Goes up by one each time what the scene shows changes, so views
    /// know to render again.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Like [`revision`](Scene::revision), but only for the meshes, not
    /// their highlighting.
    pub fn geometry_revision(&self) -> u64 {
        self.geometry_revision
    }

    pub(super) fn origin(&self) -> DVec3 {
        self.origin.unwrap_or_default()
    }

    fn changed(&mut self) {
        self.revision += 1;
    }

    /// Uploads what changed since the last call. Call it once a frame,
    /// before the views draw.
    pub fn prepare(&mut self, rs: &RenderState) {
        let colors: Vec<_> = self.elements.keys().map(|&id| self.colors(id)).collect();
        let gpu = self.gpu.get_or_insert_with(|| Gpu::new(&rs.device));
        if gpu.revision == Some(self.revision) {
            return;
        }
        let origin = self.origin.unwrap_or_default();
        gpu.meshes.retain(|id, uploaded| {
            self.elements
                .get(id)
                .is_some_and(|m| Arc::ptr_eq(m, &uploaded.mesh))
        });
        for (slot, (&id, mesh)) in self.elements.iter().enumerate() {
            let uploaded = gpu
                .meshes
                .entry(id)
                .or_insert_with(|| GpuMesh::new(&rs.device, mesh.clone(), origin));
            uploaded.slot = slot as u32;
        }
        gpu.write_elements(rs, &colors);
        gpu.revision = Some(self.revision);
    }

    pub(super) fn gpu(&self) -> Option<&Gpu> {
        self.gpu.as_ref()
    }
}

/// A vertex: where it is and the normal of its face.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
}

/// An element's colours, as the shader's `Element`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ElementColors {
    face: [f32; 4],
    edge: [f32; 4],
}

/// What the scene keeps on the device: the pipelines every view draws with,
/// each element's vertices, and every element's colours in one buffer.
pub(super) struct Gpu {
    pub faces: wgpu::RenderPipeline,
    pub edges: wgpu::RenderPipeline,
    pub view_layout: wgpu::BindGroupLayout,
    element_layout: wgpu::BindGroupLayout,
    meshes: HashMap<ElementId, GpuMesh>,
    elements: Option<(wgpu::Buffer, wgpu::BindGroup, usize)>,
    /// The distance between elements' colours in their buffer.
    stride: u64,
    revision: Option<u64>,
}

pub(super) struct GpuMesh {
    mesh: Arc<Mesh>,
    faces: wgpu::Buffer,
    face_vertices: u32,
    edges: wgpu::Buffer,
    edge_vertices: u32,
    /// Where its colours are in the elements' buffer.
    slot: u32,
}

impl Gpu {
    fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let uniform = |label, dynamic| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: dynamic,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            })
        };
        let view_layout = uniform("3D view", false);
        let element_layout = uniform("3D view elements", true);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("3D view"),
            bind_group_layouts: &[Some(&view_layout), Some(&element_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        let buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attributes,
        })];
        let pipeline = |topology, fragment, bias| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("3D view"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias,
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(COLOR_FORMAT.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        // Faces sit a little behind their edges, so the edges show.
        let behind = wgpu::DepthBiasState {
            constant: 2,
            slope_scale: 1.0,
            clamp: 0.0,
        };
        let faces = pipeline(wgpu::PrimitiveTopology::TriangleList, "fs_face", behind);
        let edges = pipeline(
            wgpu::PrimitiveTopology::LineList,
            "fs_edge",
            Default::default(),
        );
        let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment);
        Self {
            faces,
            edges,
            view_layout,
            element_layout,
            meshes: HashMap::new(),
            elements: None,
            stride: (size_of::<ElementColors>() as u64).next_multiple_of(alignment),
            revision: None,
        }
    }

    /// Writes every element's colours, growing the buffer if it's too
    /// small.
    fn write_elements(&mut self, rs: &RenderState, colors: &[ElementColors]) {
        let count = colors.len().max(1);
        if self.elements.as_ref().is_none_or(|(_, _, n)| *n < count) {
            let capacity = count.next_power_of_two();
            let buffer = rs.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("3D view elements"),
                size: self.stride * capacity as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = rs.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("3D view elements"),
                layout: &self.element_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(size_of::<ElementColors>() as u64),
                    }),
                }],
            });
            self.elements = Some((buffer, group, capacity));
        }
        let (buffer, _, capacity) = self.elements.as_ref().expect("just made");
        let mut bytes = vec![0u8; self.stride as usize * capacity];
        for (slot, colors) in bytes.chunks_mut(self.stride as usize).zip(colors) {
            slot[..size_of::<ElementColors>()].copy_from_slice(bytemuck::bytes_of(colors));
        }
        rs.queue.write_buffer(buffer, 0, &bytes);
    }

    /// Draws every element: `faces` or `edges`.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, edges: bool) {
        let Some((_, group, _)) = &self.elements else {
            return;
        };
        pass.set_pipeline(if edges { &self.edges } else { &self.faces });
        for mesh in self.meshes.values() {
            let (buffer, count) = if edges {
                (&mesh.edges, mesh.edge_vertices)
            } else {
                (&mesh.faces, mesh.face_vertices)
            };
            if count > 0 {
                let offset = u64::from(mesh.slot) * self.stride;
                pass.set_bind_group(1, group, &[offset as u32]);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..count, 0..1);
            }
        }
    }
}

impl GpuMesh {
    fn new(device: &wgpu::Device, mesh: Arc<Mesh>, origin: DVec3) -> Self {
        let (faces, edges) = vertices(&mesh, origin);
        let buffer = |label, data: &[Vertex]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                // A buffer can't be empty.
                contents: if data.is_empty() {
                    &[0; size_of::<Vertex>()]
                } else {
                    bytemuck::cast_slice(data)
                },
                usage: wgpu::BufferUsages::VERTEX,
            })
        };
        Self {
            mesh,
            faces: buffer("3D view faces", &faces),
            face_vertices: faces.len() as u32,
            edges: buffer("3D view edges", &edges),
            edge_vertices: edges.len() as u32,
            slot: 0,
        }
    }
}

/// A mesh's triangles, each with its own corners and its face's normal so
/// faces look flat, and the lines of its edges: where the mesh ends, or
/// where two faces meet at a crease.
fn vertices(mesh: &Mesh, origin: DVec3) -> (Vec<Vertex>, Vec<Vertex>) {
    let at = |i: u32| (DVec3::from(mesh.positions[i as usize]) - origin).as_vec3();
    let mut faces = Vec::with_capacity(mesh.triangles.len() * 3);
    let mut normals = Vec::with_capacity(mesh.triangles.len());
    for &[a, b, c] in &mesh.triangles {
        let (pa, pb, pc) = (at(a), at(b), at(c));
        let normal = (pb - pa).cross(pc - pa).normalize_or_zero();
        normals.push(normal);
        for position in [pa, pb, pc] {
            faces.push(Vertex {
                position: position.into(),
                normal: normal.into(),
            });
        }
    }

    let mut sides: HashMap<(u32, u32), Vec<Vec3>> = HashMap::new();
    for (&[a, b, c], &normal) in mesh.triangles.iter().zip(&normals) {
        for (from, to) in [(a, b), (b, c), (c, a)] {
            sides
                .entry((from.min(to), from.max(to)))
                .or_default()
                .push(normal);
        }
    }
    let mut edges = Vec::new();
    let mut keys: Vec<_> = sides.keys().copied().collect();
    keys.sort_unstable();
    for key in keys {
        let normals = &sides[&key];
        let crease = match normals.as_slice() {
            [n1, n2] => n1.angle_between(*n2) > CREASE,
            _ => true,
        };
        if crease {
            for i in [key.0, key.1] {
                edges.push(Vertex {
                    position: at(i).into(),
                    normal: [0.0; 3],
                });
            }
        }
    }
    (faces, edges)
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use hito_geometry::{CHORD_TOLERANCE, Csg, Frame, GeometryError, Kernel, Profile, Solid};

    /// Extrusions don't need a kernel.
    struct NoKernel;

    impl Kernel for NoKernel {
        fn evaluate(&self, _: &Csg) -> Result<Mesh, GeometryError> {
            Err(GeometryError::Kernel("not needed".into()))
        }
    }

    pub(crate) fn mesh(solid: Solid) -> Arc<Mesh> {
        Arc::new(solid.mesh(&NoKernel, CHORD_TOLERANCE).unwrap())
    }

    /// A 3 m tall square column, centred on (x, 0).
    pub(crate) fn column(x: f64, width: f64) -> Arc<Mesh> {
        mesh(Solid::Extrusion {
            profile: Profile::Rectangle {
                width,
                height: width,
            },
            frame: Frame::at([x, 0.0, 0.0]),
            depth: 3.0,
        })
    }

    #[test]
    fn a_box_has_its_twelve_edges_and_no_diagonals() {
        let (faces, edges) = vertices(&column(0.0, 0.3), DVec3::ZERO);
        assert_eq!(faces.len(), 12 * 3, "two triangles a side");
        assert_eq!(edges.len(), 12 * 2);
    }

    #[test]
    fn a_round_column_has_no_lines_down_its_side() {
        let round = mesh(Solid::Extrusion {
            profile: Profile::Circle { radius: 0.2 },
            frame: Frame::WORLD,
            depth: 3.0,
        });
        let (_, edges) = vertices(&round, DVec3::ZERO);
        let vertical = edges
            .chunks(2)
            .filter(|line| line[0].position[2] != line[1].position[2])
            .count();
        assert_eq!(vertical, 0);
    }

    #[test]
    fn far_from_the_origin_vertices_stay_precise() {
        let far = column(5_500_000.0, 0.3);
        let mut scene = Scene::default();
        let id = ElementId::generate();
        scene.insert(id, far.clone());
        let (faces, _) = vertices(&far, scene.origin());
        let widths: Vec<f32> = faces.iter().map(|v| v.position[0]).collect();
        let span = widths.iter().copied().fold(f32::MIN, f32::max)
            - widths.iter().copied().fold(f32::MAX, f32::min);
        assert!((span - 0.3).abs() < 1e-6, "{span}");
    }

    #[test]
    fn bounds_and_revisions_follow_the_elements() {
        let mut scene = Scene::default();
        assert_eq!(scene.bounds(), None);
        let (a, b) = (ElementId::generate(), ElementId::generate());
        scene.insert(a, column(0.0, 0.2));
        scene.insert(b, column(10.0, 0.4));
        let bounds = scene.bounds().unwrap();
        assert_eq!(bounds.max, DVec3::new(10.2, 0.2, 3.0));
        let revision = scene.revision();
        scene.remove(b);
        assert!(scene.revision() > revision);
        assert_eq!(scene.bounds().unwrap().max, DVec3::new(0.1, 0.1, 3.0));
        let revision = scene.revision();
        scene.remove(b);
        assert_eq!(scene.revision(), revision, "nothing changed");
    }

    /// The scene follows the geometry cache (#28) through committed change
    /// sets, as the shell does after each command.
    #[test]
    fn the_scene_follows_the_geometry_cache() {
        use hito_core::{
            ClassDef, ClassId, DataType, Dimension, Element, ElementStore, Model, ModelKind,
            Schema, SchemaRegistry, SchemaVersion,
        };
        use hito_geometry::{Generator, GeometryCache};

        let column = ClassId::new("Test", "Column");
        let schema = Schema::new("Test", SchemaVersion::new(1, 0, 0)).class(
            ClassDef::new("Column", ClassId::GEOMETRIC_ELEMENT_3D)
                .property("X", DataType::Quantity(Dimension::LENGTH)),
        );
        let mut registry = SchemaRegistry::new();
        registry.register(schema).unwrap();
        let mut store = ElementStore::new(registry);
        let generator = Generator::new(column.clone(), |inputs, element| {
            Some(Solid::Extrusion {
                profile: Profile::Rectangle {
                    width: 0.3,
                    height: 0.3,
                },
                frame: Frame::at([inputs.real(element.id(), "X")?, 0.0, 0.0]),
                depth: 3.0,
            })
        });
        let mut cache = GeometryCache::new(Arc::new(NoKernel), [generator]);
        let mut scene = Scene::default();

        let mut tx = store.transaction();
        let model = tx.insert_model(Model::new(ModelKind::Physical)).unwrap();
        let id = tx
            .insert(Element::new(column, model).with_property("X", 4.0))
            .unwrap();
        let changes = tx.commit().unwrap();
        let regenerated = cache.update(&store, &changes);
        scene.update(&cache, &regenerated);
        assert_eq!(scene.bounds().unwrap().center(), DVec3::new(4.0, 0.0, 1.5));

        let mut tx = store.transaction();
        tx.set_property(id, "X", 10.0).unwrap();
        let changes = tx.commit().unwrap();
        let regenerated = cache.update(&store, &changes);
        scene.update(&cache, &regenerated);
        assert_eq!(scene.bounds().unwrap().center(), DVec3::new(10.0, 0.0, 1.5));

        let mut tx = store.transaction();
        tx.delete(id).unwrap();
        let changes = tx.commit().unwrap();
        let regenerated = cache.update(&store, &changes);
        scene.update(&cache, &regenerated);
        assert!(scene.is_empty());
    }
}
