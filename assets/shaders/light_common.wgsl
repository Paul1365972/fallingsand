#define_import_path fallingsand::light_common

struct LightingParams {
    lights: array<vec4<f32>, 32>,
    darkness: f32,
    light_count: u32,
    snapped_cam: vec2<f32>,
    native_size: vec2<f32>,
    emissive_origin: vec2<f32>,
    emissive_size: vec2<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> light_params: LightingParams;

fn glow(world: vec2<f32>) -> f32 {
    var g = 0.0;
    for (var i = 0u; i < light_params.light_count; i = i + 1u) {
        let light = light_params.lights[i];
        let lp = floor(light.xy) + vec2<f32>(0.5);
        let dist = distance(world, lp);
        let falloff = clamp(1.0 - dist / max(light.z, 1.0), 0.0, 1.0);
        g = max(g, falloff * falloff * light.w);
    }
    return g;
}
