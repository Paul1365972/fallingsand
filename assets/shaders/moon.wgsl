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
    let dims = vec2<f32>(textureDimensions(tex));
    let snapped = (floor(in.uv * dims) + vec2<f32>(0.5, 0.5)) / dims;
    let p = vec2<f32>(snapped.x - 0.5, 0.5 - snapped.y) * 2.0;
    let surface = sample.rgb;

    let s = normalize(params.sun_direction + vec2<f32>(1e-5, 0.0));
    let sp = vec2<f32>(-s.y, s.x);
    let along = dot(p, s);
    let perp = dot(p, sp);
    let hw = sqrt(max(0.81 - perp * perp, 0.0));
    let term = (1.0 - 2.0 * params.illumination) * hw;
    let texel = 2.0 / dims.x;
    let lit = smoothstep(-0.5 * texel, 0.5 * texel, along - term);

    let ud = length(p - params.umbra);
    let shade = 1.0 - smoothstep(-0.5 * texel, 0.5 * texel, ud - params.umbra_radius);
    let pen = 1.0 - smoothstep(params.umbra_radius, params.umbra_radius + 1.1, ud);
    let core = smoothstep(0.1, 1.2, params.umbra_radius - ud);
    let luma = dot(surface, vec3<f32>(0.333, 0.334, 0.333));
    let blood = mix(vec3<f32>(0.62, 0.20, 0.07), vec3<f32>(0.26, 0.045, 0.035), core)
        * (0.5 + 1.2 * luma);

    let dark_col = vec3<f32>(0.004, 0.005, 0.008);
    let albedo = mix(dark_col, surface, lit);
    var night_col = albedo * 4.2;
    night_col = night_col * (1.0 - 0.45 * pen);
    night_col = mix(night_col, blood * 1.2, shade);

    var day_col = mix(vec3<f32>(0.009, 0.010, 0.012), surface * 0.15, lit);
    day_col = day_col * (1.0 - 0.5 * pen);
    day_col = mix(day_col, blood * 0.5, shade);

    let col = mix(night_col, day_col, params.sky_color.a)
        + params.sky_color.rgb * (1.0 - 0.75 * shade);
    return vec4<f32>(col, sample.a);
}
