use super::lighting::MAX_PLAYER_LIGHTS;
use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;

#[derive(ShaderType, Debug, Clone, PartialEq)]
pub struct LightingParams {
    pub lights: [Vec4; MAX_PLAYER_LIGHTS],
    pub darkness: f32,
    pub light_count: u32,
    pub snapped_cam: Vec2,
    pub margin: Vec2,
}

impl Default for LightingParams {
    fn default() -> Self {
        Self {
            lights: [Vec4::ZERO; MAX_PLAYER_LIGHTS],
            darkness: 0.0,
            light_count: 0,
            snapped_cam: Vec2::ZERO,
            margin: Vec2::ZERO,
        }
    }
}

#[derive(ShaderType, Debug, Clone, Default, PartialEq)]
pub struct SunParams {
    pub redness: f32,
    pub occlusion: f32,
    pub quad_size: f32,
    pub disc_radius: f32,
}

#[derive(ShaderType, Debug, Clone, Default, PartialEq)]
pub struct MoonParams {
    pub sun_direction: Vec2,
    pub illumination: f32,
    pub umbra: Vec2,
    pub umbra_radius: f32,
    pub sky_color: Vec4,
    pub quad_size: f32,
    pub disc_radius: f32,
    pub lunar_shadow: f32,
}

#[derive(ShaderType, Debug, Clone, Default, PartialEq)]
pub struct StarfieldParams {
    pub center: Vec2,
    pub scroll: Vec2,
    pub world_scale: f32,
    pub star_visibility: f32,
    pub horizon: f32,
    pub sidereal: f32,
}

#[derive(ShaderType, Debug, Clone, Default, PartialEq)]
pub struct AtmosphereParams {
    pub color: Vec4,
    pub sun_pos: Vec2,
    pub moon_pos: Vec2,
    pub sun_glow: Vec4,
    pub moon_glow: Vec4,
    pub horizon: f32,
    pub intensity: f32,
    pub aspect: f32,
    pub _pad: f32,
}
