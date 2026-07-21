mod composite;
mod extract;
mod light_field;
pub(crate) mod primitives;
pub(crate) mod raster;
pub(crate) mod sky;
mod targets;

use bevy::core_pipeline::schedule::Core2d;
use bevy::core_pipeline::{Core2dSystems, FullscreenShader};
use bevy::ecs::system::SystemParam;
use bevy::image::{
    ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::texture::{FallbackImageZero, GpuImage};
use bevy::render::view::ViewTarget;
use bevy::render::{
    ExtractSchedule, Render, RenderApp, RenderStartup, RenderSystems, init_gpu_resource,
};
use bevy::shader::Shader;
use composite::CompositePass;
use extract::ExtractedRenderFrame;
use light_field::LightFieldPass;
use raster::{ChunkAtlasState, RasterPass};
use targets::GameplayTargets;

const HDR_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

#[derive(Resource, Clone)]
struct RenderSourceAssets {
    stars: Handle<Image>,
    _common_shader: Handle<Shader>,
}

impl FromWorld for RenderSourceAssets {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.resource::<AssetServer>();
        Self {
            stars: asset_server
                .load_builder()
                .with_settings(|settings: &mut ImageLoaderSettings| {
                    settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                        address_mode_u: ImageAddressMode::Repeat,
                        address_mode_v: ImageAddressMode::Repeat,
                        mag_filter: ImageFilterMode::Nearest,
                        min_filter: ImageFilterMode::Nearest,
                        ..default()
                    });
                })
                .load("sky/stars.png"),
            _common_shader: asset_server.load("shaders/common.wgsl"),
        }
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum GameplayRenderSet {
    Camera,
    Prepared,
}

pub struct GameplayRendererPlugin;

impl Plugin for GameplayRendererPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderSourceAssets>()
            .init_resource::<ChunkAtlasState>()
            .init_resource::<primitives::ParticleVisuals>()
            .init_resource::<primitives::DebugPrimitives>()
            .init_resource::<sky::Sky>()
            .configure_sets(
                Update,
                (GameplayRenderSet::Camera, GameplayRenderSet::Prepared)
                    .chain()
                    .after(super::io::drive_game),
            )
            .add_systems(Startup, super::camera::setup_camera)
            .add_systems(
                Update,
                super::camera::sync_camera.in_set(GameplayRenderSet::Camera),
            )
            .add_systems(
                Update,
                (
                    sky::sync_sky,
                    raster::sync_chunk_atlas,
                    primitives::update_particles,
                    primitives::update_debug_primitives,
                )
                    .chain()
                    .in_set(GameplayRenderSet::Prepared),
            );

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<ExtractedRenderFrame>()
            .init_resource::<GameplayTargets>()
            .add_systems(
                RenderStartup,
                init_renderer.after(init_gpu_resource::<FallbackImageZero>),
            )
            .add_systems(ExtractSchedule, extract::extract_render_frame)
            .add_systems(
                Render,
                prepare_renderer.in_set(RenderSystems::PrepareResources),
            )
            .add_systems(
                Core2d,
                render_game
                    .after(Core2dSystems::MainPass)
                    .before(Core2dSystems::PostProcess),
            );
    }
}

fn init_renderer(
    mut commands: Commands,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    asset_server: Res<AssetServer>,
    fullscreen: Res<FullscreenShader>,
    fallback: Res<FallbackImageZero>,
    cache: Res<PipelineCache>,
) {
    commands.insert_resource(RasterPass::new(&device, &queue, &asset_server, &cache));
    commands.insert_resource(LightFieldPass::new(
        &device,
        &queue,
        &asset_server,
        &fullscreen,
        &cache,
    ));
    commands.insert_resource(CompositePass::new(
        &device,
        &asset_server,
        &fullscreen,
        &fallback,
        &cache,
    ));
}

#[derive(SystemParam)]
struct PreparePasses<'w> {
    targets: ResMut<'w, GameplayTargets>,
    raster: ResMut<'w, RasterPass>,
    light_field: ResMut<'w, LightFieldPass>,
    composite: ResMut<'w, CompositePass>,
}

fn prepare_renderer(
    frame: Res<ExtractedRenderFrame>,
    mut passes: PreparePasses,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    images: Res<RenderAssets<GpuImage>>,
    cache: Res<PipelineCache>,
) {
    if !frame.active {
        passes
            .raster
            .deactivate(frame.atlas_side, frame.atlas_generation, &device);
        return;
    }
    let targets = passes.targets.ensure(&device, frame.native);
    passes.raster.prepare(
        &frame.raster,
        &frame.chunks,
        &frame.quads,
        &frame.uploads,
        frame.atlas_side,
        frame.atlas_generation,
        frame.instance_generation,
        &device,
        &queue,
        &cache,
    );
    passes.light_field.prepare(targets, &device, &cache);
    passes.composite.prepare(
        &frame.scene,
        &frame.lights,
        &frame.lines,
        &frame.stars,
        targets,
        &device,
        &queue,
        &images,
        &cache,
    );
}

#[derive(SystemParam)]
struct DrawPasses<'w> {
    targets: Res<'w, GameplayTargets>,
    raster: Res<'w, RasterPass>,
    light_field: Res<'w, LightFieldPass>,
    composite: Res<'w, CompositePass>,
    cache: Res<'w, PipelineCache>,
}

fn render_game(
    view: ViewQuery<&ViewTarget>,
    frame: Res<ExtractedRenderFrame>,
    passes: DrawPasses,
    mut context: RenderContext,
) {
    if !frame.active {
        return;
    }
    let Some(targets) = passes.targets.get() else {
        return;
    };
    passes.raster.draw(
        &mut context,
        targets,
        frame.chunks.len() as u32,
        frame.quads.len() as u32,
        &passes.cache,
    );
    passes
        .light_field
        .draw(&mut context, targets, &passes.cache);
    passes.composite.draw(
        &mut context,
        view.into_inner(),
        frame.lines.len() as u32,
        &passes.cache,
    );
}

pub(super) fn queue_pipeline(
    cache: &PipelineCache,
    label: &'static str,
    layout: Vec<BindGroupLayoutDescriptor>,
    vertex: VertexState,
    shader: Handle<Shader>,
    entry: &'static str,
    blend: Option<BlendState>,
) -> CachedRenderPipelineId {
    cache.queue_render_pipeline(RenderPipelineDescriptor {
        label: Some(label.into()),
        layout,
        vertex,
        fragment: Some(FragmentState {
            shader,
            entry_point: Some(entry.into()),
            targets: vec![Some(ColorTargetState {
                format: HDR_FORMAT,
                blend,
                write_mask: ColorWrites::ALL,
            })],
            ..default()
        }),
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            ..default()
        },
        ..default()
    })
}

pub(super) fn populated<T: Clone>(values: &[T], sentinel: T) -> Vec<T> {
    if values.is_empty() {
        vec![sentinel]
    } else {
        values.to_vec()
    }
}

pub(super) fn color_attachment(
    view: &TextureView,
    clear: Option<Color>,
) -> RenderPassColorAttachment<'_> {
    RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: Operations {
            load: clear.map_or(LoadOp::Load, |color| {
                LoadOp::Clear(color.to_linear().into())
            }),
            store: StoreOp::Store,
        },
    }
}

pub(super) fn pipeline(
    cache: &PipelineCache,
    id: CachedRenderPipelineId,
) -> Option<&RenderPipeline> {
    cache.get_render_pipeline(id)
}
