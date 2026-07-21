use super::atlas::{ChunkInstance, INITIAL_ATLAS_SIDE};
use super::extract::{PixelViewport, RasterExtract};
use super::primitives::WorldQuad;
use super::targets::RenderTargets;
use super::{color_attachment, pipeline, populated, queue_pipeline};
use bevy::prelude::*;
use bevy::render::render_resource::binding_types::{
    storage_buffer_read_only, texture_2d, uniform_buffer,
};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue};
use fallingsand_core::CHUNK_SIZE;
use fallingsand_core::content;

const SHADES: u32 = 16;

#[derive(Clone, ShaderType)]
pub(super) struct QuadInstance {
    center: Vec2,
    size: Vec2,
    color: Vec4,
}

impl From<WorldQuad> for QuadInstance {
    fn from(quad: WorldQuad) -> Self {
        Self {
            center: quad.center,
            size: quad.size,
            color: quad.color,
        }
    }
}

#[derive(Clone, ShaderType)]
pub(super) struct RasterFrame {
    pub viewport: PixelViewport,
    pub world_snapped: Vec2,
    pub emission_size: Vec2,
    pub time: f32,
}

impl Default for RasterFrame {
    fn default() -> Self {
        Self {
            viewport: default(),
            world_snapped: Vec2::ZERO,
            emission_size: Vec2::ONE,
            time: 0.0,
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

#[derive(Resource)]
pub(super) struct RasterPass {
    layout: BindGroupLayoutDescriptor,
    chunk_pipeline: CachedRenderPipelineId,
    emission_pipeline: CachedRenderPipelineId,
    quad_pipeline: CachedRenderPipelineId,
    frame: UniformBuffer<RasterFrame>,
    chunks: StorageBuffer<Vec<ChunkInstance>>,
    quads: StorageBuffer<Vec<QuadInstance>>,
    chunk_generation: u64,
    _palette: Texture,
    palette_view: TextureView,
    _emissive_palette: Texture,
    emissive_palette_view: TextureView,
    atlas: Atlas,
    bind_group: Option<BindGroup>,
}

impl RasterPass {
    pub(super) fn new(
        device: &RenderDevice,
        queue: &RenderQueue,
        asset_server: &AssetServer,
        cache: &PipelineCache,
    ) -> Self {
        let layout = BindGroupLayoutDescriptor::new(
            "game_raster_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::VERTEX_FRAGMENT,
                (
                    uniform_buffer::<RasterFrame>(false),
                    storage_buffer_read_only::<Vec<ChunkInstance>>(false),
                    storage_buffer_read_only::<Vec<QuadInstance>>(false),
                    texture_2d(TextureSampleType::Uint),
                    texture_2d(TextureSampleType::Float { filterable: false }),
                    texture_2d(TextureSampleType::Float { filterable: false }),
                ),
            ),
        );
        let shader = asset_server.load("shaders/raster.wgsl");
        let vertex = |entry: &'static str| VertexState {
            shader: shader.clone(),
            entry_point: Some(entry.into()),
            ..default()
        };
        let chunk_pipeline = queue_pipeline(
            cache,
            "game_chunk_pipeline",
            vec![layout.clone()],
            vertex("chunk_vertex"),
            shader.clone(),
            "chunk_fragment",
            Some(BlendState::ALPHA_BLENDING),
        );
        let emission_pipeline = queue_pipeline(
            cache,
            "game_emission_pipeline",
            vec![layout.clone()],
            vertex("emissive_vertex"),
            shader.clone(),
            "emissive_fragment",
            None,
        );
        let quad_pipeline = queue_pipeline(
            cache,
            "game_quad_pipeline",
            vec![layout.clone()],
            vertex("quad_vertex"),
            shader,
            "quad_fragment",
            Some(BlendState::ALPHA_BLENDING),
        );
        let (palette, emissive_palette) = create_palette_textures(device, queue);
        let palette_view = palette.create_view(&TextureViewDescriptor::default());
        let emissive_palette_view = emissive_palette.create_view(&TextureViewDescriptor::default());
        let mut frame = UniformBuffer::from(RasterFrame::default());
        frame.set_label(Some("game_raster_frame"));
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
        Self {
            layout,
            chunk_pipeline,
            emission_pipeline,
            quad_pipeline,
            frame,
            chunks,
            quads,
            chunk_generation: u64::MAX,
            _palette: palette,
            palette_view,
            _emissive_palette: emissive_palette,
            emissive_palette_view,
            atlas: Atlas::new(device, INITIAL_ATLAS_SIDE, 0),
            bind_group: None,
        }
    }

    pub(super) fn prepare(
        &mut self,
        input: &RasterExtract,
        device: &RenderDevice,
        queue: &RenderQueue,
        cache: &PipelineCache,
    ) {
        let atlas_changed =
            self.atlas.side != input.atlas_side || self.atlas.generation != input.atlas_generation;
        if atlas_changed {
            self.atlas = Atlas::new(device, input.atlas_side, input.atlas_generation);
        }
        for upload in &input.uploads {
            queue.write_texture(
                TexelCopyTextureInfo {
                    texture: &self.atlas.texture,
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
        self.frame.set(input.frame.clone());
        self.frame.write_buffer(device, queue);
        let chunk_buffer = self.chunks.buffer().map(Buffer::id);
        let quad_buffer = self.quads.buffer().map(Buffer::id);
        if self.chunk_generation != input.instance_generation {
            self.chunks.set(populated(
                &input.chunks,
                ChunkInstance {
                    world_origin: Vec2::ZERO,
                    atlas_origin: UVec2::ZERO,
                },
            ));
            self.chunks.write_buffer(device, queue);
            self.chunk_generation = input.instance_generation;
        }
        self.quads.set(populated(
            &input.quads,
            QuadInstance {
                center: Vec2::ZERO,
                size: Vec2::ZERO,
                color: Vec4::ZERO,
            },
        ));
        self.quads.write_buffer(device, queue);
        let buffers_changed = chunk_buffer != self.chunks.buffer().map(Buffer::id)
            || quad_buffer != self.quads.buffer().map(Buffer::id);
        if atlas_changed || buffers_changed || self.bind_group.is_none() {
            let layout = cache.get_bind_group_layout(&self.layout);
            self.bind_group = Some(device.create_bind_group(
                "game_raster_bind_group",
                &layout,
                &BindGroupEntries::sequential((
                    self.frame.binding().expect("raster frame written"),
                    self.chunks.binding().expect("chunk buffer written"),
                    self.quads.binding().expect("quad buffer written"),
                    &self.atlas.view,
                    &self.palette_view,
                    &self.emissive_palette_view,
                )),
            ));
        }
    }

    pub(super) fn deactivate(
        &mut self,
        atlas_side: u32,
        atlas_generation: u64,
        device: &RenderDevice,
    ) {
        if self.atlas.side == atlas_side && self.atlas.generation == atlas_generation {
            return;
        }
        self.atlas = Atlas::new(device, atlas_side, atlas_generation);
        self.chunk_generation = u64::MAX;
        self.bind_group = None;
    }

    pub(super) fn draw(
        &self,
        context: &mut RenderContext,
        targets: &RenderTargets,
        chunk_count: u32,
        quad_count: u32,
        cache: &PipelineCache,
    ) {
        let Some(bind_group) = self.bind_group.as_ref() else {
            return;
        };
        let Some(chunk_pipeline) = pipeline(cache, self.chunk_pipeline) else {
            return;
        };
        let Some(emission_pipeline) = pipeline(cache, self.emission_pipeline) else {
            return;
        };
        let Some(quad_pipeline) = pipeline(cache, self.quad_pipeline) else {
            return;
        };
        {
            let mut pass = context
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
            pass.set_bind_group(0, bind_group, &[]);
            if chunk_count > 0 {
                pass.set_pipeline(chunk_pipeline);
                pass.draw(0..6, 0..chunk_count);
            }
            if quad_count > 0 {
                pass.set_pipeline(quad_pipeline);
                pass.draw(0..6, 0..quad_count);
            }
        }
        let mut pass = context
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
        if chunk_count > 0 {
            pass.set_pipeline(emission_pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..6, 0..chunk_count);
        }
    }
}
