use super::Sky;
use super::materials::{LightingMaterial, LightingParams};
use crate::view::Game;
use crate::view::camera::{CameraState, LayerAssets};
use crate::view::chunks::EmissiveUploadQueue;
use bevy::asset::{AssetId, RenderAssetUsages};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use fallingsand_core::content;
use fallingsand_core::{CellPos, Tag};

pub const MAX_PLAYER_LIGHTS: usize = 32;
const PLAYER_LIGHT_RADIUS: f32 = 70.0;
const BURNING_LIGHT_RADIUS: f32 = 40.0;

const SPREAD_RADIUS: i32 = 10;
const GLOW_MARGIN: u32 = SPREAD_RADIUS as u32 + 2;
const EMISSIVE_GAIN: f32 = 1.15;

#[derive(Resource, Default)]
pub struct ActiveLights {
    pub lights: Vec<Vec4>,
    pub darkness: f32,
}

impl ActiveLights {
    pub fn write(&self, params: &mut LightingParams) {
        params.darkness = self.darkness;
        params.light_count = self.lights.len().min(MAX_PLAYER_LIGHTS) as u32;
        let mut array = [Vec4::ZERO; MAX_PLAYER_LIGHTS];
        for (slot, light) in array.iter_mut().zip(self.lights.iter()) {
            *slot = *light;
        }
        params.lights = array;
    }
}

#[derive(Resource)]
pub struct EmissiveField {
    pub image: Handle<Image>,
    pub origin: IVec2,
    pub size: UVec2,
    scratch: Vec<[f32; 3]>,
    light: Vec<[f32; 3]>,
    tmp: Vec<[f32; 3]>,
    built: bool,
}

impl EmissiveField {
    pub fn new(images: &mut Assets<Image>) -> Self {
        let size = UVec2::splat(1);
        Self {
            image: alloc_image(images, size),
            origin: IVec2::ZERO,
            size,
            scratch: Vec::new(),
            light: Vec::new(),
            tmp: Vec::new(),
            built: false,
        }
    }
}

fn alloc_image(images: &mut Assets<Image>, size: UVec2) -> Handle<Image> {
    let mut image = Image::new_fill(
        Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
    images.add(image)
}

pub fn setup_emissive(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let field = EmissiveField::new(&mut images);
    commands.insert_resource(field);
}

#[allow(clippy::too_many_arguments)]
pub fn sync_emissive(
    mut game: ResMut<Game>,
    sky: Res<Sky>,
    state: Res<CameraState>,
    assets: Res<LayerAssets>,
    mut field: ResMut<EmissiveField>,
    mut images: ResMut<Assets<Image>>,
    mut lighting_mats: ResMut<Assets<LightingMaterial>>,
    mut queue: ResMut<EmissiveUploadQueue>,
) {
    let size = state.native + UVec2::splat(2 * GLOW_MARGIN);
    if field.size != size {
        field.image = alloc_image(&mut images, size);
        field.size = size;
        field.scratch = vec![[0.0; 3]; (size.x * size.y) as usize];
        field.light = vec![[0.0; 3]; (size.x * size.y) as usize];
        field.tmp = vec![[0.0; 3]; (size.x * size.y) as usize];
        field.built = false;
        if let Some(mut material) = lighting_mats.get_mut(&assets.lighting) {
            material.emissive = field.image.clone();
        }
    }

    let (snapped, _) = state.layer(Vec2::ZERO);
    let origin = snapped - (state.native / 2).as_ivec2() - IVec2::splat(GLOW_MARGIN as i32);
    let lit = sky.synced && sky.darkness() > 0.001;

    let moved = origin != field.origin;
    let dirty = game
        .0
        .ingame_mut()
        .is_some_and(|ingame| ingame.world.emissive_dirty());

    if field.built && !moved && !dirty && lit {
        return;
    }
    if !lit {
        if field.built {
            field.light.iter_mut().for_each(|texel| *texel = [0.0; 3]);
            upload(&mut queue, field.image.id(), size, &field.light);
            field.built = false;
        }
        field.origin = origin;
        return;
    }

    field.origin = origin;
    field.built = true;
    let width = size.x as i32;
    let height = size.y as i32;

    let mut scratch = std::mem::take(&mut field.scratch);
    let mut light = std::mem::take(&mut field.light);
    let mut tmp = std::mem::take(&mut field.tmp);
    scratch.iter_mut().for_each(|texel| *texel = [0.0; 3]);
    if let Some(ingame) = game.0.ingame() {
        let view = &ingame.world;
        for ty in 0..height {
            for tx in 0..width {
                let cell = CellPos::new(origin.x + tx, origin.y + height - 1 - ty);
                let Some(content_cell) = view.get_cell(cell) else {
                    continue;
                };
                if !content::tags(content_cell.material).contains(Tag::Emissive) {
                    continue;
                }
                let info = content::material(content_cell.material);
                let color = info.colors[content_cell.shade() as usize % info.colors.len()];
                scratch[(ty * width + tx) as usize] = [
                    color[0] as f32 / 255.0,
                    color[1] as f32 / 255.0,
                    color[2] as f32 / 255.0,
                ];
            }
        }
    }

    spread(&scratch, &mut light, &mut tmp, width, height);
    upload(&mut queue, field.image.id(), size, &light);
    field.scratch = scratch;
    field.light = light;
    field.tmp = tmp;
}

fn spread(src: &[[f32; 3]], dst: &mut [[f32; 3]], tmp: &mut [[f32; 3]], width: i32, height: i32) {
    let falloff: [f32; (2 * SPREAD_RADIUS + 1) as usize] = std::array::from_fn(|i| {
        let d = i as i32 - SPREAD_RADIUS;
        1.0 - d.abs() as f32 / (SPREAD_RADIUS as f32 + 1.0)
    });
    for y in 0..height {
        for x in 0..width {
            let mut acc = [0.0f32; 3];
            for (i, f) in falloff.iter().enumerate() {
                let sx = (x + i as i32 - SPREAD_RADIUS).clamp(0, width - 1);
                let s = src[(y * width + sx) as usize];
                acc[0] = acc[0].max(s[0] * f);
                acc[1] = acc[1].max(s[1] * f);
                acc[2] = acc[2].max(s[2] * f);
            }
            tmp[(y * width + x) as usize] = acc;
        }
    }
    for y in 0..height {
        for x in 0..width {
            let mut acc = [0.0f32; 3];
            for (i, f) in falloff.iter().enumerate() {
                let sy = (y + i as i32 - SPREAD_RADIUS).clamp(0, height - 1);
                let s = tmp[(sy * width + x) as usize];
                acc[0] = acc[0].max(s[0] * f);
                acc[1] = acc[1].max(s[1] * f);
                acc[2] = acc[2].max(s[2] * f);
            }
            let out = &mut dst[(y * width + x) as usize];
            out[0] = (acc[0] * EMISSIVE_GAIN).min(1.0);
            out[1] = (acc[1] * EMISSIVE_GAIN).min(1.0);
            out[2] = (acc[2] * EMISSIVE_GAIN).min(1.0);
        }
    }
}

fn upload(
    queue: &mut EmissiveUploadQueue,
    image: AssetId<Image>,
    size: UVec2,
    texels: &[[f32; 3]],
) {
    let mut data = Vec::with_capacity(texels.len() * 4);
    for texel in texels {
        for channel in texel {
            data.push((channel.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
        data.push(255);
    }
    queue.push(image, size, data);
}

pub fn apply_lighting(
    game: Res<Game>,
    sky: Res<Sky>,
    field: Res<EmissiveField>,
    state: Res<CameraState>,
    assets: Res<LayerAssets>,
    mut active: ResMut<ActiveLights>,
    mut materials: ResMut<Assets<LightingMaterial>>,
) {
    active.darkness = if sky.synced { sky.darkness() } else { 0.0 };
    active.lights.clear();
    if active.darkness > 0.001
        && let Some(ingame) = game.0.ingame()
    {
        if let Some(local) = ingame.local_avatar() {
            active.lights.push(Vec4::new(
                local.pos.x,
                local.pos.y,
                PLAYER_LIGHT_RADIUS,
                1.0,
            ));
            if local.burning {
                active.lights.push(Vec4::new(
                    local.pos.x,
                    local.pos.y,
                    BURNING_LIGHT_RADIUS,
                    1.0,
                ));
            }
        }
        let local = ingame
            .net
            .session
            .as_ref()
            .and_then(|session| session.player());
        for (&player, remote) in &ingame.players.avatars {
            if Some(player) == local {
                continue;
            }
            if remote.burning {
                active.lights.push(Vec4::new(
                    remote.pos.x,
                    remote.pos.y,
                    BURNING_LIGHT_RADIUS,
                    1.0,
                ));
            }
        }
    }

    let Some(mut material) = materials.get_mut(&assets.lighting) else {
        return;
    };
    active.write(&mut material.params);
    material.params.emissive_origin = field.origin.as_vec2();
    material.params.emissive_size = field.size.as_vec2();
    let (snapped, _) = state.layer(Vec2::ZERO);
    material.params.snapped_cam = snapped.as_vec2();
    material.params.native_size = state.native.as_vec2();
}
