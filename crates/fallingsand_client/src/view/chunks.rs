use super::Game;
use super::camera::WORLD_LAYER;
use crate::game::world::ChunkChange;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, Origin3d, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
    TextureDimension, TextureFormat,
};
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;
use bevy::render::{ExtractSchedule, MainWorld, Render, RenderApp, RenderSystems};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d};
use fallingsand_core::{CHUNK_AREA, CHUNK_SIZE, Cell, CellOffset, ChunkPos, DirtyRect};

const SHADES: u32 = 16;
const UPLOAD_RETRY_FRAMES: u8 = 3;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ChunkMaterial {
    #[texture(0, sample_type = "u_int")]
    pub cells: Handle<Image>,
    #[texture(1, filterable = false)]
    pub palette: Handle<Image>,
}

impl Material2d for ChunkMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/chunk.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Resource)]
pub struct RenderShared {
    pub palette: Handle<Image>,
    pub quad: Handle<Mesh>,
}

#[derive(Resource, Default)]
pub struct ChunkVisuals {
    pub chunk_entities: HashMap<ChunkPos, (Entity, Handle<Image>)>,
    pub uploads: usize,
    pub upload_bytes: usize,
}

pub struct ChunkUpload {
    image: AssetId<Image>,
    rect: DirtyRect,
    data: Vec<u8>,
    retries: u8,
}

#[derive(Resource, Default)]
pub struct ChunkUploadQueue(Vec<ChunkUpload>);

#[derive(Resource, Default)]
struct RenderChunkUploads(Vec<ChunkUpload>);

#[derive(Component)]
pub struct ChunkQuad;

pub fn setup_render_app(app: &mut App) {
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app
            .init_resource::<RenderChunkUploads>()
            .add_systems(ExtractSchedule, extract_chunk_uploads)
            .add_systems(
                Render,
                upload_chunk_rects.in_set(RenderSystems::PrepareResources),
            );
    }
}

fn extract_chunk_uploads(
    mut main_world: ResMut<MainWorld>,
    mut uploads: ResMut<RenderChunkUploads>,
) {
    let mut queue = main_world.resource_mut::<ChunkUploadQueue>();
    uploads.0.append(&mut queue.0);
}

fn upload_chunk_rects(
    mut uploads: ResMut<RenderChunkUploads>,
    images: Res<RenderAssets<GpuImage>>,
    queue: Res<RenderQueue>,
) {
    uploads.0.retain_mut(|upload| {
        let Some(gpu) = images.get(upload.image) else {
            upload.retries += 1;
            return upload.retries < UPLOAD_RETRY_FRAMES;
        };
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &gpu.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: upload.rect.min_x as u32,
                    y: upload.rect.min_y as u32,
                    z: 0,
                },
                aspect: TextureAspect::All,
            },
            &upload.data,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(upload.rect.width() * 4),
                rows_per_image: Some(upload.rect.height()),
            },
            Extent3d {
                width: upload.rect.width(),
                height: upload.rect.height(),
                depth_or_array_layers: 1,
            },
        );
        false
    });
}

pub fn setup_shared(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    game: Res<Game>,
) {
    let materials = &game.0.registries.materials;
    let width = materials.len().max(1) as u32;
    let mut data = vec![0u8; (width * SHADES * 4) as usize];
    for (id, material) in materials.iter() {
        for shade in 0..SHADES {
            let color = material.colors[shade as usize % material.colors.len()];
            let index = ((shade * width + id.0 as u32) * 4) as usize;
            data[index..index + 4].copy_from_slice(&color);
        }
    }
    let palette = images.add(Image::new(
        Extent3d {
            width,
            height: SHADES,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ));
    let quad = meshes.add(Rectangle::default());
    commands.insert_resource(RenderShared { palette, quad });
}

fn chunk_texture_data(cells: &[Cell; CHUNK_AREA]) -> Vec<u8> {
    let mut data = Vec::with_capacity(CHUNK_AREA * 4);
    for cell in cells {
        data.extend_from_slice(&cell.material.0.to_le_bytes());
        data.push(cell.shade_flags);
        data.push(0);
    }
    data
}

fn pack_rect(cells: &[Cell; CHUNK_AREA], rect: DirtyRect) -> Vec<u8> {
    let mut data = Vec::with_capacity((rect.width() * rect.height() * 4) as usize);
    for y in rect.min_y..=rect.max_y {
        for x in rect.min_x..=rect.max_x {
            let cell = cells[CellOffset::new(x, y).index()];
            data.extend_from_slice(&cell.material.0.to_le_bytes());
            data.push(cell.shade_flags);
            data.push(0);
        }
    }
    data
}

enum Plan {
    Full,
    Rects(Vec<DirtyRect>),
}

#[allow(clippy::too_many_arguments)]
pub fn sync_chunks(
    mut commands: Commands,
    mut game: ResMut<Game>,
    mut visuals: ResMut<ChunkVisuals>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<ChunkMaterial>>,
    mut queue: ResMut<ChunkUploadQueue>,
    shared: Res<RenderShared>,
) {
    visuals.uploads = 0;
    visuals.upload_bytes = 0;

    let Some(ingame) = game.0.ingame_mut() else {
        for (_, (entity, image)) in visuals.chunk_entities.drain() {
            commands.entity(entity).despawn();
            images.remove(&image);
        }
        return;
    };
    let changes = ingame.world.take_changes();
    if changes.is_empty() {
        return;
    }

    let mut plans: HashMap<ChunkPos, Plan> = HashMap::default();
    for change in changes {
        match change {
            ChunkChange::Cleared => {
                for (_, (entity, image)) in visuals.chunk_entities.drain() {
                    commands.entity(entity).despawn();
                    images.remove(&image);
                }
                plans.clear();
            }
            ChunkChange::Loaded(pos) => {
                plans.insert(pos, Plan::Full);
            }
            ChunkChange::Unloaded(pos) => {
                plans.remove(&pos);
                if let Some((entity, image)) = visuals.chunk_entities.remove(&pos) {
                    commands.entity(entity).despawn();
                    images.remove(&image);
                }
            }
            ChunkChange::Delta(pos, rect) => match plans.get_mut(&pos) {
                Some(Plan::Full) => {}
                Some(Plan::Rects(rects)) => rects.push(rect),
                None => {
                    plans.insert(pos, Plan::Rects(vec![rect]));
                }
            },
        }
    }

    for (pos, plan) in plans {
        let Some(chunk) = ingame.world.chunks.get(&pos) else {
            continue;
        };
        let rects = match plan {
            Plan::Rects(rects) if visuals.chunk_entities.contains_key(&pos) => rects,
            _ => {
                full_upload(
                    &mut commands,
                    &mut visuals,
                    &mut images,
                    &mut materials,
                    &mut queue,
                    &shared,
                    pos,
                    &chunk.cells,
                );
                continue;
            }
        };
        let image = visuals.chunk_entities[&pos].1.id();
        for rect in rects {
            let data = pack_rect(&chunk.cells, rect);
            visuals.uploads += 1;
            visuals.upload_bytes += data.len();
            queue.0.push(ChunkUpload {
                image,
                rect,
                data,
                retries: 0,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn full_upload(
    commands: &mut Commands,
    visuals: &mut ChunkVisuals,
    images: &mut Assets<Image>,
    materials: &mut Assets<ChunkMaterial>,
    queue: &mut ChunkUploadQueue,
    shared: &RenderShared,
    pos: ChunkPos,
    cells: &[Cell; CHUNK_AREA],
) {
    visuals.uploads += 1;
    visuals.upload_bytes += CHUNK_AREA * 4;

    if let Some((_, image)) = visuals.chunk_entities.get(&pos) {
        queue.0.push(ChunkUpload {
            image: image.id(),
            rect: DirtyRect::FULL,
            data: chunk_texture_data(cells),
            retries: 0,
        });
        return;
    }

    let size = CHUNK_SIZE as f32;
    let image = images.add(Image::new(
        Extent3d {
            width: CHUNK_SIZE as u32,
            height: CHUNK_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        chunk_texture_data(cells),
        TextureFormat::Rgba8Uint,
        RenderAssetUsages::RENDER_WORLD,
    ));
    let material = materials.add(ChunkMaterial {
        cells: image.clone(),
        palette: shared.palette.clone(),
    });
    let entity = commands
        .spawn((
            ChunkQuad,
            Mesh2d(shared.quad.clone()),
            MeshMaterial2d(material),
            Transform::from_xyz(
                pos.x as f32 * size + size / 2.0,
                pos.y as f32 * size + size / 2.0,
                0.0,
            )
            .with_scale(Vec3::new(size, size, 1.0)),
            RenderLayers::layer(WORLD_LAYER),
        ))
        .id();
    visuals.chunk_entities.insert(pos, (entity, image));
}
