#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct LightingParams {
    lights: array<vec4<f32>, 32>,
    darkness: f32,
    light_count: u32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: LightingParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var world_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var world_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let world = textureSample(world_tex, world_sampler, in.uv);
    var glow = 0.0;
    for (var i = 0u; i < params.light_count; i = i + 1u) {
        let light = params.lights[i];
        let dist = distance(in.world_position.xy, light.xy);
        let falloff = clamp(1.0 - dist / max(light.z, 1.0), 0.0, 1.0);
        glow = max(glow, falloff * falloff * light.w);
    }
    let factor = clamp(params.darkness * (1.0 - glow), 0.0, 1.0);
    let dark = vec3<f32>(0.01, 0.012, 0.03);
    let rgb = mix(world.rgb, dark, factor);
    return vec4<f32>(rgb, world.a);
}
