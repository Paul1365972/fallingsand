use super::RenderSourceAssets;
use super::atlas::{ChunkAtlasState, ChunkInstance, ChunkUpload};
use super::light_field::extended_size;
use super::primitives::{DebugPrimitives, ParticleVisuals};
use super::raster::{QuadInstance, RasterFrame};
use super::scene::{LineInstance, PointLight, SceneFrame, point_lights, scene_frame};
use super::sky::Sky;
use crate::view::Game;
use crate::view::camera::CameraState;
use bevy::prelude::*;
use bevy::render::MainWorld;
use bevy::render::render_resource::ShaderType;

#[derive(Clone, ShaderType)]
pub(super) struct PixelViewport {
    pub native_size: Vec2,
    pub window_size: Vec2,
    pub physical_size: Vec2,
    pub window_center: Vec2,
}

impl Default for PixelViewport {
    fn default() -> Self {
        Self {
            native_size: Vec2::ONE,
            window_size: Vec2::ONE,
            physical_size: Vec2::ONE,
            window_center: Vec2::splat(0.5),
        }
    }
}

#[derive(Resource)]
pub struct ExtractedRenderFrame {
    pub active: bool,
    pub raster: RasterExtract,
    pub composite: CompositeExtract,
    pub native: UVec2,
}

pub(super) struct RasterExtract {
    pub frame: RasterFrame,
    pub chunks: Vec<ChunkInstance>,
    pub quads: Vec<QuadInstance>,
    pub uploads: Vec<ChunkUpload>,
    pub atlas_side: u32,
    pub atlas_generation: u64,
    pub instance_generation: u64,
}

pub(super) struct CompositeExtract {
    pub frame: SceneFrame,
    pub lines: Vec<LineInstance>,
    pub lights: Vec<PointLight>,
    pub stars: Handle<Image>,
}

impl Default for ExtractedRenderFrame {
    fn default() -> Self {
        Self {
            active: false,
            raster: RasterExtract {
                frame: default(),
                chunks: Vec::new(),
                quads: Vec::new(),
                uploads: Vec::new(),
                atlas_side: 16,
                atlas_generation: 0,
                instance_generation: u64::MAX,
            },
            composite: CompositeExtract {
                frame: default(),
                lines: Vec::new(),
                lights: Vec::new(),
                stars: default(),
            },
            native: UVec2::ONE,
        }
    }
}

pub fn extract_render_frame(
    mut main_world: ResMut<MainWorld>,
    mut out: ResMut<ExtractedRenderFrame>,
) {
    let state = main_world.resource::<CameraState>();
    let viewport = PixelViewport {
        native_size: state.native.as_vec2(),
        window_size: state.window_px.as_vec2(),
        physical_size: state.native.as_vec2() * state.k as f32,
        window_center: state.window_px.as_vec2() * 0.5,
    };
    let world_snapped = state.layer(Vec2::ZERO).0.as_vec2();
    let native = state.native;
    let elapsed = main_world.resource::<Time>().elapsed_secs();
    let sky = main_world.resource::<Sky>().clone();
    let clear_color: Vec4 = main_world
        .resource::<ClearColor>()
        .0
        .to_linear()
        .to_f32_array()
        .into();
    let game = main_world.resource::<Game>();
    let active = game.0.ingame().is_some();
    let lights = point_lights(game, &sky);
    let scene = scene_frame(viewport.clone(), state, &sky, clear_color, lights.len());
    let raster = RasterFrame {
        viewport,
        world_snapped,
        emission_size: extended_size(native).as_vec2(),
        time: elapsed,
    };
    let quads = main_world
        .resource::<ParticleVisuals>()
        .quads
        .iter()
        .copied()
        .map(QuadInstance::from)
        .collect();
    let lines = main_world
        .resource::<DebugPrimitives>()
        .lines
        .iter()
        .copied()
        .map(LineInstance::from)
        .collect();
    let stars = main_world.resource::<RenderSourceAssets>().stars.clone();
    let previous_generation = out.raster.instance_generation;
    let atlas = main_world
        .resource_mut::<ChunkAtlasState>()
        .extract(previous_generation);
    if previous_generation != atlas.instance_generation {
        out.raster.chunks = atlas.chunks;
    }
    out.active = active;
    out.raster.frame = raster;
    out.raster.quads = quads;
    out.raster.uploads = atlas.uploads;
    out.raster.atlas_side = atlas.side;
    out.raster.atlas_generation = atlas.atlas_generation;
    out.raster.instance_generation = atlas.instance_generation;
    out.composite.frame = scene;
    out.composite.lines = lines;
    out.composite.lights = lights;
    out.composite.stars = stars;
    out.native = native;
}
