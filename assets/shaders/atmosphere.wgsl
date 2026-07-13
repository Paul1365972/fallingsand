#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import fallingsand::celestial::unpremultiply

struct AtmosphereParams {
    color: vec4<f32>,
    sun_pos: vec2<f32>,
    moon_pos: vec2<f32>,
    sun_glow: vec4<f32>,
    moon_glow: vec4<f32>,
    horizon: f32,
    intensity: f32,
    aspect: f32,
    _pad: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: AtmosphereParams;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let distance_below_horizon = in.uv.y - params.horizon;
    let ground_alpha = smoothstep(-0.02, 0.02, distance_below_horizon);
    let haze = exp(-max(-distance_below_horizon, 0.0) * 6.0) * params.intensity;
    let atmosphere_alpha = clamp(ground_alpha + haze * (1.0 - ground_alpha), 0.0, 1.0);
    let atmosphere_color = params.color.rgb;

    let aspect_correction = vec2<f32>(params.aspect, 1.0);
    let sun_glow_alpha = params.sun_glow.w
        * exp(-length((in.uv - params.sun_pos) * aspect_correction) * 3.0);
    let moon_glow_alpha = params.moon_glow.w
        * exp(-length((in.uv - params.moon_pos) * aspect_correction) * 4.5);
    let glow_color = params.sun_glow.rgb * sun_glow_alpha
        + params.moon_glow.rgb * moon_glow_alpha;
    let glow_alpha = clamp(sun_glow_alpha + moon_glow_alpha, 0.0, 1.0);

    let alpha = glow_alpha + atmosphere_alpha * (1.0 - glow_alpha);
    let premultiplied_color = glow_color
        + atmosphere_color * atmosphere_alpha * (1.0 - glow_alpha);
    return unpremultiply(premultiplied_color, alpha);
}
