use super::camera::{CameraState, FAR_RATIO, NEAR_RATIO, WALL_RATIO};
use super::sky::Sky;
use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;

const WALL_COLOR: Vec3 = Vec3::new(0.060, 0.052, 0.048);
const FAR_HAZE: f32 = 0.6;
const NEAR_HAZE: f32 = 0.35;
const FAR_BASE: f32 = 14.0;
const FAR_AMP: f32 = 90.0;
const FAR_WAVELENGTH: f32 = 220.0;
const NEAR_BASE: f32 = 4.0;
const NEAR_AMP: f32 = 45.0;
const NEAR_WAVELENGTH: f32 = 90.0;

#[derive(ShaderType, Debug, Clone, Default)]
pub struct WallParams {
    pub base_color: Vec4,
    pub world_offset: Vec2,
}

#[derive(ShaderType, Debug, Clone, Default)]
pub struct SilhouetteParams {
    pub color: Vec4,
    pub snapped_cam: Vec2,
    pub base: f32,
    pub amp: f32,
    pub inv_wavelength: f32,
    pub seed: f32,
}

#[derive(Resource, Clone, Default)]
pub struct ParallaxState {
    pub wall: WallParams,
    pub far: SilhouetteParams,
    pub near: SilhouetteParams,
}

pub fn sync_parallax(sky: Res<Sky>, state: Res<CameraState>, mut parallax: ResMut<ParallaxState>) {
    parallax.wall = WallParams {
        base_color: WALL_COLOR.extend(1.0),
        world_offset: WALL_RATIO * state.pos,
    };
    let sky_linear = if sky.synced {
        sky.color_linear
    } else {
        Vec3::ZERO
    };
    let (far_snapped, _) = state.layer(FAR_RATIO);
    parallax.far = SilhouetteParams {
        color: (sky_linear * FAR_HAZE).extend(1.0),
        snapped_cam: far_snapped.as_vec2(),
        base: FAR_BASE,
        amp: FAR_AMP,
        inv_wavelength: 1.0 / FAR_WAVELENGTH,
        seed: 17.0,
    };
    let (near_snapped, _) = state.layer(NEAR_RATIO);
    parallax.near = SilhouetteParams {
        color: (sky_linear * NEAR_HAZE).extend(1.0),
        snapped_cam: near_snapped.as_vec2(),
        base: NEAR_BASE,
        amp: NEAR_AMP,
        inv_wavelength: 1.0 / NEAR_WAVELENGTH,
        seed: 53.0,
    };
}
