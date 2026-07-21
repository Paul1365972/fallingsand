#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import fallingsand::common::{PixelViewport, pcg, vnoise}

const TAU: f32 = 6.2831853;
const LIGHT_FIELD_DOWNSCALE: f32 = 4.0;
const CAVE_DARK: vec3<f32> = vec3<f32>(0.01, 0.012, 0.03);

struct SunFrame {
    redness: f32,
    occlusion: f32,
    quad_size: f32,
    disc_radius: f32,
}

struct MoonFrame {
    sun_direction: vec2<f32>,
    illumination: f32,
    umbra: vec2<f32>,
    umbra_radius: f32,
    sky_color: vec4<f32>,
    quad_size: f32,
    disc_radius: f32,
    lunar_shadow: f32,
}

struct StarfieldFrame {
    center: vec2<f32>,
    scroll: vec2<f32>,
    world_scale: f32,
    star_visibility: f32,
    horizon: f32,
    sidereal: f32,
}

struct AtmosphereFrame {
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

struct CelestialFrame {
    sun: SunFrame,
    moon: MoonFrame,
    stars: StarfieldFrame,
    atmosphere: AtmosphereFrame,
    sun_center: vec2<f32>,
    sun_size: vec2<f32>,
    moon_center: vec2<f32>,
    moon_size: vec2<f32>,
}

struct LightingFrame {
    darkness: f32,
    light_count: u32,
    snapped_cam: vec2<f32>,
    margin: vec2<f32>,
}

struct WallFrame {
    base_color: vec4<f32>,
    world_offset: vec2<f32>,
}

struct SilhouetteFrame {
    color: vec4<f32>,
    snapped_cam: vec2<f32>,
    base: f32,
    amplitude: f32,
    inv_wavelength: f32,
    seed: f32,
}

struct WorldFrame {
    lighting: LightingFrame,
    wall: WallFrame,
    far: SilhouetteFrame,
    near: SilhouetteFrame,
    wall_snapped: vec2<f32>,
}

struct BackdropFrame {
    celestial: CelestialFrame,
    star_offset: vec2<f32>,
    far_offset: vec2<f32>,
    near_offset: vec2<f32>,
    wall_offset: vec2<f32>,
}

struct SceneFrame {
    viewport: PixelViewport,
    world: WorldFrame,
    backdrop: BackdropFrame,
    world_offset: vec2<f32>,
    clear_color: vec4<f32>,
    backdrop_ready: u32,
}

struct PointLight {
    center: vec2<f32>,
    radius: f32,
    intensity: f32,
}

@group(0) @binding(0) var<uniform> frame: SceneFrame;
@group(0) @binding(1) var<storage, read> point_lights: array<PointLight>;
@group(0) @binding(2) var world_tex: texture_2d<f32>;
@group(0) @binding(3) var emission_tex: texture_2d<f32>;
@group(0) @binding(4) var light_tex: texture_2d<f32>;
@group(0) @binding(5) var linear_sampler: sampler;
@group(0) @binding(6) var star_tex: texture_2d<f32>;
@group(0) @binding(7) var star_sampler: sampler;

fn layer_uv(pixel: vec2<f32>, offset: vec2<f32>) -> vec2<f32> {
    let screen_offset = vec2<f32>(-offset.x, offset.y);
    return (pixel + screen_offset + (frame.viewport.physical_size - frame.viewport.window_size) * 0.5) / frame.viewport.physical_size;
}

fn layer_texel(uv: vec2<f32>) -> vec2<f32> {
    return min(floor(uv * frame.viewport.native_size), frame.viewport.native_size - vec2<f32>(1.0));
}

fn layer_cell(texel: vec2<f32>, snapped: vec2<f32>) -> vec2<f32> {
    return snapped + vec2<f32>(
        texel.x + 0.5 - frame.viewport.native_size.x * 0.5,
        frame.viewport.native_size.y * 0.5 - texel.y - 0.5
    );
}

fn composite_over_opaque(dst: vec3<f32>, src_premultiplied: vec4<f32>) -> vec3<f32> {
    return src_premultiplied.rgb + dst * (1.0 - src_premultiplied.a);
}

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn normalized_premultiplied_layer(premultiplied: vec3<f32>, alpha: f32) -> vec4<f32> {
    let coverage = clamp(alpha, 0.0, 1.0);
    return vec4<f32>(premultiplied * coverage / max(alpha, 1e-4), coverage);
}

fn star_layer_premultiplied(uv: vec2<f32>) -> vec4<f32> {
    let texel = layer_texel(uv);
    let cell = layer_cell(texel, vec2<f32>(0.0));
    let stars = frame.backdrop.celestial.stars;
    let star_uv = (cell - stars.center + stars.scroll) / stars.world_scale;
    let sample = textureSample(star_tex, star_sampler, star_uv);
    let grid_cell = floor(fract(star_uv) * stars.world_scale);
    let phase = hash2(grid_cell) * TAU;
    let cycles = round(48.0 + 119.0 * hash2(grid_cell + vec2<f32>(19.3, 7.1)));
    let flicker = 1.0 - 0.55 * pow(
        0.5 + 0.5 * sin(stars.sidereal * cycles * TAU + phase),
        2.0,
    );
    let magnitude = hash2(grid_cell + vec2<f32>(41.7, 289.3));
    let visibility = smoothstep(0.0, 0.12, stars.star_visibility - magnitude * 0.85);
    let above = 1.0 - smoothstep(stars.horizon - 0.04, stars.horizon, uv.y);
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
    let screen = vec2<f32>(pixel.x - frame.viewport.window_center.x, frame.viewport.window_center.y - pixel.y);
    return (screen - center) / max(size, vec2<f32>(1.0)) + vec2<f32>(0.5);
}

fn sun_layer_premultiplied(pixel: vec2<f32>) -> vec4<f32> {
    let celestial = frame.backdrop.celestial;
    let sun = celestial.sun;
    let uv = celestial_uv(pixel, celestial.sun_center, celestial.sun_size);
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
    color = mix(color, vec3<f32>(1.0, 0.22, 0.03), smoothstep(0.5, 1.0, normalized_radius));
    color *= 1.0 + (position_hash(floor(uv * sun.quad_size)) - 0.5) * 0.05 * (1.0 - normalized_radius);
    let disc = mix(color, color * vec3<f32>(1.0, 0.74, 0.46), sun.redness) * 14.0;
    let aura = aura_falloff(radius, sun.disc_radius) * (1.0 - coverage) * 0.5;
    let corona = pow(clamp(1.0 - radius, 0.0, 1.0), 2.2) * 2.4 * sun.occlusion * sun.occlusion;
    let alpha = coverage + aura + corona;
    let premultiplied = disc * coverage
        + mix(vec3<f32>(1.0, 0.62, 0.30), vec3<f32>(1.0, 0.40, 0.17), sun.redness) * 1.6 * aura
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
    let celestial = frame.backdrop.celestial;
    let moon = celestial.moon;
    let uv = celestial_uv(pixel, celestial.moon_center, celestial.moon_size);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) {
        return vec4<f32>(0.0);
    }
    let quad_size = max(moon.quad_size, 1.0);
    let position = snapped_disc_position(uv, quad_size);
    let pixel_size = 2.0 / quad_size;
    let radius = length(position);
    let coverage = disc_coverage(radius, moon.disc_radius, pixel_size);
    let mare = quantize(mare_coverage(position), 5.0);
    let albedo = mix(vec3<f32>(0.66, 0.71, 0.84), vec3<f32>(0.30, 0.35, 0.55), mare);
    let albedo_luminance = dot(albedo, vec3<f32>(0.3333));
    let sun_direction = normalize(moon.sun_direction + vec2<f32>(1e-5, 0.0));
    let tangent = vec2<f32>(-sun_direction.y, sun_direction.x);
    let tangent_distance = dot(position, tangent);
    let half_width = sqrt(max(moon.disc_radius * moon.disc_radius - tangent_distance * tangent_distance, 0.0));
    let terminator = (1.0 - 2.0 * moon.illumination) * half_width;
    let sunlight = smoothstep(-pixel_size, pixel_size, dot(position, sun_direction) - terminator);
    let distance_to_umbra = length(position - moon.umbra);
    let umbra_shadow = 1.0 - smoothstep(-0.5 * pixel_size, 0.5 * pixel_size, distance_to_umbra - moon.umbra_radius);
    let penumbra = 1.0 - smoothstep(moon.umbra_radius, moon.umbra_radius + 1.1, distance_to_umbra);
    let umbra_core = smoothstep(0.1, 1.2, moon.umbra_radius - distance_to_umbra);
    let blood_moon_color = mix(vec3<f32>(1.05, 0.16, 0.06), vec3<f32>(0.32, 0.045, 0.03), umbra_core)
        * (0.5 + 0.75 * albedo_luminance);
    let reflected = mix(albedo * 0.04, albedo * 4.2, sunlight);
    let day_reflected = mix(albedo * 0.02, albedo * 0.12, sunlight);
    var light = mix(reflected, day_reflected, moon.sky_color.a);
    light *= 1.0 - 0.4 * penumbra;
    light = mix(light, blood_moon_color, umbra_shadow);
    let disc_color = moon.sky_color.rgb + light;
    let halo = aura_falloff(radius, moon.disc_radius) * (1.0 - coverage)
        * mix(0.4, 0.8, moon.lunar_shadow) * (1.0 - moon.sky_color.a);
    let halo_color = mix(vec3<f32>(0.55, 0.60, 0.78) * 0.7, vec3<f32>(1.0, 0.14, 0.05) * 1.4, moon.lunar_shadow);
    let alpha = coverage + halo;
    return normalized_premultiplied_layer(disc_color * coverage + halo_color * halo, alpha);
}

fn atmosphere_layer_premultiplied(uv: vec2<f32>) -> vec4<f32> {
    let atmosphere = frame.backdrop.celestial.atmosphere;
    let distance_below_horizon = uv.y - atmosphere.horizon;
    let ground_alpha = smoothstep(-0.02, 0.02, distance_below_horizon);
    let haze = exp(-max(-distance_below_horizon, 0.0) * 6.0) * atmosphere.intensity;
    let atmosphere_alpha = clamp(ground_alpha + haze * (1.0 - ground_alpha), 0.0, 1.0);
    let aspect = vec2<f32>(atmosphere.aspect, 1.0);
    let sun_alpha = atmosphere.sun_glow.w * exp(-length((uv - atmosphere.sun_pos) * aspect) * 3.0);
    let moon_alpha = atmosphere.moon_glow.w * exp(-length((uv - atmosphere.moon_pos) * aspect) * 4.5);
    let glow_color = atmosphere.sun_glow.rgb * sun_alpha + atmosphere.moon_glow.rgb * moon_alpha;
    let glow_alpha = clamp(sun_alpha + moon_alpha, 0.0, 1.0);
    let alpha = glow_alpha + atmosphere_alpha * (1.0 - glow_alpha);
    let premultiplied = glow_color + atmosphere.color.rgb * atmosphere_alpha * (1.0 - glow_alpha);
    return normalized_premultiplied_layer(premultiplied, alpha);
}

fn silhouette_hash(position: f32, seed: f32) -> f32 {
    let value = pcg(bitcast<u32>(i32(position)) ^ (u32(seed) * 374761393u));
    return f32(value) / 4294967296.0;
}

fn silhouette_noise(position: f32, seed: f32) -> f32 {
    let integer = floor(position);
    let fraction = position - integer;
    let blend = fraction * fraction * (3.0 - 2.0 * fraction);
    return mix(silhouette_hash(integer, seed), silhouette_hash(integer + 1.0, seed), blend);
}

fn silhouette_fbm(position: f32, seed: f32) -> f32 {
    return silhouette_noise(position, seed) * 0.55
        + silhouette_noise(position * 2.03 + 13.7, seed) * 0.3
        + silhouette_noise(position * 4.01 + 41.3, seed) * 0.15;
}

fn silhouette_height(cell_x: f32, params: SilhouetteFrame) -> f32 {
    let noise = silhouette_fbm(cell_x * params.inv_wavelength, params.seed);
    return params.base + params.amplitude * (noise * 2.0 - 1.0);
}

fn silhouette_layer_premultiplied(uv: vec2<f32>, params: SilhouetteFrame) -> vec4<f32> {
    let cell = layer_cell(layer_texel(uv), params.snapped_cam);
    let alpha = select(0.0, params.color.a, cell.y < silhouette_height(cell.x, params));
    return vec4<f32>(params.color.rgb * alpha, alpha);
}

fn point_glow(position: vec2<f32>) -> f32 {
    var result = 0.0;
    for (var i = 0u; i < frame.world.lighting.light_count; i += 1u) {
        let light = point_lights[i];
        let falloff = clamp(
            1.0 - distance(position, floor(light.center) + vec2<f32>(0.5)) / max(light.radius, 1.0),
            0.0,
            1.0,
        );
        result = max(result, falloff * falloff * light.intensity);
    }
    return result;
}

fn wall_layer_premultiplied(uv: vec2<f32>) -> vec4<f32> {
    let cell = layer_cell(layer_texel(uv), frame.world.wall_snapped);
    let world = cell + frame.world.wall.world_offset;
    let n = vnoise(cell * 0.11) * 0.55
        + vnoise(cell * 0.2233 + vec2<f32>(13.7, 41.3)) * 0.3
        + vnoise(cell * 0.4411 + vec2<f32>(71.9, 7.5)) * 0.15;
    let step_value = min(u32(n * 4.0), 3u);
    var rgb = frame.world.wall.base_color.rgb * (0.7 + 0.15 * f32(step_value));
    rgb = mix(rgb, CAVE_DARK, clamp(frame.world.lighting.darkness * (1.0 - point_glow(world)), 0.0, 1.0));
    let alpha = (1.0 - smoothstep(-24.0, 8.0, world.y)) * frame.world.wall.base_color.a;
    return vec4<f32>(rgb * alpha, alpha);
}

fn backdrop_color(pixel: vec2<f32>) -> vec3<f32> {
    var color = frame.clear_color.rgb;
    color = composite_over_opaque(color, star_layer_premultiplied(layer_uv(pixel, frame.backdrop.star_offset)));
    color = composite_over_opaque(color, sun_layer_premultiplied(pixel));
    color = composite_over_opaque(color, moon_layer_premultiplied(pixel));
    color = composite_over_opaque(color, atmosphere_layer_premultiplied(layer_uv(pixel, vec2<f32>(0.0))));
    color = composite_over_opaque(color, silhouette_layer_premultiplied(layer_uv(pixel, frame.backdrop.far_offset), frame.world.far));
    color = composite_over_opaque(color, silhouette_layer_premultiplied(layer_uv(pixel, frame.backdrop.near_offset), frame.world.near));
    return composite_over_opaque(color, wall_layer_premultiplied(layer_uv(pixel, frame.backdrop.wall_offset)));
}

fn lit_world_premultiplied(pixel: vec2<f32>) -> vec4<f32> {
    let uv = layer_uv(pixel, frame.world_offset);
    let texel = layer_texel(uv);
    let position = vec2<u32>(texel);
    let world = textureLoad(world_tex, position, 0);
    let cell = layer_cell(texel, frame.world.lighting.snapped_cam);
    let extended_texel = texel + frame.world.lighting.margin;
    let core = textureLoad(emission_tex, vec2<u32>(extended_texel), 0).rgb;
    let field_size = vec2<f32>(textureDimensions(light_tex)) * LIGHT_FIELD_DOWNSCALE;
    let field = textureSample(light_tex, linear_sampler, (extended_texel + vec2<f32>(0.5)) / field_size);
    let halo = field.rgb;
    let lit = max(point_glow(cell), max(halo.r, max(halo.g, halo.b)) * 0.1);
    let ambient = clamp(field.a * 10.0, 0.0, 1.0) * (1.0 - frame.world.lighting.darkness);
    let incident = clamp(ambient + lit, 0.0, 1.0);
    var rgb = mix(CAVE_DARK * world.a, world.rgb, incident);
    rgb += halo * (world.a * 0.03) + core * (world.a * 3.0);
    return vec4<f32>(rgb, world.a);
}

@fragment
fn composite_fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let pixel = in.uv * frame.viewport.window_size;
    var backdrop = frame.clear_color.rgb;
    if frame.backdrop_ready != 0u {
        backdrop = backdrop_color(pixel);
    }
    let world = lit_world_premultiplied(pixel);
    return vec4<f32>(world.rgb + backdrop * (1.0 - world.a), 1.0);
}
