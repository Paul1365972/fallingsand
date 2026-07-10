use super::lighting::MAX_LIGHTS;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d};

#[derive(ShaderType, Debug, Clone)]
pub struct LightingParams {
    pub lights: [Vec4; MAX_LIGHTS],
    pub darkness: f32,
    pub light_count: u32,
    pub snapped_cam: Vec2,
    pub native_size: Vec2,
}

impl Default for LightingParams {
    fn default() -> Self {
        Self {
            lights: [Vec4::ZERO; MAX_LIGHTS],
            darkness: 0.0,
            light_count: 0,
            snapped_cam: Vec2::ZERO,
            native_size: Vec2::ONE,
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct LightingMaterial {
    #[uniform(0)]
    pub params: LightingParams,
    #[texture(1)]
    #[sampler(2)]
    pub world: Handle<Image>,
}

impl Material2d for LightingMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/lighting.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub(super) struct SkyCompositeMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub(super) texture: Handle<Image>,
}

impl Material2d for SkyCompositeMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/sky_composite.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(ShaderType, Debug, Clone, Default)]
pub(super) struct SunParams {
    pub(super) redness: f32,
    pub(super) occlusion: f32,
    pub(super) _pad: Vec2,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub(super) struct SunMaterial {
    #[uniform(0)]
    pub(super) params: SunParams,
    #[texture(1)]
    #[sampler(2)]
    pub(super) texture: Handle<Image>,
}

impl Material2d for SunMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/sun.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(ShaderType, Debug, Clone, Default)]
pub(super) struct MoonParams {
    pub(super) sun_direction: Vec2,
    pub(super) illumination: f32,
    pub(super) umbra: Vec2,
    pub(super) umbra_radius: f32,
    pub(super) sky_color: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub(super) struct MoonMaterial {
    #[uniform(0)]
    pub(super) params: MoonParams,
    #[texture(1)]
    #[sampler(2)]
    pub(super) texture: Handle<Image>,
}

impl Material2d for MoonMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/moon.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(ShaderType, Debug, Clone, Default)]
pub(super) struct StarfieldParams {
    pub(super) tiling: f32,
    pub(super) aspect: f32,
    pub(super) star_visibility: f32,
    pub(super) horizon: f32,
    pub(super) time: f32,
    pub(super) scroll: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub(super) struct StarfieldMaterial {
    #[uniform(0)]
    pub(super) params: StarfieldParams,
    #[texture(1)]
    #[sampler(2)]
    pub(super) texture: Handle<Image>,
}

impl Material2d for StarfieldMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/starfield.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(ShaderType, Debug, Clone, Default)]
pub(super) struct HorizonParams {
    pub(super) color: Vec4,
    pub(super) horizon: f32,
    pub(super) intensity: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub(super) struct HorizonMaterial {
    #[uniform(0)]
    pub(super) params: HorizonParams,
}

impl Material2d for HorizonMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/horizon.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}
