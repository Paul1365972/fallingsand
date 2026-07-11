#define_import_path fallingsand::layer_common

fn layer_texel(uv: vec2<f32>, native: vec2<f32>) -> vec2<f32> {
    return min(floor(uv * native), native - vec2<f32>(1.0));
}

fn layer_cell(t: vec2<f32>, snapped_cam: vec2<f32>, native: vec2<f32>) -> vec2<f32> {
    return snapped_cam + vec2<f32>(t.x + 0.5 - native.x * 0.5, native.y * 0.5 - t.y - 0.5);
}

fn pcg(v: u32) -> u32 {
    var x = v * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return (x >> 22u) ^ x;
}
