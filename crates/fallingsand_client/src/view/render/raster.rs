use super::extract::PixelViewport;
use super::primitives::WorldQuad;
use super::targets::RenderTargets;
use super::{color_attachment, pipeline, populated, queue_pipeline};
use crate::game::world::ChunkChange;
use crate::view::Game;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::binding_types::{
    storage_buffer_read_only, texture_2d, uniform_buffer,
};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue};
use fallingsand_core::content;
use fallingsand_core::{CHUNK_AREA, CHUNK_SIZE, Cell, CellOffset, ChunkPos, DirtyRect};

const INITIAL_ATLAS_SIDE: u32 = 16;
const SHADES: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AtlasSlot {
    pub x: u32,
    pub y: u32,
}

pub(super) struct ChunkUpload {
    pub slot: AtlasSlot,
    pub rect: DirtyRect,
    pub data: Vec<u8>,
}

#[derive(Resource)]
pub struct ChunkAtlasState {
    slots: HashMap<ChunkPos, AtlasSlot>,
    uploads: usize,
    upload_bytes: usize,
    atlas_side: u32,
    atlas_generation: u64,
    instance_generation: u64,
    free: Vec<AtlasSlot>,
    pub(super) pending: Vec<ChunkUpload>,
}

impl Default for ChunkAtlasState {
    fn default() -> Self {
        let mut state = Self {
            slots: HashMap::default(),
            uploads: 0,
            upload_bytes: 0,
            atlas_side: INITIAL_ATLAS_SIDE,
            atlas_generation: 0,
            instance_generation: 0,
            free: Vec::new(),
            pending: Vec::new(),
        };
        state.add_slots(0, INITIAL_ATLAS_SIDE);
        state
    }
}

impl ChunkAtlasState {
    pub fn uploads(&self) -> usize {
        self.uploads
    }

    pub fn upload_bytes(&self) -> usize {
        self.upload_bytes
    }

    pub fn live_chunks(&self) -> usize {
        self.slots.len()
    }

    fn add_slots(&mut self, old_side: u32, new_side: u32) {
        for y in 0..new_side {
            for x in 0..new_side {
                if x >= old_side || y >= old_side {
                    self.free.push(AtlasSlot { x, y });
                }
            }
        }
    }

    fn allocate(&mut self) -> AtlasSlot {
        if let Some(slot) = self.free.pop() {
            return slot;
        }
        let old_side = self.atlas_side;
        self.atlas_side *= 2;
        self.atlas_generation = self.atlas_generation.wrapping_add(1);
        self.add_slots(old_side, self.atlas_side);
        self.free.pop().expect("grown atlas has slots")
    }

    fn clear(&mut self) {
        self.slots.clear();
        self.pending.clear();
        self.free.clear();
        self.atlas_side = INITIAL_ATLAS_SIDE;
        self.add_slots(0, INITIAL_ATLAS_SIDE);
        self.atlas_generation = self.atlas_generation.wrapping_add(1);
        self.instance_generation = self.instance_generation.wrapping_add(1);
    }
}

fn pack_rect(cells: &[Cell; CHUNK_AREA], rect: DirtyRect) -> Vec<u8> {
    let mut data = Vec::with_capacity((rect.width() * rect.height() * 4) as usize);
    for y in rect.min_y..=rect.max_y {
        for x in rect.min_x..=rect.max_x {
            let cell = cells[CellOffset::new(x, y).index()];
            data.extend_from_slice(&cell.material.0.to_le_bytes());
            data.push(cell.shade);
            data.push(0);
        }
    }
    data
}

enum UploadPlan {
    Full,
    Rects(Vec<DirtyRect>),
}

pub(super) fn sync_chunk_atlas(mut game: ResMut<Game>, mut state: ResMut<ChunkAtlasState>) {
    state.uploads = 0;
    state.upload_bytes = 0;

    let Some(ingame) = game.0.ingame_mut() else {
        if !state.slots.is_empty() || state.atlas_side != INITIAL_ATLAS_SIDE {
            state.clear();
        }
        return;
    };
    let changes = ingame.world.take_changes();
    if changes.is_empty() {
        return;
    }

    let mut plans: HashMap<ChunkPos, UploadPlan> = HashMap::default();
    for change in changes {
        match change {
            ChunkChange::Cleared => {
                state.clear();
                plans.clear();
            }
            ChunkChange::Loaded(pos) => {
                plans.insert(pos, UploadPlan::Full);
            }
            ChunkChange::Unloaded(pos) => {
                plans.remove(&pos);
                if let Some(slot) = state.slots.remove(&pos) {
                    state.free.push(slot);
                    state.instance_generation = state.instance_generation.wrapping_add(1);
                }
            }
            ChunkChange::Delta(pos, rect) => match plans.get_mut(&pos) {
                Some(UploadPlan::Full) => {}
                Some(UploadPlan::Rects(rects)) => rects.push(rect),
                None => {
                    plans.insert(pos, UploadPlan::Rects(vec![rect]));
                }
            },
        }
    }

    let old_generation = state.atlas_generation;
    for (&pos, plan) in &plans {
        if matches!(plan, UploadPlan::Full) && !state.slots.contains_key(&pos) {
            let slot = state.allocate();
            state.slots.insert(pos, slot);
            state.instance_generation = state.instance_generation.wrapping_add(1);
        }
    }

    if state.atlas_generation != old_generation {
        state.pending.clear();
        let live: Vec<_> = state
            .slots
            .iter()
            .map(|(&pos, &slot)| (pos, slot))
            .collect();
        for (pos, slot) in live {
            if let Some(chunk) = ingame.world.chunks.get(&pos) {
                let data = pack_rect(&chunk.cells, DirtyRect::FULL);
                state.uploads += 1;
                state.upload_bytes += data.len();
                state.pending.push(ChunkUpload {
                    slot,
                    rect: DirtyRect::FULL,
                    data,
                });
            }
        }
        return;
    }

    for (pos, plan) in plans {
        let Some(chunk) = ingame.world.chunks.get(&pos) else {
            continue;
        };
        let Some(&slot) = state.slots.get(&pos) else {
            continue;
        };
        let rects = match plan {
            UploadPlan::Full => vec![DirtyRect::FULL],
            UploadPlan::Rects(rects) => rects,
        };
        for rect in rects {
            let data = pack_rect(&chunk.cells, rect);
            state.uploads += 1;
            state.upload_bytes += data.len();
            state.pending.push(ChunkUpload { slot, rect, data });
        }
    }
}

#[derive(Clone, ShaderType)]
pub(super) struct ChunkInstance {
    world_origin: Vec2,
    atlas_origin: UVec2,
}

impl ChunkInstance {
    pub(super) fn new(pos: ChunkPos, slot: AtlasSlot) -> Self {
        Self {
            world_origin: Vec2::new(
                (pos.x * CHUNK_SIZE as i32) as f32,
                (pos.y * CHUNK_SIZE as i32) as f32,
            ),
            atlas_origin: UVec2::new(slot.x, slot.y) * CHUNK_SIZE as u32,
        }
    }
}

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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        &mut self,
        frame: &RasterFrame,
        chunks: &[ChunkInstance],
        quads: &[QuadInstance],
        uploads: &[ChunkUpload],
        atlas_side: u32,
        atlas_generation: u64,
        instance_generation: u64,
        device: &RenderDevice,
        queue: &RenderQueue,
        cache: &PipelineCache,
    ) {
        let atlas_changed =
            self.atlas.side != atlas_side || self.atlas.generation != atlas_generation;
        if atlas_changed {
            self.atlas = Atlas::new(device, atlas_side, atlas_generation);
        }
        for upload in uploads {
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
        self.frame.set(frame.clone());
        self.frame.write_buffer(device, queue);
        let chunk_buffer = self.chunks.buffer().map(Buffer::id);
        let quad_buffer = self.quads.buffer().map(Buffer::id);
        if self.chunk_generation != instance_generation {
            self.chunks.set(populated(
                chunks,
                ChunkInstance {
                    world_origin: Vec2::ZERO,
                    atlas_origin: UVec2::ZERO,
                },
            ));
            self.chunks.write_buffer(device, queue);
            self.chunk_generation = instance_generation;
        }
        self.quads.set(populated(
            quads,
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

pub(super) struct AtlasSnapshot {
    pub chunks: Vec<ChunkInstance>,
    pub uploads: Vec<ChunkUpload>,
    pub side: u32,
    pub atlas_generation: u64,
    pub instance_generation: u64,
}

impl ChunkAtlasState {
    pub(super) fn extract(&mut self, previous_generation: u64) -> AtlasSnapshot {
        let chunks = if previous_generation == self.instance_generation {
            Vec::new()
        } else {
            self.slots
                .iter()
                .map(|(&pos, &slot)| ChunkInstance::new(pos, slot))
                .collect()
        };
        AtlasSnapshot {
            chunks,
            uploads: std::mem::take(&mut self.pending),
            side: self.atlas_side,
            atlas_generation: self.atlas_generation,
            instance_generation: self.instance_generation,
        }
    }
}
