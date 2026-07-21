struct OverlayFrame {
    window_size: vec2<f32>,
}

struct LineInstance {
    a: vec2<f32>,
    b: vec2<f32>,
    color: vec4<f32>,
}

struct LineOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: OverlayFrame;
@group(0) @binding(1) var<storage, read> lines: array<LineInstance>;

@vertex
fn line_vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> LineOutput {
    let item = lines[instance];
    let direction = item.b - item.a;
    let normal = normalize(vec2<f32>(-direction.y, direction.x) + vec2<f32>(1e-5, 0.0)) * 0.5;
    let corners = array<vec2<f32>, 6>(
        item.a - normal, item.b - normal, item.b + normal,
        item.a - normal, item.b + normal, item.a + normal
    );
    let position = corners[vertex];
    var out: LineOutput;
    out.clip_position = vec4<f32>(position.x * 2.0 / frame.window_size.x, position.y * 2.0 / frame.window_size.y, 0.0, 1.0);
    out.color = item.color;
    return out;
}

@fragment
fn line_fragment(in: LineOutput) -> @location(0) vec4<f32> {
    return in.color;
}
