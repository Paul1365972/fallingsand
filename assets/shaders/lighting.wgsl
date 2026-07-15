#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import fallingsand::layer_common::{layer_texel, layer_cell}
#import fallingsand::light_common::{light_params, glow}

@group(#{MATERIAL_BIND_GROUP}) @binding(1) var world_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var world_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var glow_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var emission_tex: texture_2d<f32>;

const CORE_GAIN: f32 = 1.5;
const HALO_LIGHT: f32 = 0.4;
const HALO_SPILL: f32 = 0.12;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let native = light_params.native_size;
    let t = layer_texel(in.uv, native);
    let world = textureLoad(world_tex, vec2<u32>(t), 0);
    let cell = layer_cell(t, light_params.snapped_cam, native);

    let core = textureLoad(emission_tex, vec2<u32>(t), 0).rgb;
    let halo = textureLoad(glow_tex, vec2<u32>(t), 0).rgb;
    let point = glow(cell);

    let lit = max(point, max(halo.r, max(halo.g, halo.b)) * HALO_LIGHT);
    let ambient = 1.0 - light_params.darkness;
    let incident = clamp(ambient + lit, 0.0, 1.0);

    let dark = vec3<f32>(0.01, 0.012, 0.03);
    var rgb = mix(dark * world.a, world.rgb, incident);
    rgb = rgb + halo * (world.a * HALO_SPILL);
    rgb = rgb + core * (world.a * CORE_GAIN);
    return vec4<f32>(rgb, world.a);
}
