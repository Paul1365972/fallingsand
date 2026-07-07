#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct MoonParams {
    sun_direction: vec2<f32>,
    illumination: f32,
    umbra: vec2<f32>,
    umbra_radius: f32,
    sky_color: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: MoonParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var tex_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let sample = textureSample(tex, tex_sampler, in.uv);
    let p = vec2<f32>(in.uv.x - 0.5, 0.5 - in.uv.y) * 2.0;
    let surface = sample.rgb;

    let s = normalize(params.sun_direction + vec2<f32>(1e-5, 0.0));
    let sp = vec2<f32>(-s.y, s.x);
    let along = dot(p, s);
    let perp = dot(p, sp);
    let hw = sqrt(max(0.81 - perp * perp, 0.0));
    let term = (1.0 - 2.0 * params.illumination) * hw;
    let lit = clamp((along - term) / 0.10, 0.0, 1.0);

    let dark_col = vec3<f32>(0.004, 0.005, 0.008);
    let albedo = mix(dark_col, surface, lit);
    var night_col = albedo * 4.2;

    let ud = length(p - params.umbra);
    let shade = 1.0 - smoothstep(params.umbra_radius - 0.14, params.umbra_radius + 0.14, ud);
    let pen = 1.0 - smoothstep(params.umbra_radius, params.umbra_radius + 1.1, ud);
    night_col = night_col * (1.0 - 0.4 * pen);
    let blood = vec3<f32>(0.36, 0.09, 0.05);
    night_col = mix(night_col, blood, shade);

    let day_col = mix(vec3<f32>(0.009, 0.010, 0.012), surface * 0.15, lit);
    let col = mix(night_col, day_col, params.sky_color.a) + params.sky_color.rgb;
    return vec4<f32>(col, sample.a);
}
