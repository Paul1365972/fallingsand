#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct MoonParams {
    sun_direction: vec2<f32>,
    illumination: f32,
    umbra: vec2<f32>,
    umbra_radius: f32,
    sky_color: vec4<f32>,
    quad_size: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: MoonParams;

const DISC_RADIUS: f32 = 0.92;

fn crater(p: vec2<f32>, center: vec2<f32>, radius: f32) -> f32 {
    let distance = length(p - center) / radius;
    let bowl = 1.0 - smoothstep(0.0, 0.78, distance);
    let rim = smoothstep(0.68, 0.86, distance) * (1.0 - smoothstep(0.88, 1.06, distance));
    return rim * 0.16 - bowl * 0.22;
}

fn surface(p: vec2<f32>) -> vec3<f32> {
    var shade = 0.48 + sin(p.x * 17.0 + sin(p.y * 9.0)) * 0.035;
    shade += crater(p, vec2<f32>(-0.31, 0.24), 0.24);
    shade += crater(p, vec2<f32>(0.27, 0.38), 0.14);
    shade += crater(p, vec2<f32>(0.36, -0.22), 0.23);
    shade += crater(p, vec2<f32>(-0.18, -0.42), 0.12);
    shade += crater(p, vec2<f32>(0.02, 0.02), 0.09);
    shade = floor(clamp(shade, 0.0, 1.0) * 6.0 + 0.5) / 6.0;
    let low = vec3<f32>(0.314, 0.371, 0.604);
    let high = vec3<f32>(0.604, 0.651, 0.768);
    return mix(low, high, shade);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let grid_size = max(params.quad_size, 1.0);
    let grid = (floor(in.uv * grid_size) + vec2<f32>(0.5)) / grid_size;
    let p = vec2<f32>(grid.x - 0.5, 0.5 - grid.y) * 2.0;
    let pixel = 2.0 / grid_size;

    let distance = length(p);
    let cover = clamp((DISC_RADIUS - distance) / pixel + 0.5, 0.0, 1.0);
    let inner = p * (min(distance, DISC_RADIUS - 0.5 * pixel) / max(distance, 1e-5));
    let albedo = surface(inner);

    let sun = normalize(params.sun_direction + vec2<f32>(1e-5, 0.0));
    let perpendicular = vec2<f32>(-sun.y, sun.x);
    let along = dot(p, sun);
    let across = dot(p, perpendicular);
    let half_width = sqrt(max(DISC_RADIUS * DISC_RADIUS - across * across, 0.0));
    let terminator = (1.0 - 2.0 * params.illumination) * half_width;
    let lit = smoothstep(-0.5 * pixel, 0.5 * pixel, along - terminator);

    let shadow_distance = length(p - params.umbra);
    let shade = 1.0 - smoothstep(-0.5 * pixel, 0.5 * pixel, shadow_distance - params.umbra_radius);
    let penumbra = 1.0 - smoothstep(params.umbra_radius, params.umbra_radius + 1.1, shadow_distance);
    let core = smoothstep(0.1, 1.2, params.umbra_radius - shadow_distance);
    let luma = dot(albedo, vec3<f32>(0.333, 0.334, 0.333));
    let blood = mix(vec3<f32>(0.62, 0.20, 0.07), vec3<f32>(0.26, 0.045, 0.035), core)
        * (0.5 + 1.2 * luma);

    let dark = vec3<f32>(0.004, 0.005, 0.008);
    var night = mix(dark, albedo, lit) * 4.2;
    night *= 1.0 - 0.45 * penumbra;
    night = mix(night, blood * 1.2, shade);

    var day = mix(vec3<f32>(0.009, 0.010, 0.012), albedo * 0.15, lit);
    day *= 1.0 - 0.5 * penumbra;
    day = mix(day, blood * 0.5, shade);

    let color = mix(night, day, params.sky_color.a)
        + params.sky_color.rgb * (1.0 - 0.75 * shade);
    return vec4<f32>(color, cover);
}
