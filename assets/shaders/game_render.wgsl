#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

const MAX_LIGHTS: u32 = 256u;
const TAP_RADIUS: i32 = 13;
const TAP_VEC4S: u32 = 7u;
const CHUNK_SIZE: f32 = 64.0;
const LIGHT_FIELD_DOWNSCALE: f32 = 4.0;
const CAVE_DARK: vec3<f32> = vec3<f32>(0.01, 0.012, 0.03);
const TAU: f32 = 6.2831853;

struct LightingParams {
    lights: array<vec4<f32>, MAX_LIGHTS>,
    darkness: f32,
    light_count: u32,
    snapped_cam: vec2<f32>,
    margin: vec2<f32>,
}

struct SunParams {
    redness: f32,
    occlusion: f32,
    quad_size: f32,
    disc_radius: f32,
}

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

struct StarfieldParams {
    center: vec2<f32>,
    scroll: vec2<f32>,
    world_scale: f32,
    star_visibility: f32,
    horizon: f32,
    sidereal: f32,
}

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

struct WallParams {
    base_color: vec4<f32>,
    world_offset: vec2<f32>,
}

struct SilhouetteParams {
    color: vec4<f32>,
    snapped_cam: vec2<f32>,
    base: f32,
    amp: f32,
    inv_wavelength: f32,
    seed: f32,
}

struct FrameUniform {
    lighting: LightingParams,
    sun: SunParams,
    moon: MoonParams,
    stars: StarfieldParams,
    atmosphere: AtmosphereParams,
    wall: WallParams,
    far: SilhouetteParams,
    near: SilhouetteParams,
    world_snapped: vec2<f32>,
    wall_snapped: vec2<f32>,
    native_size: vec2<f32>,
    window_size: vec2<f32>,
    sun_center: vec2<f32>,
    sun_size: vec2<f32>,
    moon_center: vec2<f32>,
    moon_size: vec2<f32>,
    world_offset: vec2<f32>,
    star_offset: vec2<f32>,
    far_offset: vec2<f32>,
    near_offset: vec2<f32>,
    wall_offset: vec2<f32>,
    clear_color: vec4<f32>,
    scale: f32,
    time: f32,
    sky_synced: u32,
}

struct ChunkInstance {
    world_origin: vec2<f32>,
    atlas_origin: vec2<u32>,
}

struct QuadInstance {
    center: vec2<f32>,
    size: vec2<f32>,
    color: vec4<f32>,
}

struct LineInstance {
    a: vec2<f32>,
    b: vec2<f32>,
    color: vec4<f32>,
}

struct LightBlurParams {
    glow_weights: array<vec4<f32>, TAP_VEC4S>,
    air_weights: array<vec4<f32>, TAP_VEC4S>,
}

@group(0) @binding(0) var<uniform> frame: FrameUniform;
@group(0) @binding(1) var<storage, read> chunks: array<ChunkInstance>;
@group(0) @binding(2) var<storage, read> quads: array<QuadInstance>;
@group(0) @binding(3) var<storage, read> lines: array<LineInstance>;
@group(0) @binding(4) var atlas: texture_2d<u32>;
@group(0) @binding(5) var palette: texture_2d<f32>;
@group(0) @binding(6) var emissive_palette: texture_2d<f32>;
@group(0) @binding(7) var world_tex: texture_2d<f32>;
@group(0) @binding(8) var emission_tex: texture_2d<f32>;
@group(0) @binding(9) var light_source_tex: texture_2d<f32>;
@group(0) @binding(10) var light_temp_tex: texture_2d<f32>;
@group(0) @binding(11) var light_tex: texture_2d<f32>;
@group(0) @binding(12) var linear_sampler: sampler;
@group(0) @binding(13) var star_tex: texture_2d<f32>;
@group(0) @binding(14) var star_sampler: sampler;
@group(0) @binding(15) var<uniform> blur_params: LightBlurParams;

struct RasterOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) atlas_position: vec2<f32>,
    @location(1) world_position: vec2<f32>,
}

struct ColorOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

fn quad_corner(vertex: u32) -> vec2<f32> {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0)
    );
    return corners[vertex];
}

fn world_clip(world: vec2<f32>, size: vec2<f32>) -> vec4<f32> {
    let relative = world - frame.world_snapped;
    return vec4<f32>(relative.x * 2.0 / size.x, relative.y * 2.0 / size.y, 0.0, 1.0);
}

@vertex
fn chunk_vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> RasterOutput {
    let corner = quad_corner(vertex);
    let item = chunks[instance];
    let world = item.world_origin + corner * CHUNK_SIZE;
    var out: RasterOutput;
    out.clip_position = world_clip(world, frame.native_size);
    out.atlas_position = vec2<f32>(item.atlas_origin) + corner * CHUNK_SIZE;
    out.world_position = world;
    return out;
}

@vertex
fn emissive_vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> RasterOutput {
    let corner = quad_corner(vertex);
    let item = chunks[instance];
    let world = item.world_origin + corner * CHUNK_SIZE;
    let size = frame.native_size + frame.lighting.margin * 2.0;
    var out: RasterOutput;
    out.clip_position = world_clip(world, size);
    out.atlas_position = vec2<f32>(item.atlas_origin) + corner * CHUNK_SIZE;
    out.world_position = world;
    return out;
}

fn cell_entry(position: vec2<f32>) -> vec4<u32> {
    let dims = vec2<u32>(textureDimensions(atlas));
    let p = min(vec2<u32>(position), dims - vec2<u32>(1u));
    return textureLoad(atlas, p, 0);
}

@fragment
fn chunk_fragment(in: RasterOutput) -> @location(0) vec4<f32> {
    let cell = cell_entry(in.atlas_position);
    let material = cell.r | (cell.g << 8u);
    return textureLoad(palette, vec2<u32>(material, cell.b & 15u), 0);
}

fn pcg(v: u32) -> u32 {
    var x = v * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return (x >> 22u) ^ x;
}

fn cell_hash(cell: vec2<i32>) -> f32 {
    let h = pcg(bitcast<u32>(cell.x) * 1597334677u ^ bitcast<u32>(cell.y) * 3812015801u);
    return f32(h) / 4294967295.0;
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = cell_hash(i);
    let b = cell_hash(i + vec2<i32>(1, 0));
    let c = cell_hash(i + vec2<i32>(0, 1));
    let d = cell_hash(i + vec2<i32>(1, 1));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

@fragment
fn emissive_fragment(in: RasterOutput) -> @location(0) vec4<f32> {
    let cell = cell_entry(in.atlas_position);
    let material = cell.r | (cell.g << 8u);
    let shade = cell.b & 15u;
    let entry = textureLoad(emissive_palette, vec2<u32>(material, shade), 0);
    var emission = entry.rgb;
    if entry.a > 0.0 {
        let coarse = vnoise(in.world_position * (1.0 / 18.0) + vec2<f32>(0.0, -frame.time * 0.9));
        let fine = vnoise(in.world_position * (1.0 / 6.0) + vec2<f32>(0.0, -frame.time * 1.9));
        let n = mix(coarse, fine, 0.35) * 2.0 - 1.0;
        emission *= max(0.0, 1.0 + entry.a * n);
    }
    let air = 1.0 - textureLoad(palette, vec2<u32>(material, shade), 0).a;
    return vec4<f32>(emission, air);
}

@vertex
fn quad_vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> ColorOutput {
    let corner = quad_corner(vertex);
    let item = quads[instance];
    let world = item.center + (corner - vec2<f32>(0.5)) * item.size;
    var out: ColorOutput;
    out.clip_position = world_clip(world, frame.native_size);
    out.color = item.color;
    return out;
}

@fragment
fn quad_fragment(in: ColorOutput) -> @location(0) vec4<f32> {
    return in.color;
}

@fragment
fn downsample_fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let out_size = vec2<u32>(textureDimensions(emission_tex)) / 4u;
    let p = min(vec2<u32>(floor(in.uv * vec2<f32>(out_size))), out_size - vec2<u32>(1u));
    let base = p * 4u;
    var value = vec4<f32>(0.0);
    for (var y = 0u; y < 4u; y += 1u) {
        for (var x = 0u; x < 4u; x += 1u) {
            value += textureLoad(emission_tex, base + vec2<u32>(x, y), 0);
        }
    }
    return value * (1.0 / 16.0);
}

fn blur_tap(source: texture_2d<f32>, center: vec2<i32>, dir: vec2<i32>, d: i32) -> vec4<f32> {
    let dims = vec2<i32>(textureDimensions(source));
    let p = clamp(center + dir * d, vec2<i32>(0), dims - vec2<i32>(1));
    return textureLoad(source, vec2<u32>(p), 0);
}

fn blur_value(source: texture_2d<f32>, uv: vec2<f32>, dir: vec2<i32>) -> vec4<f32> {
    let dims = vec2<i32>(textureDimensions(source));
    let center = min(vec2<i32>(floor(uv * vec2<f32>(dims))), dims - vec2<i32>(1));
    var glow = vec3<f32>(0.0);
    var air = 0.0;
    for (var v = 0u; v < TAP_VEC4S; v += 1u) {
        let gw = blur_params.glow_weights[v];
        let aw = blur_params.air_weights[v];
        let base = i32(v * 4u) - TAP_RADIUS;
        let s0 = blur_tap(source, center, dir, base);
        let s1 = blur_tap(source, center, dir, base + 1);
        let s2 = blur_tap(source, center, dir, base + 2);
        let s3 = blur_tap(source, center, dir, base + 3);
        glow += s0.rgb * gw.x + s1.rgb * gw.y + s2.rgb * gw.z + s3.rgb * gw.w;
        air += dot(vec4<f32>(s0.a, s1.a, s2.a, s3.a), aw);
    }
    return vec4<f32>(glow, air);
}

@fragment
fn blur_horizontal_fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return blur_value(light_source_tex, in.uv, vec2<i32>(1, 0));
}

@fragment
fn blur_vertical_fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return blur_value(light_temp_tex, in.uv, vec2<i32>(0, 1));
}

fn layer_uv(pixel: vec2<f32>, offset: vec2<f32>) -> vec2<f32> {
    let physical = frame.native_size * frame.scale;
    let screen_offset = vec2<f32>(-offset.x, offset.y);
    return (pixel + screen_offset + (physical - frame.window_size) * 0.5) / physical;
}

fn layer_texel(uv: vec2<f32>) -> vec2<f32> {
    return min(floor(uv * frame.native_size), frame.native_size - vec2<f32>(1.0));
}

fn layer_cell(texel: vec2<f32>, snapped: vec2<f32>) -> vec2<f32> {
    return snapped + vec2<f32>(
        texel.x + 0.5 - frame.native_size.x * 0.5,
        frame.native_size.y * 0.5 - texel.y - 0.5
    );
}

fn over(dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {
    let a = clamp(src.a, 0.0, 1.0);
    return vec4<f32>(src.rgb * a + dst.rgb * (1.0 - a), a + dst.a * (1.0 - a));
}

fn over_premultiplied(dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {
    let a = clamp(src.a, 0.0, 1.0);
    return vec4<f32>(src.rgb + dst.rgb * (1.0 - a), a + dst.a * (1.0 - a));
}

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn star_layer(uv: vec2<f32>) -> vec4<f32> {
    let t = layer_texel(uv);
    let cell = layer_cell(t, vec2<f32>(0.0));
    let star_uv = (cell - frame.stars.center + frame.stars.scroll) / frame.stars.world_scale;
    let sample = textureSample(star_tex, star_sampler, star_uv);
    let gcell = floor(fract(star_uv) * frame.stars.world_scale);
    let phase = hash2(gcell) * TAU;
    let cycles = round(48.0 + 119.0 * hash2(gcell + vec2<f32>(19.3, 7.1)));
    let flicker = 1.0 - 0.55 * pow(0.5 + 0.5 * sin(frame.stars.sidereal * cycles * TAU + phase), 2.0);
    let magnitude = hash2(gcell + vec2<f32>(41.7, 289.3));
    let visibility = smoothstep(0.0, 0.12, frame.stars.star_visibility - magnitude * 0.85);
    let above = 1.0 - smoothstep(frame.stars.horizon - 0.04, frame.stars.horizon, uv.y);
    return vec4<f32>(sample.rgb, clamp(sample.a * visibility * above * flicker, 0.0, 1.0));
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
    let screen = vec2<f32>(pixel.x - frame.window_size.x * 0.5, frame.window_size.y * 0.5 - pixel.y);
    return (screen - center) / max(size, vec2<f32>(1.0)) + vec2<f32>(0.5);
}

fn sun_layer(pixel: vec2<f32>) -> vec4<f32> {
    let uv = celestial_uv(pixel, frame.sun_center, frame.sun_size);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) { return vec4<f32>(0.0); }
    let position = snapped_disc_position(uv, max(frame.sun.quad_size, 1.0));
    let pixel_size = 2.0 / max(frame.sun.quad_size, 1.0);
    let radius = length(position);
    let coverage = disc_coverage(radius, frame.sun.disc_radius, pixel_size);
    let normalized_radius = clamp(radius / frame.sun.disc_radius, 0.0, 1.0);
    var color = mix(vec3<f32>(1.0, 0.88, 0.59), vec3<f32>(1.0, 0.64, 0.17), smoothstep(0.0, 0.5, normalized_radius));
    color = mix(color, vec3<f32>(1.0, 0.22, 0.03), smoothstep(0.5, 1.0, normalized_radius));
    color *= 1.0 + (position_hash(floor(uv * frame.sun.quad_size)) - 0.5) * 0.05 * (1.0 - normalized_radius);
    let disc = mix(color, color * vec3<f32>(1.0, 0.74, 0.46), frame.sun.redness) * 14.0;
    let aura = aura_falloff(radius, frame.sun.disc_radius) * (1.0 - coverage) * 0.5;
    let corona = pow(clamp(1.0 - radius, 0.0, 1.0), 2.2) * 2.4 * frame.sun.occlusion * frame.sun.occlusion;
    let alpha = coverage + aura + corona;
    let premultiplied = disc * coverage
        + mix(vec3<f32>(1.0, 0.62, 0.30), vec3<f32>(1.0, 0.40, 0.17), frame.sun.redness) * 1.6 * aura
        + vec3<f32>(1.0, 0.82, 0.52) * 3.0 * corona;
    return vec4<f32>(premultiplied / max(alpha, 1e-4), clamp(alpha, 0.0, 1.0));
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

fn moon_layer(pixel: vec2<f32>) -> vec4<f32> {
    let uv = celestial_uv(pixel, frame.moon_center, frame.moon_size);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) { return vec4<f32>(0.0); }
    let quad_size = max(frame.moon.quad_size, 1.0);
    let position = snapped_disc_position(uv, quad_size);
    let pixel_size = 2.0 / quad_size;
    let radius = length(position);
    let coverage = disc_coverage(radius, frame.moon.disc_radius, pixel_size);
    let mare = quantize(mare_coverage(position), 5.0);
    let albedo = mix(vec3<f32>(0.66, 0.71, 0.84), vec3<f32>(0.30, 0.35, 0.55), mare);
    let albedo_luminance = dot(albedo, vec3<f32>(0.3333));
    let sun_direction = normalize(frame.moon.sun_direction + vec2<f32>(1e-5, 0.0));
    let tangent = vec2<f32>(-sun_direction.y, sun_direction.x);
    let tangent_distance = dot(position, tangent);
    let half_width = sqrt(max(frame.moon.disc_radius * frame.moon.disc_radius - tangent_distance * tangent_distance, 0.0));
    let terminator = (1.0 - 2.0 * frame.moon.illumination) * half_width;
    let sunlight = smoothstep(-pixel_size, pixel_size, dot(position, sun_direction) - terminator);
    let distance_to_umbra = length(position - frame.moon.umbra);
    let umbra_shadow = 1.0 - smoothstep(-0.5 * pixel_size, 0.5 * pixel_size, distance_to_umbra - frame.moon.umbra_radius);
    let penumbra = 1.0 - smoothstep(frame.moon.umbra_radius, frame.moon.umbra_radius + 1.1, distance_to_umbra);
    let umbra_core = smoothstep(0.1, 1.2, frame.moon.umbra_radius - distance_to_umbra);
    let blood_moon_color = mix(vec3<f32>(1.05, 0.16, 0.06), vec3<f32>(0.32, 0.045, 0.03), umbra_core)
        * (0.5 + 0.75 * albedo_luminance);
    let reflected = mix(albedo * 0.04, albedo * 4.2, sunlight);
    let day_reflected = mix(albedo * 0.02, albedo * 0.12, sunlight);
    var light = mix(reflected, day_reflected, frame.moon.sky_color.a);
    light *= 1.0 - 0.4 * penumbra;
    light = mix(light, blood_moon_color, umbra_shadow);
    let disc_color = frame.moon.sky_color.rgb + light;
    let halo = aura_falloff(radius, frame.moon.disc_radius)
        * (1.0 - coverage)
        * mix(0.4, 0.8, frame.moon.lunar_shadow)
        * (1.0 - frame.moon.sky_color.a);
    let halo_color = mix(vec3<f32>(0.55, 0.60, 0.78) * 0.7, vec3<f32>(1.0, 0.14, 0.05) * 1.4, frame.moon.lunar_shadow);
    let alpha = coverage + halo;
    return vec4<f32>((disc_color * coverage + halo_color * halo) / max(alpha, 1e-4), clamp(alpha, 0.0, 1.0));
}

fn atmosphere_layer(uv: vec2<f32>) -> vec4<f32> {
    let distance_below_horizon = uv.y - frame.atmosphere.horizon;
    let ground_alpha = smoothstep(-0.02, 0.02, distance_below_horizon);
    let haze = exp(-max(-distance_below_horizon, 0.0) * 6.0) * frame.atmosphere.intensity;
    let atmosphere_alpha = clamp(ground_alpha + haze * (1.0 - ground_alpha), 0.0, 1.0);
    let aspect = vec2<f32>(frame.atmosphere.aspect, 1.0);
    let sun_alpha = frame.atmosphere.sun_glow.w * exp(-length((uv - frame.atmosphere.sun_pos) * aspect) * 3.0);
    let moon_alpha = frame.atmosphere.moon_glow.w * exp(-length((uv - frame.atmosphere.moon_pos) * aspect) * 4.5);
    let glow_color = frame.atmosphere.sun_glow.rgb * sun_alpha + frame.atmosphere.moon_glow.rgb * moon_alpha;
    let glow_alpha = clamp(sun_alpha + moon_alpha, 0.0, 1.0);
    let alpha = glow_alpha + atmosphere_alpha * (1.0 - glow_alpha);
    let premultiplied = glow_color + frame.atmosphere.color.rgb * atmosphere_alpha * (1.0 - glow_alpha);
    return vec4<f32>(premultiplied / max(alpha, 1e-4), alpha);
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

fn silhouette_height(cell_x: f32, params: SilhouetteParams) -> f32 {
    let noise = silhouette_fbm(cell_x * params.inv_wavelength, params.seed);
    return params.base + params.amp * (noise * 2.0 - 1.0);
}

fn silhouette_layer(uv: vec2<f32>, params: SilhouetteParams) -> vec4<f32> {
    let cell = layer_cell(layer_texel(uv), params.snapped_cam);
    return vec4<f32>(params.color.rgb, select(0.0, params.color.a, cell.y < silhouette_height(cell.x, params)));
}

fn point_glow(world: vec2<f32>) -> f32 {
    var result = 0.0;
    for (var i = 0u; i < frame.lighting.light_count; i += 1u) {
        let light = frame.lighting.lights[i];
        let falloff = clamp(1.0 - distance(world, floor(light.xy) + vec2<f32>(0.5)) / max(light.z, 1.0), 0.0, 1.0);
        result = max(result, falloff * falloff * light.w);
    }
    return result;
}

fn wall_layer(uv: vec2<f32>) -> vec4<f32> {
    let cell = layer_cell(layer_texel(uv), frame.wall_snapped);
    let world = cell + frame.wall.world_offset;
    let n = vnoise(cell * 0.11) * 0.55 + vnoise(cell * 0.2233 + vec2<f32>(13.7, 41.3)) * 0.3 + vnoise(cell * 0.4411 + vec2<f32>(71.9, 7.5)) * 0.15;
    let step_value = min(u32(n * 4.0), 3u);
    var rgb = frame.wall.base_color.rgb * (0.7 + 0.15 * f32(step_value));
    rgb = mix(rgb, CAVE_DARK, clamp(frame.lighting.darkness * (1.0 - point_glow(world)), 0.0, 1.0));
    return vec4<f32>(rgb, (1.0 - smoothstep(-24.0, 8.0, world.y)) * frame.wall.base_color.a);
}

fn lit_world(uv: vec2<f32>) -> vec4<f32> {
    let t = layer_texel(uv);
    let p = vec2<u32>(t);
    let world = textureLoad(world_tex, p, 0);
    let cell = layer_cell(t, frame.lighting.snapped_cam);
    let te = t + frame.lighting.margin;
    let core = textureLoad(emission_tex, vec2<u32>(te), 0).rgb;
    let field_size = vec2<f32>(textureDimensions(light_tex)) * LIGHT_FIELD_DOWNSCALE;
    let field = textureSample(light_tex, linear_sampler, (te + vec2<f32>(0.5)) / field_size);
    let halo = field.rgb;
    let lit = max(point_glow(cell), max(halo.r, max(halo.g, halo.b)) * 0.1);
    let ambient = clamp(field.a * 10.0, 0.0, 1.0) * (1.0 - frame.lighting.darkness);
    let incident = clamp(ambient + lit, 0.0, 1.0);
    var rgb = mix(CAVE_DARK * world.a, world.rgb, incident);
    rgb += halo * (world.a * 0.03) + core * (world.a * 3.0);
    return vec4<f32>(rgb, world.a);
}

@fragment
fn scene_fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let pixel = in.uv * frame.window_size;
    var color = frame.clear_color;
    if frame.sky_synced != 0u {
        color = over(color, star_layer(layer_uv(pixel, frame.star_offset)));
        color = over(color, sun_layer(pixel));
        color = over(color, moon_layer(pixel));
        color = over(color, atmosphere_layer(layer_uv(pixel, vec2<f32>(0.0))));
        color = over(color, silhouette_layer(layer_uv(pixel, frame.far_offset), frame.far));
        color = over(color, silhouette_layer(layer_uv(pixel, frame.near_offset), frame.near));
        color = over(color, wall_layer(layer_uv(pixel, frame.wall_offset)));
    }
    color = over_premultiplied(color, lit_world(layer_uv(pixel, frame.world_offset)));
    return color;
}

@vertex
fn line_vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> ColorOutput {
    let item = lines[instance];
    let direction = item.b - item.a;
    let normal = normalize(vec2<f32>(-direction.y, direction.x) + vec2<f32>(1e-5, 0.0)) * 0.5;
    let corners = array<vec2<f32>, 6>(
        item.a - normal, item.b - normal, item.b + normal,
        item.a - normal, item.b + normal, item.a + normal
    );
    let p = corners[vertex];
    var out: ColorOutput;
    out.clip_position = vec4<f32>(p.x * 2.0 / frame.window_size.x, p.y * 2.0 / frame.window_size.y, 0.0, 1.0);
    out.color = item.color;
    return out;
}

@fragment
fn line_fragment(in: ColorOutput) -> @location(0) vec4<f32> {
    return in.color;
}
