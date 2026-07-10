#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import fallingsand::layer_common::{layer_texel, layer_cell}

struct SilhouetteParams {
    color: vec4<f32>,
    snapped_cam: vec2<f32>,
    native_size: vec2<f32>,
    base: f32,
    amp: f32,
    inv_wavelength: f32,
    seed: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: SilhouetteParams;

fn pcg(v: u32) -> u32 {
    var x = v * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return (x >> 22u) ^ x;
}

fn hash1(p: f32) -> f32 {
    let x = pcg(bitcast<u32>(i32(p)) ^ (u32(params.seed) * 374761393u));
    return f32(x) / 4294967296.0;
}

fn vnoise1(x: f32) -> f32 {
    let i = floor(x);
    let f = x - i;
    let u = f * f * (3.0 - 2.0 * f);
    return mix(hash1(i), hash1(i + 1.0), u);
}

fn fbm1(x: f32) -> f32 {
    return vnoise1(x) * 0.55 + vnoise1(x * 2.03 + 13.7) * 0.3 + vnoise1(x * 4.01 + 41.3) * 0.15;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let native = params.native_size;
    let cell = layer_cell(layer_texel(in.uv, native), params.snapped_cam, native);
    let h = params.base + params.amp * (fbm1(cell.x * params.inv_wavelength) * 2.0 - 1.0);
    let a = select(0.0, params.color.a, cell.y < h);
    return vec4<f32>(params.color.rgb, a);
}
