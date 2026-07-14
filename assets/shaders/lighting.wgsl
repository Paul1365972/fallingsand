#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import fallingsand::layer_common::{layer_texel, layer_cell}
#import fallingsand::light_common::{light_params, glow}

@group(#{MATERIAL_BIND_GROUP}) @binding(1) var world_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var world_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var glow_tex: texture_2d<f32>;

const EMISSIVE_GAIN: f32 = 1.4;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let native = light_params.native_size;
    let t = layer_texel(in.uv, native);
    let world = textureLoad(world_tex, vec2<u32>(t), 0);
    let cell = layer_cell(t, light_params.snapped_cam, native);

    let e = min(textureLoad(glow_tex, vec2<u32>(t), 0).rgb * EMISSIVE_GAIN, vec3<f32>(1.0));
    let point = glow(cell);
    let lit = max(point, max(e.r, max(e.g, e.b)));
    let factor = clamp(light_params.darkness * (1.0 - lit), 0.0, 1.0);
    let dark = vec3<f32>(0.01, 0.012, 0.03);
    var rgb = mix(world.rgb, dark * world.a, factor);
    rgb = rgb + e * light_params.darkness * world.a;
    return vec4<f32>(rgb, world.a);
}
