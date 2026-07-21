use super::extract::PixelViewport;
use super::light_field;
use super::primitives::DebugLine;
use super::sky::Sky;
use crate::game::RenderMode;
use crate::view::Game;
use crate::view::camera::CameraState;
use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;

const FAR_RATIO: Vec2 = Vec2::new(0.88, 0.92);
const NEAR_RATIO: Vec2 = Vec2::new(0.72, 0.80);
const WALL_RATIO: Vec2 = Vec2::splat(0.15);
const WALL_COLOR: Vec3 = Vec3::new(0.060, 0.052, 0.048);
const FAR_HAZE: f32 = 0.6;
const NEAR_HAZE: f32 = 0.35;
const FAR_BASE: f32 = 14.0;
const FAR_AMP: f32 = 90.0;
const FAR_WAVELENGTH: f32 = 220.0;
const NEAR_BASE: f32 = 4.0;
const NEAR_AMP: f32 = 45.0;
const NEAR_WAVELENGTH: f32 = 90.0;
const PLAYER_LIGHT_RADIUS: f32 = 40.0;
const BURNING_LIGHT_RADIUS: f32 = 64.0;

#[derive(Clone, ShaderType, Default)]
pub(super) struct SunFrame {
    pub(super) redness: f32,
    pub(super) occlusion: f32,
    pub(super) quad_size: f32,
    pub(super) disc_radius: f32,
}

#[derive(Clone, ShaderType, Default)]
pub(super) struct MoonFrame {
    pub(super) sun_direction: Vec2,
    pub(super) illumination: f32,
    pub(super) umbra: Vec2,
    pub(super) umbra_radius: f32,
    pub(super) sky_color: Vec4,
    pub(super) quad_size: f32,
    pub(super) disc_radius: f32,
    pub(super) lunar_shadow: f32,
}

#[derive(Clone, ShaderType, Default)]
pub(super) struct StarfieldFrame {
    pub(super) center: Vec2,
    pub(super) scroll: Vec2,
    pub(super) world_scale: f32,
    pub(super) star_visibility: f32,
    pub(super) horizon: f32,
    pub(super) sidereal: f32,
}

#[derive(Clone, ShaderType, Default)]
pub(super) struct AtmosphereFrame {
    pub(super) color: Vec4,
    pub(super) sun_pos: Vec2,
    pub(super) moon_pos: Vec2,
    pub(super) sun_glow: Vec4,
    pub(super) moon_glow: Vec4,
    pub(super) horizon: f32,
    pub(super) intensity: f32,
    pub(super) aspect: f32,
    pub(super) _pad: f32,
}

#[derive(Clone, ShaderType, Default)]
pub(super) struct CelestialFrame {
    pub(super) sun: SunFrame,
    pub(super) moon: MoonFrame,
    pub(super) stars: StarfieldFrame,
    pub(super) atmosphere: AtmosphereFrame,
    pub(super) sun_center: Vec2,
    pub(super) sun_size: Vec2,
    pub(super) moon_center: Vec2,
    pub(super) moon_size: Vec2,
}

#[derive(Clone, ShaderType, Default)]
struct LightingFrame {
    darkness: f32,
    light_count: u32,
    snapped_cam: Vec2,
    margin: Vec2,
}

#[derive(Clone, ShaderType, Default)]
struct WallFrame {
    base_color: Vec4,
    world_offset: Vec2,
}

#[derive(Clone, ShaderType, Default)]
struct SilhouetteFrame {
    color: Vec4,
    snapped_cam: Vec2,
    base: f32,
    amplitude: f32,
    inv_wavelength: f32,
    seed: f32,
}

#[derive(Clone, ShaderType, Default)]
struct WorldFrame {
    lighting: LightingFrame,
    wall: WallFrame,
    far: SilhouetteFrame,
    near: SilhouetteFrame,
    wall_snapped: Vec2,
}

#[derive(Clone, ShaderType, Default)]
struct BackdropFrame {
    celestial: CelestialFrame,
    star_offset: Vec2,
    far_offset: Vec2,
    near_offset: Vec2,
    wall_offset: Vec2,
}

#[derive(Clone, ShaderType)]
pub(super) struct SceneFrame {
    pub(super) viewport: PixelViewport,
    world: WorldFrame,
    backdrop: BackdropFrame,
    world_offset: Vec2,
    clear_color: Vec4,
    backdrop_ready: u32,
}

impl Default for SceneFrame {
    fn default() -> Self {
        Self {
            viewport: default(),
            world: default(),
            backdrop: default(),
            world_offset: Vec2::ZERO,
            clear_color: Vec4::ZERO,
            backdrop_ready: 0,
        }
    }
}

#[derive(Clone, ShaderType)]
pub(super) struct PointLight {
    pub(super) center: Vec2,
    pub(super) radius: f32,
    pub(super) intensity: f32,
}

#[derive(Clone, ShaderType)]
pub(super) struct LineInstance {
    pub(super) a: Vec2,
    pub(super) b: Vec2,
    pub(super) color: Vec4,
}

impl From<DebugLine> for LineInstance {
    fn from(line: DebugLine) -> Self {
        Self {
            a: line.a,
            b: line.b,
            color: line.color,
        }
    }
}

fn layer_offset(state: &CameraState, ratio: Vec2, drift: Vec2) -> Vec2 {
    let (_, remainder) = state.layer(ratio);
    let raw = remainder + drift;
    match state.render_mode {
        RenderMode::PixelPerfect => -raw.round(),
        RenderMode::Smooth => -raw,
        RenderMode::Retro => Vec2::ZERO,
    }
}

pub(super) fn point_lights(game: &Game, sky: &Sky) -> Vec<PointLight> {
    let mut lights = Vec::new();
    if sky.darkness() <= 0.001 {
        return lights;
    }
    let Some(ingame) = game.0.ingame() else {
        return lights;
    };
    if let Some(local) = ingame.local_avatar() {
        lights.push(PointLight {
            center: local.pos,
            radius: if local.burning {
                BURNING_LIGHT_RADIUS
            } else {
                PLAYER_LIGHT_RADIUS
            },
            intensity: 1.0,
        });
    }
    let local = ingame
        .net
        .session
        .as_ref()
        .and_then(|session| session.player());
    for (&player, remote) in &ingame.players.avatars {
        if Some(player) != local && remote.burning {
            lights.push(PointLight {
                center: remote.pos,
                radius: BURNING_LIGHT_RADIUS,
                intensity: 1.0,
            });
        }
    }
    lights
}

pub(super) fn scene_frame(
    viewport: PixelViewport,
    state: &CameraState,
    sky: &Sky,
    clear_color: Vec4,
    light_count: usize,
) -> SceneFrame {
    let sky_linear = if sky.synced {
        sky.color_linear
    } else {
        Vec3::ZERO
    };
    let (world_snapped, _) = state.layer(Vec2::ZERO);
    let (wall_snapped, _) = state.layer(WALL_RATIO);
    let (far_snapped, _) = state.layer(FAR_RATIO);
    let (near_snapped, _) = state.layer(NEAR_RATIO);
    let star_drift = (sky.star_scroll - sky.star_scroll.floor()) * state.k as f32;
    SceneFrame {
        viewport,
        world: WorldFrame {
            lighting: LightingFrame {
                darkness: if sky.synced { sky.darkness() } else { 0.0 },
                light_count: light_count as u32,
                snapped_cam: world_snapped.as_vec2(),
                margin: light_field::margin(state.native),
            },
            wall: WallFrame {
                base_color: WALL_COLOR.extend(1.0),
                world_offset: WALL_RATIO * state.pos,
            },
            far: SilhouetteFrame {
                color: (sky_linear * FAR_HAZE).extend(1.0),
                snapped_cam: far_snapped.as_vec2(),
                base: FAR_BASE,
                amplitude: FAR_AMP,
                inv_wavelength: 1.0 / FAR_WAVELENGTH,
                seed: 17.0,
            },
            near: SilhouetteFrame {
                color: (sky_linear * NEAR_HAZE).extend(1.0),
                snapped_cam: near_snapped.as_vec2(),
                base: NEAR_BASE,
                amplitude: NEAR_AMP,
                inv_wavelength: 1.0 / NEAR_WAVELENGTH,
                seed: 53.0,
            },
            wall_snapped: wall_snapped.as_vec2(),
        },
        backdrop: BackdropFrame {
            celestial: sky.celestial.clone(),
            star_offset: layer_offset(state, Vec2::ONE, star_drift),
            far_offset: layer_offset(state, FAR_RATIO, Vec2::ZERO),
            near_offset: layer_offset(state, NEAR_RATIO, Vec2::ZERO),
            wall_offset: layer_offset(state, WALL_RATIO, Vec2::ZERO),
        },
        world_offset: layer_offset(state, Vec2::ZERO, Vec2::ZERO),
        clear_color,
        backdrop_ready: u32::from(sky.synced),
    }
}
