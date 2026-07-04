use crate::ClientRegistry;
use crate::worldview::WorldView;
use bevy::asset::RenderAssetUsages;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};
use fallingsand_core::{CHUNK_AREA, CHUNK_SIZE, Cell, ChunkPos};

pub struct ChunkRenderPlugin;

const SHADES: u32 = 16;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ChunkMaterial {
    #[texture(0, sample_type = "u_int")]
    pub cells: Handle<Image>,
    #[texture(1, filterable = false)]
    pub palette: Handle<Image>,
}

impl Material2d for ChunkMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://fallingsand/shaders/chunk.wgsl".into()
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
    pub entities: HashMap<ChunkPos, (Entity, Handle<Image>)>,
    pub uploads: usize,
}

#[derive(Component)]
pub struct ChunkQuad;

impl Plugin for ChunkRenderPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "shaders/chunk.wgsl");
        app.add_plugins(Material2dPlugin::<ChunkMaterial>::default())
            .init_resource::<ChunkVisuals>()
            .add_systems(Startup, setup_shared)
            .add_systems(Update, sync_chunks);
    }
}

fn setup_shared(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    registry: Res<ClientRegistry>,
) {
    let materials = &registry.0;
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

fn sync_chunks(
    mut commands: Commands,
    mut view: ResMut<WorldView>,
    mut visuals: ResMut<ChunkVisuals>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<ChunkMaterial>>,
    shared: Res<RenderShared>,
) {
    let size = CHUNK_SIZE as f32;
    visuals.uploads = 0;

    let mut removed: Vec<ChunkPos> = Vec::new();
    for &pos in visuals.entities.keys() {
        if !view.chunks.contains_key(&pos) {
            removed.push(pos);
        }
    }
    for pos in removed {
        if let Some((entity, image)) = visuals.entities.remove(&pos) {
            commands.entity(entity).despawn();
            images.remove(&image);
        }
    }

    for (&pos, chunk) in view.chunks.iter_mut() {
        if !chunk.dirty {
            continue;
        }
        chunk.dirty = false;
        visuals.uploads += 1;

        if let Some((_, image)) = visuals.entities.get(&pos) {
            if let Some(mut image) = images.get_mut(image) {
                image.data = Some(chunk_texture_data(&chunk.cells));
            }
            continue;
        }

        let image = images.add(Image::new(
            Extent3d {
                width: CHUNK_SIZE as u32,
                height: CHUNK_SIZE as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            chunk_texture_data(&chunk.cells),
            TextureFormat::Rgba8Uint,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
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
            ))
            .id();
        visuals.entities.insert(pos, (entity, image));
    }
}
