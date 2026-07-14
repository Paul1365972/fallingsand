use super::lighting::MAX_PLAYER_LIGHTS;
use crate::view::camera::premultiplied_composite;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey};

#[derive(ShaderType, Debug, Clone)]
pub struct LightingParams {
    pub lights: [Vec4; MAX_PLAYER_LIGHTS],
    pub darkness: f32,
    pub light_count: u32,
    pub snapped_cam: Vec2,
    pub native_size: Vec2,
}

impl Default for LightingParams {
    fn default() -> Self {
        Self {
            lights: [Vec4::ZERO; MAX_PLAYER_LIGHTS],
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
    #[texture(3)]
    pub glow: Handle<Image>,
}

impl Material2d for LightingMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/lighting.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        premultiplied_composite(descriptor);
        Ok(())
    }
}

#[derive(ShaderType, Debug, Clone, Default)]
pub struct SunParams {
    pub redness: f32,
    pub occlusion: f32,
    pub quad_size: f32,
    pub disc_radius: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct SunMaterial {
    #[uniform(0)]
    pub params: SunParams,
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

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct MoonMaterial {
    #[uniform(0)]
    pub params: MoonParams,
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
pub struct StarfieldParams {
    pub center: Vec2,
    pub native_size: Vec2,
    pub scroll: Vec2,
    pub world_scale: f32,
    pub star_visibility: f32,
    pub horizon: f32,
    pub sidereal: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct StarfieldMaterial {
    #[uniform(0)]
    pub params: StarfieldParams,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Handle<Image>,
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

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct AtmosphereMaterial {
    #[uniform(0)]
    pub params: AtmosphereParams,
}

impl Material2d for AtmosphereMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/atmosphere.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}
