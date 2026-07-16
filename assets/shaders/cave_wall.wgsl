#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import fallingsand::layer_common::{layer_texel, layer_cell, vnoise}
#import fallingsand::light_common::{light_params, glow, CAVE_DARK}

struct WallParams {
    base_color: vec4<f32>,
    world_offset: vec2<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> wall: WallParams;

fn fbm2(p: vec2<f32>) -> f32 {
    return vnoise(p) * 0.55
        + vnoise(p * 2.03 + vec2<f32>(13.7, 41.3)) * 0.3
        + vnoise(p * 4.01 + vec2<f32>(71.9, 7.5)) * 0.15;
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
    rgb = mix(rgb, CAVE_DARK, factor);
    let a = (1.0 - smoothstep(-24.0, 8.0, world.y)) * wall.base_color.a;
    return vec4<f32>(rgb, a);
}
