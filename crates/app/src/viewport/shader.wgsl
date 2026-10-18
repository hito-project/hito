// The 3D view's shader: shaded faces with their edges drawn over them, as in
// Revit's Shaded visual style.

struct View {
    // World (relative to the scene's origin) to clip space.
    view_projection: mat4x4<f32>,
    // Towards the light, in the world. It comes from over the viewer's
    // shoulder, so every face the camera sees is lit.
    light: vec4<f32>,
}

struct Element {
    face: vec4<f32>,
    edge: vec4<f32>,
}

@group(0) @binding(0) var<uniform> view: View;
@group(1) @binding(0) var<uniform> element: Element;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
}

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>) -> VertexOut {
    var out: VertexOut;
    out.clip = view.view_projection * vec4<f32>(position, 1.0);
    out.normal = normal;
    return out;
}

@fragment
fn fs_face(in: VertexOut) -> @location(0) vec4<f32> {
    let lit = max(dot(normalize(in.normal), view.light.xyz), 0.0);
    return vec4<f32>(element.face.rgb * (0.6 + 0.4 * lit), 1.0);
}

@fragment
fn fs_edge(in: VertexOut) -> @location(0) vec4<f32> {
    return element.edge;
}
