#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import fallingsand::layer_common::{layer_texel, layer_cell}
#import fallingsand::light_common::{light_params, glow}

@group(#{MATERIAL_BIND_GROUP}) @binding(1) var world_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var world_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var glow_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var emission_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var air_tex: texture_2d<f32>;

const CORE_GAIN: f32 = 3.0;
const HALO_LIGHT: f32 = 0.1;
const HALO_SPILL: f32 = 0.03;
const AIR_GAIN: f32 = 10.0;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let native = light_params.native_size;
    let t = layer_texel(in.uv, native);
    let world = textureLoad(world_tex, vec2<u32>(t), 0);
    let cell = layer_cell(t, light_params.snapped_cam, native);

    let te = vec2<u32>(t + light_params.margin);
    let core = textureLoad(emission_tex, te, 0).rgb;
    let halo = textureLoad(glow_tex, te, 0).rgb;
    let point = glow(cell);

    let lit = max(point, max(halo.r, max(halo.g, halo.b)) * HALO_LIGHT);
    let sky = 1.0 - light_params.darkness;
    let air = textureLoad(air_tex, te, 0).r;
    let ambient = clamp(air * AIR_GAIN, 0.0, 1.0) * sky;
    let incident = clamp(ambient + lit, 0.0, 1.0);

    let dark = vec3<f32>(0.01, 0.012, 0.03);
    var rgb = mix(dark * world.a, world.rgb, incident);
    rgb = rgb + halo * (world.a * HALO_SPILL);
    rgb = rgb + core * (world.a * CORE_GAIN);
    return vec4<f32>(rgb, world.a);
}
