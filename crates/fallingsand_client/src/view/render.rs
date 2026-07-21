use super::Game;
use super::camera::{
    CameraState, FAR_RATIO, LIGHT_FIELD_DOWNSCALE, NEAR_RATIO, WALL_RATIO, extended_size,
    light_blur_params,
};
use super::chunks::{ChunkRenderState, ChunkUpload};
use super::parallax::{ParallaxState, SilhouetteParams, WallParams};
use super::particles::{ParticleVisuals, WorldQuad};
use super::sky::{
    ActiveLights, AtmosphereParams, LightingParams, MoonParams, Sky, SkyRenderState,
    StarfieldParams, SunParams,
};
use super::ui::debug::{DebugLine, DebugPrimitives};
use crate::game::RenderMode;
use bevy::core_pipeline::schedule::Core2d;
use bevy::core_pipeline::{Core2dSystems, FullscreenShader};
use bevy::image::{
    ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::{
    sampler, storage_buffer_read_only, texture_2d, uniform_buffer,
};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::texture::{FallbackImageZero, GpuImage};
use bevy::render::view::ViewTarget;
use bevy::render::{ExtractSchedule, MainWorld, Render, RenderApp, RenderStartup, RenderSystems};
use fallingsand_core::content;
use fallingsand_core::{CHUNK_SIZE, ChunkPos};

const SHADES: u32 = 16;
const HDR_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

#[derive(Resource, Clone)]
struct RenderSourceAssets {
    stars: Handle<Image>,
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
        }
    }
}

#[derive(Clone, ShaderType)]
struct ChunkInstance {
    world_origin: Vec2,
    atlas_origin: UVec2,
}

#[derive(Clone, ShaderType)]
struct QuadInstance {
    center: Vec2,
    size: Vec2,
    color: Vec4,
}

#[derive(Clone, ShaderType)]
struct LineInstance {
    a: Vec2,
    b: Vec2,
    color: Vec4,
}

#[derive(Clone, ShaderType)]
struct FrameUniform {
    lighting: LightingParams,
    sun: SunParams,
    moon: MoonParams,
    stars: StarfieldParams,
    atmosphere: AtmosphereParams,
    wall: WallParams,
    far: SilhouetteParams,
    near: SilhouetteParams,
    world_snapped: Vec2,
    wall_snapped: Vec2,
    native_size: Vec2,
    window_size: Vec2,
    sun_center: Vec2,
    sun_size: Vec2,
    moon_center: Vec2,
    moon_size: Vec2,
    world_offset: Vec2,
    star_offset: Vec2,
    far_offset: Vec2,
    near_offset: Vec2,
    wall_offset: Vec2,
    clear_color: Vec4,
    scale: f32,
    time: f32,
    sky_synced: u32,
}

impl Default for FrameUniform {
    fn default() -> Self {
        Self {
            lighting: default(),
            sun: default(),
            moon: default(),
            stars: default(),
            atmosphere: default(),
            wall: default(),
            far: default(),
            near: default(),
            world_snapped: Vec2::ZERO,
            wall_snapped: Vec2::ZERO,
            native_size: Vec2::ONE,
            window_size: Vec2::ONE,
            sun_center: Vec2::ZERO,
            sun_size: Vec2::ZERO,
            moon_center: Vec2::ZERO,
            moon_size: Vec2::ZERO,
            world_offset: Vec2::ZERO,
            star_offset: Vec2::ZERO,
            far_offset: Vec2::ZERO,
            near_offset: Vec2::ZERO,
            wall_offset: Vec2::ZERO,
            clear_color: Vec4::ZERO,
            scale: 1.0,
            time: 0.0,
            sky_synced: 0,
        }
    }
}

#[derive(Resource, Default)]
pub struct GameRenderFrame {
    active: bool,
    uniform: FrameUniform,
    chunks: Vec<ChunkInstance>,
    quads: Vec<QuadInstance>,
    lines: Vec<LineInstance>,
    uploads: Vec<ChunkUpload>,
    atlas_side: u32,
    atlas_generation: u64,
    instance_generation: u64,
    native: UVec2,
    stars: Option<Handle<Image>>,
}

struct Target {
    _texture: Texture,
    view: TextureView,
}

impl Target {
    fn new(device: &RenderDevice, label: &'static str, size: UVec2) -> Self {
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
            format: HDR_FORMAT,
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

struct GameTargets {
    native: UVec2,
    world: Target,
    emission: Target,
    quarter: Target,
    blur_temp: Target,
    light: Target,
}

impl GameTargets {
    fn new(device: &RenderDevice, native: UVec2) -> Self {
        let extended = extended_size(native);
        let quarter = extended / LIGHT_FIELD_DOWNSCALE;
        Self {
            native,
            world: Target::new(device, "game_world", native),
            emission: Target::new(device, "game_emission", extended),
            quarter: Target::new(device, "game_light_source", quarter),
            blur_temp: Target::new(device, "game_light_horizontal", quarter),
            light: Target::new(device, "game_light", quarter),
        }
    }
}

struct Atlas {
    generation: u64,
    side: u32,
    texture: Texture,
    view: TextureView,
}

impl Atlas {
    fn new(device: &RenderDevice, side: u32, generation: u64) -> Self {
        let dimension = side * CHUNK_SIZE as u32;
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("game_chunk_atlas"),
            size: Extent3d {
                width: dimension,
                height: dimension,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Uint,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        Self {
            generation,
            side,
            texture,
            view,
        }
    }
}

#[derive(Resource)]
struct GameGpuData {
    frame: UniformBuffer<FrameUniform>,
    blur: UniformBuffer<super::camera::LightBlurParams>,
    chunks: StorageBuffer<Vec<ChunkInstance>>,
    quads: StorageBuffer<Vec<QuadInstance>>,
    lines: StorageBuffer<Vec<LineInstance>>,
    chunk_generation: u64,
    _palette: Texture,
    palette_view: TextureView,
    _emissive_palette: Texture,
    emissive_palette_view: TextureView,
    atlas: Atlas,
    targets: Option<GameTargets>,
    raster_bind_group: Option<BindGroup>,
    downsample_bind_group: Option<BindGroup>,
    blur_h_bind_group: Option<BindGroup>,
    blur_v_bind_group: Option<BindGroup>,
    scene_bind_group: Option<BindGroup>,
    star_view: Option<TextureViewId>,
    fallback_star_view: TextureView,
    fallback_star_sampler: Sampler,
    linear_sampler: Sampler,
}

#[derive(Resource)]
struct GamePipelines {
    layout: BindGroupLayoutDescriptor,
    chunk: CachedRenderPipelineId,
    emissive: CachedRenderPipelineId,
    quad: CachedRenderPipelineId,
    downsample: CachedRenderPipelineId,
    blur_h: CachedRenderPipelineId,
    blur_v: CachedRenderPipelineId,
    scene: CachedRenderPipelineId,
    line: CachedRenderPipelineId,
}

pub struct GameRendererPlugin;

impl Plugin for GameRendererPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderSourceAssets>();
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<GameRenderFrame>()
            .add_systems(RenderStartup, init_renderer)
            .add_systems(ExtractSchedule, extract_game_render_frame)
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

fn layer_offset(state: &CameraState, ratio: Vec2, drift: Vec2) -> Vec2 {
    let (_, remainder) = state.layer(ratio);
    let raw = remainder + drift;
    match state.render_mode {
        RenderMode::PixelPerfect => -raw.round(),
        RenderMode::Smooth => -raw,
        RenderMode::Retro => Vec2::ZERO,
    }
}

fn extract_game_render_frame(mut main_world: ResMut<MainWorld>, mut out: ResMut<GameRenderFrame>) {
    let state = main_world.resource::<CameraState>();
    let active = main_world.resource::<Game>().0.ingame().is_some();
    let sky = *main_world.resource::<Sky>();
    let sky_render = main_world.resource::<SkyRenderState>().clone();
    let lights = main_world.resource::<ActiveLights>().clone();
    let parallax = main_world.resource::<ParallaxState>().clone();
    let quads = main_world.resource::<ParticleVisuals>().quads.clone();
    let lines = main_world.resource::<DebugPrimitives>().lines.clone();
    let clear = main_world
        .resource::<ClearColor>()
        .0
        .to_linear()
        .to_f32_array();
    let elapsed = main_world.resource::<Time>().elapsed_secs();
    let sources = main_world.resource::<RenderSourceAssets>().clone();
    let world_offset = layer_offset(state, Vec2::ZERO, Vec2::ZERO);
    let star_drift = (state.star_scroll - state.star_scroll.floor()) * state.k as f32;
    let uniform = FrameUniform {
        lighting: lights.params,
        sun: sky_render.sun,
        moon: sky_render.moon,
        stars: sky_render.stars,
        atmosphere: sky_render.atmosphere,
        wall: parallax.wall,
        far: parallax.far,
        near: parallax.near,
        world_snapped: state.layer(Vec2::ZERO).0.as_vec2(),
        wall_snapped: state.layer(WALL_RATIO).0.as_vec2(),
        native_size: state.native.as_vec2(),
        window_size: state.window_px.as_vec2(),
        sun_center: sky_render.sun_quad.center_px,
        sun_size: sky_render.sun_quad.size_px,
        moon_center: sky_render.moon_quad.center_px,
        moon_size: sky_render.moon_quad.size_px,
        world_offset,
        star_offset: layer_offset(state, Vec2::ONE, star_drift),
        far_offset: layer_offset(state, FAR_RATIO, Vec2::ZERO),
        near_offset: layer_offset(state, NEAR_RATIO, Vec2::ZERO),
        wall_offset: layer_offset(state, WALL_RATIO, Vec2::ZERO),
        clear_color: clear.into(),
        scale: state.k as f32,
        time: elapsed,
        sky_synced: u32::from(sky.synced),
    };
    let native = state.native;
    let chunk_state = main_world.resource::<ChunkRenderState>();
    let atlas_side = chunk_state.atlas_side;
    let atlas_generation = chunk_state.atlas_generation;
    let instance_generation = chunk_state.instance_generation;
    if out.instance_generation != instance_generation {
        out.chunks = chunk_state
            .chunk_entities
            .iter()
            .map(|(&pos, &slot)| chunk_instance(pos, slot))
            .collect();
        out.instance_generation = instance_generation;
    }
    let uploads = std::mem::take(&mut main_world.resource_mut::<ChunkRenderState>().pending);
    out.active = active;
    out.uniform = uniform;
    out.quads = quads.into_iter().map(quad_instance).collect();
    out.lines = lines.into_iter().map(line_instance).collect();
    out.uploads = uploads;
    out.atlas_side = atlas_side;
    out.atlas_generation = atlas_generation;
    out.native = native;
    out.stars = Some(sources.stars);
}

fn chunk_instance(pos: ChunkPos, slot: super::chunks::AtlasSlot) -> ChunkInstance {
    ChunkInstance {
        world_origin: Vec2::new(
            (pos.x * CHUNK_SIZE as i32) as f32,
            (pos.y * CHUNK_SIZE as i32) as f32,
        ),
        atlas_origin: UVec2::new(slot.x, slot.y) * CHUNK_SIZE as u32,
    }
}

fn quad_instance(quad: WorldQuad) -> QuadInstance {
    QuadInstance {
        center: quad.center,
        size: quad.size,
        color: quad.color,
    }
}

fn line_instance(line: DebugLine) -> LineInstance {
    LineInstance {
        a: line.a,
        b: line.b,
        color: line.color,
    }
}

fn create_palette_textures(device: &RenderDevice, queue: &RenderQueue) -> (Texture, Texture) {
    let width = content::MATERIAL_COUNT as u32;
    let mut colors = vec![0u8; (width * SHADES * 4) as usize];
    let mut emission = vec![0u8; (width * SHADES * 16) as usize];
    for (id, material) in content::materials() {
        let entry = [
            material.emission[0],
            material.emission[1],
            material.emission[2],
            material.flicker,
        ];
        for shade in 0..SHADES {
            let color = material.colors[shade as usize % material.colors.len()];
            let index = ((shade * width + id.0 as u32) * 4) as usize;
            colors[index..index + 4].copy_from_slice(&color);
            let index = ((shade * width + id.0 as u32) * 16) as usize;
            for (channel, value) in entry.iter().enumerate() {
                emission[index + channel * 4..index + channel * 4 + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
    }
    let descriptor = |label, format| TextureDescriptor {
        label: Some(label),
        size: Extent3d {
            width,
            height: SHADES,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    };
    (
        device.create_texture_with_data(
            queue,
            &descriptor("game_palette", TextureFormat::Rgba8UnormSrgb),
            TextureDataOrder::LayerMajor,
            &colors,
        ),
        device.create_texture_with_data(
            queue,
            &descriptor("game_emissive_palette", TextureFormat::Rgba32Float),
            TextureDataOrder::LayerMajor,
            &emission,
        ),
    )
}

fn init_renderer(
    mut commands: Commands,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    asset_server: Res<AssetServer>,
    fullscreen: Res<FullscreenShader>,
    fallback: Res<FallbackImageZero>,
    pipeline_cache: Res<PipelineCache>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "game_renderer_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::VERTEX_FRAGMENT,
            (
                uniform_buffer::<FrameUniform>(false),
                storage_buffer_read_only::<Vec<ChunkInstance>>(false),
                storage_buffer_read_only::<Vec<QuadInstance>>(false),
                storage_buffer_read_only::<Vec<LineInstance>>(false),
                texture_2d(TextureSampleType::Uint),
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<super::camera::LightBlurParams>(false),
            ),
        ),
    );
    let shader = asset_server.load("shaders/game_render.wgsl");
    let pipeline =
        |label: &'static str, vertex: VertexState, entry: &'static str, blend, format| {
            pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
                label: Some(label.into()),
                layout: vec![layout.clone()],
                vertex,
                fragment: Some(FragmentState {
                    shader: shader.clone(),
                    entry_point: Some(entry.into()),
                    targets: vec![Some(ColorTargetState {
                        format,
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
        };
    let custom_vertex = |entry: &'static str| VertexState {
        shader: shader.clone(),
        entry_point: Some(entry.into()),
        ..default()
    };
    let fullscreen_vertex = fullscreen.to_vertex_state();
    let chunk = pipeline(
        "game_chunk_pipeline",
        custom_vertex("chunk_vertex"),
        "chunk_fragment",
        Some(BlendState::ALPHA_BLENDING),
        HDR_FORMAT,
    );
    let emissive = pipeline(
        "game_emissive_pipeline",
        custom_vertex("emissive_vertex"),
        "emissive_fragment",
        None,
        HDR_FORMAT,
    );
    let quad = pipeline(
        "game_quad_pipeline",
        custom_vertex("quad_vertex"),
        "quad_fragment",
        Some(BlendState::ALPHA_BLENDING),
        HDR_FORMAT,
    );
    let downsample = pipeline(
        "game_light_downsample_pipeline",
        fullscreen_vertex.clone(),
        "downsample_fragment",
        None,
        HDR_FORMAT,
    );
    let blur_h = pipeline(
        "game_light_blur_horizontal_pipeline",
        fullscreen_vertex.clone(),
        "blur_horizontal_fragment",
        None,
        HDR_FORMAT,
    );
    let blur_v = pipeline(
        "game_light_blur_vertical_pipeline",
        fullscreen_vertex.clone(),
        "blur_vertical_fragment",
        None,
        HDR_FORMAT,
    );
    let scene = pipeline(
        "game_scene_pipeline",
        fullscreen_vertex,
        "scene_fragment",
        None,
        HDR_FORMAT,
    );
    let line = pipeline(
        "game_debug_line_pipeline",
        custom_vertex("line_vertex"),
        "line_fragment",
        Some(BlendState::ALPHA_BLENDING),
        HDR_FORMAT,
    );
    let (palette, emissive_palette) = create_palette_textures(&device, &queue);
    let palette_view = palette.create_view(&TextureViewDescriptor::default());
    let emissive_palette_view = emissive_palette.create_view(&TextureViewDescriptor::default());
    let mut frame = UniformBuffer::from(FrameUniform::default());
    frame.set_label(Some("game_frame_uniform"));
    let mut blur = UniformBuffer::from(light_blur_params());
    blur.set_label(Some("game_blur_uniform"));
    let mut chunks = StorageBuffer::from(vec![ChunkInstance {
        world_origin: Vec2::ZERO,
        atlas_origin: UVec2::ZERO,
    }]);
    chunks.set_label(Some("game_chunk_instances"));
    let mut quads = StorageBuffer::from(vec![QuadInstance {
        center: Vec2::ZERO,
        size: Vec2::ZERO,
        color: Vec4::ZERO,
    }]);
    quads.set_label(Some("game_quad_instances"));
    let mut lines = StorageBuffer::from(vec![LineInstance {
        a: Vec2::ZERO,
        b: Vec2::ZERO,
        color: Vec4::ZERO,
    }]);
    lines.set_label(Some("game_line_instances"));
    commands.insert_resource(GameGpuData {
        frame,
        blur,
        chunks,
        quads,
        lines,
        chunk_generation: u64::MAX,
        _palette: palette,
        palette_view,
        _emissive_palette: emissive_palette,
        emissive_palette_view,
        atlas: Atlas::new(&device, 16, 0),
        targets: None,
        raster_bind_group: None,
        downsample_bind_group: None,
        blur_h_bind_group: None,
        blur_v_bind_group: None,
        scene_bind_group: None,
        star_view: None,
        fallback_star_view: fallback.texture_view.clone(),
        fallback_star_sampler: fallback.sampler.clone(),
        linear_sampler: device.create_sampler(&SamplerDescriptor {
            label: Some("game_linear_sampler"),
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..default()
        }),
    });
    commands.insert_resource(GamePipelines {
        layout,
        chunk,
        emissive,
        quad,
        downsample,
        blur_h,
        blur_v,
        scene,
        line,
    });
}

fn prepare_renderer(
    frame: Res<GameRenderFrame>,
    mut gpu: ResMut<GameGpuData>,
    pipelines: Res<GamePipelines>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    images: Res<RenderAssets<GpuImage>>,
    pipeline_cache: Res<PipelineCache>,
) {
    let atlas_changed =
        gpu.atlas.side != frame.atlas_side || gpu.atlas.generation != frame.atlas_generation;
    if atlas_changed {
        gpu.atlas = Atlas::new(&device, frame.atlas_side, frame.atlas_generation);
    }
    for upload in &frame.uploads {
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &gpu.atlas.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: upload.slot.x * CHUNK_SIZE as u32 + upload.rect.min_x as u32,
                    y: upload.slot.y * CHUNK_SIZE as u32 + upload.rect.min_y as u32,
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
    }
    let targets_changed = gpu
        .targets
        .as_ref()
        .is_none_or(|targets| targets.native != frame.native);
    if targets_changed {
        gpu.targets = Some(GameTargets::new(&device, frame.native));
    }
    gpu.frame.set(frame.uniform.clone());
    gpu.frame.write_buffer(&device, &queue);
    if gpu.blur.buffer().is_none() {
        gpu.blur.write_buffer(&device, &queue);
    }
    let buffer_ids = (
        gpu.chunks.buffer().map(Buffer::id),
        gpu.quads.buffer().map(Buffer::id),
        gpu.lines.buffer().map(Buffer::id),
    );
    if gpu.chunk_generation != frame.instance_generation {
        gpu.chunks.set(if frame.chunks.is_empty() {
            vec![ChunkInstance {
                world_origin: Vec2::ZERO,
                atlas_origin: UVec2::ZERO,
            }]
        } else {
            frame.chunks.clone()
        });
        gpu.chunks.write_buffer(&device, &queue);
        gpu.chunk_generation = frame.instance_generation;
    }
    gpu.quads.set(if frame.quads.is_empty() {
        vec![QuadInstance {
            center: Vec2::ZERO,
            size: Vec2::ZERO,
            color: Vec4::ZERO,
        }]
    } else {
        frame.quads.clone()
    });
    gpu.quads.write_buffer(&device, &queue);
    gpu.lines.set(if frame.lines.is_empty() {
        vec![LineInstance {
            a: Vec2::ZERO,
            b: Vec2::ZERO,
            color: Vec4::ZERO,
        }]
    } else {
        frame.lines.clone()
    });
    gpu.lines.write_buffer(&device, &queue);
    let buffers_changed = buffer_ids
        != (
            gpu.chunks.buffer().map(Buffer::id),
            gpu.quads.buffer().map(Buffer::id),
            gpu.lines.buffer().map(Buffer::id),
        );

    let (star_texture_view, star_sampler) = frame
        .stars
        .as_ref()
        .and_then(|handle| images.get(handle.id()))
        .map_or_else(
            || {
                (
                    gpu.fallback_star_view.clone(),
                    gpu.fallback_star_sampler.clone(),
                )
            },
            |stars| (stars.texture_view.clone(), stars.sampler.clone()),
        );
    let star_view = star_texture_view.id();
    let Some(targets) = gpu.targets.as_ref() else {
        return;
    };
    let bindings_missing = gpu.raster_bind_group.is_none()
        || gpu.downsample_bind_group.is_none()
        || gpu.blur_h_bind_group.is_none()
        || gpu.blur_v_bind_group.is_none()
        || gpu.scene_bind_group.is_none();
    if !atlas_changed
        && !targets_changed
        && !buffers_changed
        && !bindings_missing
        && gpu.star_view == Some(star_view)
    {
        return;
    }
    let layout = pipeline_cache.get_bind_group_layout(&pipelines.layout);
    let make_bind_group = |label, views: [&TextureView; 5]| {
        device.create_bind_group(
            label,
            &layout,
            &BindGroupEntries::sequential((
                gpu.frame.binding().expect("frame buffer written"),
                gpu.chunks.binding().expect("chunk buffer written"),
                gpu.quads.binding().expect("quad buffer written"),
                gpu.lines.binding().expect("line buffer written"),
                &gpu.atlas.view,
                &gpu.palette_view,
                &gpu.emissive_palette_view,
                views[0],
                views[1],
                views[2],
                views[3],
                views[4],
                &gpu.linear_sampler,
                &star_texture_view,
                &star_sampler,
                gpu.blur.binding().expect("blur buffer written"),
            )),
        )
    };
    let placeholder = &gpu.palette_view;
    let raster_bind_group = make_bind_group(
        "game_raster_bind_group",
        [
            placeholder,
            placeholder,
            placeholder,
            placeholder,
            placeholder,
        ],
    );
    let downsample_bind_group = make_bind_group(
        "game_downsample_bind_group",
        [
            placeholder,
            &targets.emission.view,
            placeholder,
            placeholder,
            placeholder,
        ],
    );
    let blur_h_bind_group = make_bind_group(
        "game_blur_h_bind_group",
        [
            placeholder,
            placeholder,
            &targets.quarter.view,
            placeholder,
            placeholder,
        ],
    );
    let blur_v_bind_group = make_bind_group(
        "game_blur_v_bind_group",
        [
            placeholder,
            placeholder,
            placeholder,
            &targets.blur_temp.view,
            placeholder,
        ],
    );
    let scene_bind_group = make_bind_group(
        "game_scene_bind_group",
        [
            &targets.world.view,
            &targets.emission.view,
            &targets.quarter.view,
            &targets.blur_temp.view,
            &targets.light.view,
        ],
    );
    gpu.raster_bind_group = Some(raster_bind_group);
    gpu.downsample_bind_group = Some(downsample_bind_group);
    gpu.blur_h_bind_group = Some(blur_h_bind_group);
    gpu.blur_v_bind_group = Some(blur_v_bind_group);
    gpu.scene_bind_group = Some(scene_bind_group);
    gpu.star_view = Some(star_view);
}

fn color_attachment(view: &TextureView, clear: Option<Color>) -> RenderPassColorAttachment<'_> {
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

fn pipeline(cache: &PipelineCache, id: CachedRenderPipelineId) -> Option<&RenderPipeline> {
    cache.get_render_pipeline(id)
}

fn render_game(
    view: ViewQuery<&ViewTarget>,
    frame: Res<GameRenderFrame>,
    gpu: Res<GameGpuData>,
    pipelines: Res<GamePipelines>,
    cache: Res<PipelineCache>,
    mut ctx: RenderContext,
) {
    if !frame.active {
        return;
    }
    let Some(raster_bind_group) = gpu.raster_bind_group.as_ref() else {
        return;
    };
    let Some(downsample_bind_group) = gpu.downsample_bind_group.as_ref() else {
        return;
    };
    let Some(blur_h_bind_group) = gpu.blur_h_bind_group.as_ref() else {
        return;
    };
    let Some(blur_v_bind_group) = gpu.blur_v_bind_group.as_ref() else {
        return;
    };
    let Some(scene_bind_group) = gpu.scene_bind_group.as_ref() else {
        return;
    };
    let Some(targets) = gpu.targets.as_ref() else {
        return;
    };
    let Some(chunk_pipeline) = pipeline(&cache, pipelines.chunk) else {
        return;
    };
    let Some(emissive_pipeline) = pipeline(&cache, pipelines.emissive) else {
        return;
    };
    let Some(quad_pipeline) = pipeline(&cache, pipelines.quad) else {
        return;
    };
    let Some(downsample_pipeline) = pipeline(&cache, pipelines.downsample) else {
        return;
    };
    let Some(blur_h_pipeline) = pipeline(&cache, pipelines.blur_h) else {
        return;
    };
    let Some(blur_v_pipeline) = pipeline(&cache, pipelines.blur_v) else {
        return;
    };
    let Some(scene_pipeline) = pipeline(&cache, pipelines.scene) else {
        return;
    };
    let Some(line_pipeline) = pipeline(&cache, pipelines.line) else {
        return;
    };
    {
        let mut pass = ctx
            .command_encoder()
            .begin_render_pass(&RenderPassDescriptor {
                label: Some("game_world_pass"),
                color_attachments: &[Some(color_attachment(
                    &targets.world.view,
                    Some(Color::NONE),
                ))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        pass.set_bind_group(0, raster_bind_group, &[]);
        if !frame.chunks.is_empty() {
            pass.set_pipeline(chunk_pipeline);
            pass.draw(0..6, 0..frame.chunks.len() as u32);
        }
        if !frame.quads.is_empty() {
            pass.set_pipeline(quad_pipeline);
            pass.draw(0..6, 0..frame.quads.len() as u32);
        }
    }
    {
        let mut pass = ctx
            .command_encoder()
            .begin_render_pass(&RenderPassDescriptor {
                label: Some("game_emission_pass"),
                color_attachments: &[Some(color_attachment(
                    &targets.emission.view,
                    Some(Color::NONE),
                ))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        if !frame.chunks.is_empty() {
            pass.set_pipeline(emissive_pipeline);
            pass.set_bind_group(0, raster_bind_group, &[]);
            pass.draw(0..6, 0..frame.chunks.len() as u32);
        }
    }
    for (label, target, selected, selected_bind_group) in [
        (
            "game_light_downsample_pass",
            &targets.quarter.view,
            downsample_pipeline,
            downsample_bind_group,
        ),
        (
            "game_light_horizontal_pass",
            &targets.blur_temp.view,
            blur_h_pipeline,
            blur_h_bind_group,
        ),
        (
            "game_light_vertical_pass",
            &targets.light.view,
            blur_v_pipeline,
            blur_v_bind_group,
        ),
    ] {
        let mut pass = ctx
            .command_encoder()
            .begin_render_pass(&RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(color_attachment(target, Some(Color::NONE)))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        pass.set_pipeline(selected);
        pass.set_bind_group(0, selected_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    let view_target = view.into_inner();
    {
        let mut pass = ctx
            .command_encoder()
            .begin_render_pass(&RenderPassDescriptor {
                label: Some("game_scene_pass"),
                color_attachments: &[Some(color_attachment(
                    view_target.main_texture_view(),
                    Some(Color::NONE),
                ))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        pass.set_pipeline(scene_pipeline);
        pass.set_bind_group(0, scene_bind_group, &[]);
        pass.draw(0..3, 0..1);
        if !frame.lines.is_empty() {
            pass.set_pipeline(line_pipeline);
            pass.draw(0..6, 0..frame.lines.len() as u32);
        }
    }
}
