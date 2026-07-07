#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct MoonParams {
    sun_dir: vec2<f32>,
    illumination: f32,
    umbra: vec2<f32>,
    umbra_r: f32,
    sky_color: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: MoonParams;

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = vec2<f32>(in.uv.x - 0.5, 0.5 - in.uv.y) * 2.0;
    let r2 = dot(p, p);
    let disc = 1.0 - smoothstep(0.86, 0.94, sqrt(r2));

    let m = noise(p * 2.3 + vec2<f32>(3.7, 1.2)) * 0.6 + noise(p * 5.1) * 0.4;
    let maria = smoothstep(0.42, 0.62, m);
    let surface = mix(vec3<f32>(0.87, 0.89, 0.96), vec3<f32>(0.60, 0.64, 0.80), maria);

    let s = normalize(params.sun_dir + vec2<f32>(1e-5, 0.0));
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
    let shade = 1.0 - smoothstep(params.umbra_r - 0.14, params.umbra_r + 0.14, ud);
    let pen = 1.0 - smoothstep(params.umbra_r, params.umbra_r + 1.1, ud);
    night_col = night_col * (1.0 - 0.4 * pen);
    let blood = vec3<f32>(0.36, 0.09, 0.05);
    night_col = mix(night_col, blood, shade);

    let day_col = mix(vec3<f32>(0.009, 0.010, 0.012), surface * 0.15, lit);
    let col = mix(night_col, day_col, params.sky_color.a) + params.sky_color.rgb;
    return vec4<f32>(col, disc);
}
