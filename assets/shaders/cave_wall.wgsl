#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import fallingsand::layer_common::{layer_texel, layer_cell}
#import fallingsand::light_common::{light_params, glow}

struct WallParams {
    base_color: vec4<f32>,
    world_offset: vec2<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> wall: WallParams;

fn pcg(v: u32) -> u32 {
    var x = v * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return (x >> 22u) ^ x;
}

fn hash2(p: vec2<f32>) -> f32 {
    let v = vec2<i32>(p);
    let x = pcg((bitcast<u32>(v.x) * 374761393u) ^ (bitcast<u32>(v.y) * 668265263u));
    return f32(x) / 4294967296.0;
}

fn vnoise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = p - i;
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm2(p: vec2<f32>) -> f32 {
    return vnoise2(p) * 0.55
        + vnoise2(p * 2.03 + vec2<f32>(13.7, 41.3)) * 0.3
        + vnoise2(p * 4.01 + vec2<f32>(71.9, 7.5)) * 0.15;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let native = light_params.native_size;
    let cell = layer_cell(layer_texel(in.uv, native), light_params.snapped_cam, native);
    let world = cell + wall.world_offset;
    let n = fbm2(cell * 0.11);
    let step = min(u32(n * 4.0), 3u);
    var rgb = wall.base_color.rgb * (0.7 + 0.15 * f32(step));
    let g = glow(world);
    let factor = clamp(light_params.darkness * (1.0 - g), 0.0, 1.0);
    rgb = mix(rgb, vec3<f32>(0.01, 0.012, 0.03), factor);
    let a = (1.0 - smoothstep(-24.0, 8.0, world.y)) * wall.base_color.a;
    return vec4<f32>(rgb, a);
}
