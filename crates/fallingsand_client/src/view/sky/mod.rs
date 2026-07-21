mod lighting;
pub mod materials;

pub use lighting::{ActiveLights, apply_lighting};
pub use materials::{AtmosphereParams, LightingParams, MoonParams, StarfieldParams, SunParams};

use super::Game;
use super::camera::{CameraState, STAR_WORLD_TILE};
use crate::game::RenderMode;
use bevy::prelude::*;
use fallingsand_core::celestial::{MOON_DISC_RADIUS, SHADE_DISC_RADIUS, SUN_DISC_RADIUS};
use fallingsand_core::{CelestialState, smoothstep};

const MAX_DARKNESS: f32 = 0.82;
const HORIZON_FRAC: f32 = 0.22;
const ORBIT_RADIUS: f32 = 133.0;
const SUN_SIZE: f32 = 48.0;
const MOON_SIZE: f32 = 28.0;
const DEFAULT_CLEAR: Color = Color::srgb(0.08, 0.09, 0.13);

#[derive(Resource, Default, Clone, Copy)]
pub struct Sky {
    pub state: CelestialState,
    pub synced: bool,
    pub color_linear: Vec3,
}

impl Sky {
    pub fn darkness(&self) -> f32 {
        (1.0 - self.state.light) * MAX_DARKNESS
    }
}

#[derive(Clone, Copy, Default)]
pub struct CelestialQuad {
    pub center_px: Vec2,
    pub size_px: Vec2,
}

#[derive(Resource, Clone, Default)]
pub struct SkyRenderState {
    pub sun: SunParams,
    pub moon: MoonParams,
    pub stars: StarfieldParams,
    pub atmosphere: AtmosphereParams,
    pub sun_quad: CelestialQuad,
    pub moon_quad: CelestialQuad,
}

pub fn sky_color(light: f32, sun_alt: f32, solar_occ: f32) -> Vec3 {
    let night = Vec3::new(0.015, 0.025, 0.055);
    let day = Vec3::new(0.40, 0.60, 0.86);
    let horizon = Vec3::new(0.85, 0.45, 0.28);
    let base = night.lerp(day, light);
    let band = (1.0 - sun_alt.abs()).powi(3);
    let warm = band * (1.0 - solar_occ) * 0.6;
    let mut rgb = base.lerp(horizon, warm);
    if solar_occ > 0.0 {
        let grey = Vec3::splat((rgb.x + rgb.y + rgb.z) / 3.0);
        rgb = rgb.lerp(grey, solar_occ * 0.4);
    }
    rgb
}

pub fn sync_sky(
    game: Res<Game>,
    state: Res<CameraState>,
    mut sky: ResMut<Sky>,
    mut render: ResMut<SkyRenderState>,
    mut clear: ResMut<ClearColor>,
) {
    let clock = game.0.ingame().map(|ingame| ingame.clock);
    let synced = clock.is_some_and(|clock| clock.synced);
    let calendar = clock.map(|clock| clock.calendar).unwrap_or_default();
    sky.synced = synced;
    if !synced {
        *sky = Sky::default();
        *render = SkyRenderState::default();
        clear.0 = DEFAULT_CLEAR;
        return;
    }

    let celestial = calendar.celestial();
    let center = Vec2::new(0.0, -HORIZON_FRAC * ORBIT_RADIUS);
    let native = state.native.as_vec2();
    let horizon_uv = 0.5 + HORIZON_FRAC * ORBIT_RADIUS / native.y;
    let sun_position = Vec2::from(celestial.sun_position) * ORBIT_RADIUS;
    let moon_position = Vec2::from(celestial.moon_position) * ORBIT_RADIUS;
    let shade_position = Vec2::from(celestial.shade_position) * ORBIT_RADIUS;
    let sun_altitude = celestial.sun_altitude;
    let solar_occlusion = celestial.solar_occlusion;
    let moon_size = (MOON_SIZE * celestial.moon_radius_scale).max(1.0);
    let world_to_moon_uv = 2.0 / moon_size;
    let umbra = (shade_position - moon_position) * world_to_moon_uv;
    let umbra_radius = SHADE_DISC_RADIUS * ORBIT_RADIUS * world_to_moon_uv;
    let color = sky_color(celestial.light, sun_altitude, solar_occlusion);
    let linear = Color::srgb(color.x, color.y, color.z).to_linear();
    sky.state = celestial;
    sky.color_linear = Vec3::new(linear.red, linear.green, linear.blue);
    clear.0 = Color::srgb(color.x, color.y, color.z);

    let k = state.k as f32;
    let place = |p: Vec2| match state.render_mode {
        RenderMode::PixelPerfect => (p * k).round(),
        RenderMode::Smooth => p * k,
        RenderMode::Retro => p.round() * k,
    };
    render.sun_quad = CelestialQuad {
        center_px: place(sun_position + center),
        size_px: Vec2::splat(SUN_SIZE * k),
    };
    render.moon_quad = CelestialQuad {
        center_px: place(moon_position + center),
        size_px: Vec2::splat(moon_size * k),
    };

    let redness = 1.0 - smoothstep(0.0, 0.35, sun_altitude);
    render.sun = SunParams {
        redness,
        occlusion: solar_occlusion,
        quad_size: SUN_SIZE,
        disc_radius: SUN_DISC_RADIUS * ORBIT_RADIUS / (SUN_SIZE * 0.5),
    };
    render.moon = MoonParams {
        sun_direction: Vec2::from(celestial.sun_direction),
        illumination: celestial.illumination,
        umbra,
        umbra_radius,
        sky_color: sky.color_linear.extend(celestial.daylight),
        quad_size: moon_size,
        disc_radius: MOON_DISC_RADIUS * celestial.moon_radius_scale * ORBIT_RADIUS
            / (moon_size * 0.5),
        lunar_shadow: celestial.lunar_shadow,
    };
    render.stars = StarfieldParams {
        center,
        scroll: state.star_scroll.floor(),
        world_scale: STAR_WORLD_TILE,
        star_visibility: celestial.star_visibility,
        horizon: horizon_uv,
        sidereal: calendar.sidereal(),
    };

    let day_haze = Vec3::new(0.72, 0.82, 0.96);
    let night_haze = Vec3::new(0.08, 0.11, 0.20);
    let warm = Vec3::new(0.98, 0.6, 0.38);
    let base = night_haze.lerp(day_haze, celestial.light);
    let horizon_band = (1.0 - sun_altitude.abs()).powi(2);
    let atmosphere_color = base.lerp(warm, horizon_band * (1.0 - solar_occlusion) * 0.7);
    let to_uv = |p: Vec2| Vec2::new(0.5 + p.x / native.x, 0.5 - p.y / native.y);
    let sun_glow_col = Vec3::new(1.0, 0.6, 0.3).lerp(Vec3::new(1.0, 0.38, 0.16), redness);
    let sun_glow_i = celestial.daylight * (0.12 + 0.7 * horizon_band) * (1.0 - solar_occlusion);
    let moon_up = smoothstep(-0.10, 0.10, celestial.moon_position[1]);
    let moon_glow_i = celestial.illumination * moon_up * (1.0 - celestial.daylight) * 0.22;
    render.atmosphere = AtmosphereParams {
        color: atmosphere_color.extend(1.0),
        sun_pos: to_uv(sun_position + center),
        moon_pos: to_uv(moon_position + center),
        sun_glow: sun_glow_col.extend(sun_glow_i),
        moon_glow: Vec3::new(0.5, 0.6, 0.85).extend(moon_glow_i),
        horizon: horizon_uv,
        intensity: 0.25 + 0.6 * celestial.light,
        aspect: native.x / native.y,
        _pad: 0.0,
    };
}
