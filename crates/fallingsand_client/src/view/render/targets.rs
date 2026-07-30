use super::HDR_FORMAT;
use super::light_field::{LIGHT_FIELD_DOWNSCALE, extended_size};
use bevy::prelude::*;
use bevy::render::render_resource::*;
use bevy::render::renderer::RenderDevice;

pub(super) struct Target {
    _texture: Texture,
    pub view: TextureView,
}

impl Target {
    fn new(device: &RenderDevice, label: &'static str, size: UVec2, format: TextureFormat) -> Self {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some(label),
            size: Extent3d {
                width: size.x.max(1),
                height: size.y.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        Self {
            _texture: texture,
            view,
        }
    }
}

pub(super) const FOG_FORMAT: TextureFormat = TextureFormat::R8Unorm;

pub(super) struct RenderTargets {
    pub native: UVec2,
    pub revision: u64,
    pub world: Target,
    pub emission: Target,
    pub quarter: Target,
    pub blur_temp: Target,
    pub light: Target,
    pub fog_source: Target,
    pub fog_temp: Target,
    pub fog: Target,
}

impl RenderTargets {
    fn new(device: &RenderDevice, native: UVec2, revision: u64) -> Self {
        let extended = extended_size(native);
        let quarter = extended / LIGHT_FIELD_DOWNSCALE;
        Self {
            native,
            revision,
            world: Target::new(device, "game_world", native, HDR_FORMAT),
            emission: Target::new(device, "game_emission", extended, HDR_FORMAT),
            quarter: Target::new(device, "game_light_source", quarter, HDR_FORMAT),
            blur_temp: Target::new(device, "game_light_horizontal", quarter, HDR_FORMAT),
            light: Target::new(device, "game_light", quarter, HDR_FORMAT),
            fog_source: Target::new(device, "game_fog_source", extended, FOG_FORMAT),
            fog_temp: Target::new(device, "game_fog_horizontal", extended, FOG_FORMAT),
            fog: Target::new(device, "game_fog", extended, FOG_FORMAT),
        }
    }
}

#[derive(Resource, Default)]
pub(super) struct GameplayTargets {
    current: Option<RenderTargets>,
    revision: u64,
}

impl GameplayTargets {
    pub(super) fn ensure(&mut self, device: &RenderDevice, native: UVec2) -> &RenderTargets {
        if self
            .current
            .as_ref()
            .is_none_or(|targets| targets.native != native)
        {
            self.revision = self.revision.wrapping_add(1);
            self.current = Some(RenderTargets::new(device, native, self.revision));
        }
        self.current.as_ref().expect("render targets initialized")
    }

    pub(super) fn get(&self) -> Option<&RenderTargets> {
        self.current.as_ref()
    }
}
