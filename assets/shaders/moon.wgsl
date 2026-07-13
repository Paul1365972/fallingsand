#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import fallingsand::celestial::{snapped_disc_position, disc_coverage, aura_falloff, quantize, unpremultiply}

struct MoonParams {
    sun_direction: vec2<f32>,
    illumination: f32,
    umbra: vec2<f32>,
    umbra_radius: f32,
    sky_color: vec4<f32>,
    quad_size: f32,
    disc_radius: f32,
    lunar_shadow: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: MoonParams;

fn soft_blob(position: vec2<f32>, center: vec2<f32>, radius: f32) -> f32 {
    return 1.0 - smoothstep(radius * 0.45, radius, length(position - center));
}

fn mare_coverage(position: vec2<f32>) -> f32 {
    var coverage = soft_blob(position, vec2<f32>(-0.18, 0.20), 0.44);
    coverage = max(coverage, soft_blob(position, vec2<f32>(0.24, 0.04), 0.42));
    coverage = max(coverage, soft_blob(position, vec2<f32>(0.34, -0.26), 0.28));
    coverage = max(coverage, soft_blob(position, vec2<f32>(-0.30, -0.34), 0.12));
    coverage = max(coverage, soft_blob(position, vec2<f32>(-0.02, -0.16), 0.10));
    coverage += sin(position.x * 6.0 + sin(position.y * 5.0)) * 0.05;
    return clamp(coverage, 0.0, 1.0);
}

fn moon_albedo(position: vec2<f32>) -> vec3<f32> {
    let highland = vec3<f32>(0.66, 0.71, 0.84);
    let mare = vec3<f32>(0.30, 0.35, 0.55);
    return mix(highland, mare, quantize(mare_coverage(position), 5.0));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let quad_size = max(params.quad_size, 1.0);
    let position = snapped_disc_position(in.uv, quad_size);
    let pixel_size = 2.0 / quad_size;
    let radius = length(position);
    let coverage = disc_coverage(radius, params.disc_radius, pixel_size);
    let albedo = moon_albedo(position);
    let albedo_luminance = dot(albedo, vec3<f32>(0.3333));

    let sun_direction = normalize(params.sun_direction + vec2<f32>(1e-5, 0.0));
    let tangent = vec2<f32>(-sun_direction.y, sun_direction.x);
    let sunward_distance = dot(position, sun_direction);
    let tangent_distance = dot(position, tangent);
    let phase_half_width = sqrt(max(
        params.disc_radius * params.disc_radius - tangent_distance * tangent_distance,
        0.0
    ));
    let terminator_position = (1.0 - 2.0 * params.illumination) * phase_half_width;
    let sunlight = smoothstep(
        -pixel_size,
        pixel_size,
        sunward_distance - terminator_position
    );

    let distance_to_umbra = length(position - params.umbra);
    let umbra_shadow = 1.0 - smoothstep(
        -0.5 * pixel_size,
        0.5 * pixel_size,
        distance_to_umbra - params.umbra_radius
    );
    let penumbra = 1.0 - smoothstep(
        params.umbra_radius,
        params.umbra_radius + 1.1,
        distance_to_umbra
    );
    let umbra_core = smoothstep(0.1, 1.2, params.umbra_radius - distance_to_umbra);
    let blood_moon_color = mix(
        vec3<f32>(1.05, 0.16, 0.06),
        vec3<f32>(0.32, 0.045, 0.03),
        umbra_core
    ) * (0.5 + 0.75 * albedo_luminance);

    let night_reflection = mix(albedo * 0.04, albedo * 4.2, sunlight);
    let day_reflection = mix(albedo * 0.02, albedo * 0.12, sunlight);
    var reflected_light = mix(night_reflection, day_reflection, params.sky_color.a);
    reflected_light *= 1.0 - 0.4 * penumbra;
    reflected_light = mix(reflected_light, blood_moon_color, umbra_shadow);

    let disc_color = params.sky_color.rgb + reflected_light;

    let halo_strength = aura_falloff(radius, params.disc_radius) * (1.0 - coverage);
    let halo_color = mix(
        vec3<f32>(0.55, 0.60, 0.78) * 0.7,
        vec3<f32>(1.0, 0.14, 0.05) * 1.4,
        params.lunar_shadow
    );
    let halo_alpha = halo_strength
        * mix(0.4, 0.8, params.lunar_shadow)
        * (1.0 - params.sky_color.a);
    let alpha = coverage + halo_alpha;
    let premultiplied_color = disc_color * coverage + halo_color * halo_alpha;
    return unpremultiply(premultiplied_color, alpha);
}
