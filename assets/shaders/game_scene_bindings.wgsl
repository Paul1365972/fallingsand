#define_import_path fallingsand::game_scene_bindings
#import fallingsand::game_common::PixelViewport

const MAX_LIGHTS: u32 = 256u;

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

struct CelestialFrame {
    sun: SunParams,
    moon: MoonParams,
    stars: StarfieldParams,
    atmosphere: AtmosphereParams,
    sun_center: vec2<f32>,
    sun_size: vec2<f32>,
    moon_center: vec2<f32>,
    moon_size: vec2<f32>,
}

struct LightingParams {
    lights: array<vec4<f32>, MAX_LIGHTS>,
    darkness: f32,
    light_count: u32,
    snapped_cam: vec2<f32>,
    margin: vec2<f32>,
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

struct WorldFrame {
    lighting: LightingParams,
    wall: WallParams,
    far: SilhouetteParams,
    near: SilhouetteParams,
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

@group(0) @binding(0) var<uniform> frame: SceneFrame;
@group(0) @binding(1) var world_tex: texture_2d<f32>;
@group(0) @binding(2) var emission_tex: texture_2d<f32>;
@group(0) @binding(3) var light_tex: texture_2d<f32>;
@group(0) @binding(4) var linear_sampler: sampler;
@group(0) @binding(5) var star_tex: texture_2d<f32>;
@group(0) @binding(6) var star_sampler: sampler;
