#define_import_path fallingsand::game_celestial
#import fallingsand::game_common::{layer_cell, layer_texel}
#import fallingsand::game_scene_bindings as bindings

const TAU: f32 = 6.2831853;

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn normalized_premultiplied_layer(premultiplied: vec3<f32>, alpha: f32) -> vec4<f32> {
    let coverage = clamp(alpha, 0.0, 1.0);
    return vec4<f32>(premultiplied * coverage / max(alpha, 1e-4), coverage);
}

fn star_layer_premultiplied(uv: vec2<f32>) -> vec4<f32> {
    let viewport = bindings::frame.viewport;
    let t = layer_texel(uv, viewport);
    let cell = layer_cell(t, vec2<f32>(0.0), viewport);
    let star_uv = (
        cell
        - bindings::frame.backdrop.celestial.stars.center
        + bindings::frame.backdrop.celestial.stars.scroll
    ) / bindings::frame.backdrop.celestial.stars.world_scale;
    let sample = textureSample(bindings::star_tex, bindings::star_sampler, star_uv);
    let gcell = floor(fract(star_uv) * bindings::frame.backdrop.celestial.stars.world_scale);
    let phase = hash2(gcell) * TAU;
    let cycles = round(48.0 + 119.0 * hash2(gcell + vec2<f32>(19.3, 7.1)));
    let flicker = 1.0 - 0.55 * pow(
        0.5 + 0.5 * sin(bindings::frame.backdrop.celestial.stars.sidereal * cycles * TAU + phase),
        2.0,
    );
    let magnitude = hash2(gcell + vec2<f32>(41.7, 289.3));
    let visibility = smoothstep(
        0.0,
        0.12,
        bindings::frame.backdrop.celestial.stars.star_visibility - magnitude * 0.85,
    );
    let above = 1.0 - smoothstep(
        bindings::frame.backdrop.celestial.stars.horizon - 0.04,
        bindings::frame.backdrop.celestial.stars.horizon,
        uv.y,
    );
    let alpha = clamp(sample.a * visibility * above * flicker, 0.0, 1.0);
    return vec4<f32>(sample.rgb * alpha, alpha);
}

fn position_hash(position: vec2<f32>) -> f32 {
    return fract(sin(dot(position, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn snapped_disc_position(uv: vec2<f32>, quad_size: f32) -> vec2<f32> {
    let snapped_uv = (floor(uv * quad_size) + vec2<f32>(0.5)) / quad_size;
    return vec2<f32>(snapped_uv.x - 0.5, 0.5 - snapped_uv.y) * 2.0;
}

fn disc_coverage(radius: f32, disc_radius: f32, pixel_size: f32) -> f32 {
    return clamp((disc_radius - radius) / pixel_size + 0.5, 0.0, 1.0);
}

fn aura_falloff(radius: f32, disc_radius: f32) -> f32 {
    let strength = clamp((1.0 - radius) / max(1.0 - disc_radius, 1e-4), 0.0, 1.0);
    return strength * strength;
}

fn celestial_uv(pixel: vec2<f32>, center: vec2<f32>, size: vec2<f32>) -> vec2<f32> {
    let window_center = bindings::frame.viewport.window_center;
    let screen = vec2<f32>(pixel.x - window_center.x, window_center.y - pixel.y);
    return (screen - center) / max(size, vec2<f32>(1.0)) + vec2<f32>(0.5);
}

fn sun_layer_premultiplied(pixel: vec2<f32>) -> vec4<f32> {
    let sun = bindings::frame.backdrop.celestial.sun;
    let uv = celestial_uv(
        pixel,
        bindings::frame.backdrop.celestial.sun_center,
        bindings::frame.backdrop.celestial.sun_size,
    );
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) {
        return vec4<f32>(0.0);
    }
    let position = snapped_disc_position(uv, max(sun.quad_size, 1.0));
    let pixel_size = 2.0 / max(sun.quad_size, 1.0);
    let radius = length(position);
    let coverage = disc_coverage(radius, sun.disc_radius, pixel_size);
    let normalized_radius = clamp(radius / sun.disc_radius, 0.0, 1.0);
    var color = mix(
        vec3<f32>(1.0, 0.88, 0.59),
        vec3<f32>(1.0, 0.64, 0.17),
        smoothstep(0.0, 0.5, normalized_radius),
    );
    color = mix(
        color,
        vec3<f32>(1.0, 0.22, 0.03),
        smoothstep(0.5, 1.0, normalized_radius),
    );
    color *= 1.0
        + (position_hash(floor(uv * sun.quad_size)) - 0.5)
            * 0.05
            * (1.0 - normalized_radius);
    let disc = mix(
        color,
        color * vec3<f32>(1.0, 0.74, 0.46),
        sun.redness,
    ) * 14.0;
    let aura = aura_falloff(radius, sun.disc_radius) * (1.0 - coverage) * 0.5;
    let corona = pow(clamp(1.0 - radius, 0.0, 1.0), 2.2)
        * 2.4
        * sun.occlusion
        * sun.occlusion;
    let alpha = coverage + aura + corona;
    let premultiplied = disc * coverage
        + mix(
            vec3<f32>(1.0, 0.62, 0.30),
            vec3<f32>(1.0, 0.40, 0.17),
            sun.redness,
        ) * 1.6
            * aura
        + vec3<f32>(1.0, 0.82, 0.52) * 3.0 * corona;
    return normalized_premultiplied_layer(premultiplied, alpha);
}

fn soft_blob(position: vec2<f32>, center: vec2<f32>, radius: f32) -> f32 {
    return 1.0 - smoothstep(radius * 0.45, radius, length(position - center));
}

fn quantize(value: f32, steps: f32) -> f32 {
    return floor(value * steps + 0.5) / steps;
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

fn moon_layer_premultiplied(pixel: vec2<f32>) -> vec4<f32> {
    let moon = bindings::frame.backdrop.celestial.moon;
    let uv = celestial_uv(
        pixel,
        bindings::frame.backdrop.celestial.moon_center,
        bindings::frame.backdrop.celestial.moon_size,
    );
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) {
        return vec4<f32>(0.0);
    }
    let quad_size = max(moon.quad_size, 1.0);
    let position = snapped_disc_position(uv, quad_size);
    let pixel_size = 2.0 / quad_size;
    let radius = length(position);
    let coverage = disc_coverage(radius, moon.disc_radius, pixel_size);
    let mare = quantize(mare_coverage(position), 5.0);
    let albedo = mix(
        vec3<f32>(0.66, 0.71, 0.84),
        vec3<f32>(0.30, 0.35, 0.55),
        mare,
    );
    let albedo_luminance = dot(albedo, vec3<f32>(0.3333));
    let sun_direction = normalize(moon.sun_direction + vec2<f32>(1e-5, 0.0));
    let tangent = vec2<f32>(-sun_direction.y, sun_direction.x);
    let tangent_distance = dot(position, tangent);
    let half_width = sqrt(max(
        moon.disc_radius * moon.disc_radius - tangent_distance * tangent_distance,
        0.0,
    ));
    let terminator = (1.0 - 2.0 * moon.illumination) * half_width;
    let sunlight = smoothstep(
        -pixel_size,
        pixel_size,
        dot(position, sun_direction) - terminator,
    );
    let distance_to_umbra = length(position - moon.umbra);
    let umbra_shadow = 1.0 - smoothstep(
        -0.5 * pixel_size,
        0.5 * pixel_size,
        distance_to_umbra - moon.umbra_radius,
    );
    let penumbra = 1.0 - smoothstep(
        moon.umbra_radius,
        moon.umbra_radius + 1.1,
        distance_to_umbra,
    );
    let umbra_core = smoothstep(0.1, 1.2, moon.umbra_radius - distance_to_umbra);
    let blood_moon_color = mix(
        vec3<f32>(1.05, 0.16, 0.06),
        vec3<f32>(0.32, 0.045, 0.03),
        umbra_core,
    ) * (0.5 + 0.75 * albedo_luminance);
    let reflected = mix(albedo * 0.04, albedo * 4.2, sunlight);
    let day_reflected = mix(albedo * 0.02, albedo * 0.12, sunlight);
    var light = mix(reflected, day_reflected, moon.sky_color.a);
    light *= 1.0 - 0.4 * penumbra;
    light = mix(light, blood_moon_color, umbra_shadow);
    let disc_color = moon.sky_color.rgb + light;
    let halo = aura_falloff(radius, moon.disc_radius)
        * (1.0 - coverage)
        * mix(0.4, 0.8, moon.lunar_shadow)
        * (1.0 - moon.sky_color.a);
    let halo_color = mix(
        vec3<f32>(0.55, 0.60, 0.78) * 0.7,
        vec3<f32>(1.0, 0.14, 0.05) * 1.4,
        moon.lunar_shadow,
    );
    let alpha = coverage + halo;
    return normalized_premultiplied_layer(
        disc_color * coverage + halo_color * halo,
        alpha,
    );
}

fn atmosphere_layer_premultiplied(uv: vec2<f32>) -> vec4<f32> {
    let atmosphere = bindings::frame.backdrop.celestial.atmosphere;
    let distance_below_horizon = uv.y - atmosphere.horizon;
    let ground_alpha = smoothstep(-0.02, 0.02, distance_below_horizon);
    let haze = exp(-max(-distance_below_horizon, 0.0) * 6.0) * atmosphere.intensity;
    let atmosphere_alpha = clamp(
        ground_alpha + haze * (1.0 - ground_alpha),
        0.0,
        1.0,
    );
    let aspect = vec2<f32>(atmosphere.aspect, 1.0);
    let sun_alpha = atmosphere.sun_glow.w
        * exp(-length((uv - atmosphere.sun_pos) * aspect) * 3.0);
    let moon_alpha = atmosphere.moon_glow.w
        * exp(-length((uv - atmosphere.moon_pos) * aspect) * 4.5);
    let glow_color = atmosphere.sun_glow.rgb * sun_alpha
        + atmosphere.moon_glow.rgb * moon_alpha;
    let glow_alpha = clamp(sun_alpha + moon_alpha, 0.0, 1.0);
    let alpha = glow_alpha + atmosphere_alpha * (1.0 - glow_alpha);
    let premultiplied = glow_color
        + atmosphere.color.rgb * atmosphere_alpha * (1.0 - glow_alpha);
    return normalized_premultiplied_layer(premultiplied, alpha);
}
