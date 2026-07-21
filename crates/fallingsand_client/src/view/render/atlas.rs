use crate::game::world::ChunkChange;
use crate::view::Game;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;
use fallingsand_core::{CHUNK_AREA, CHUNK_SIZE, Cell, CellOffset, ChunkPos, DirtyRect};

pub(super) const INITIAL_ATLAS_SIDE: u32 = 16;

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
pub(crate) struct ChunkAtlasState {
    slots: HashMap<ChunkPos, AtlasSlot>,
    uploads: usize,
    upload_bytes: usize,
    atlas_side: u32,
    atlas_generation: u64,
    instance_generation: u64,
    free: Vec<AtlasSlot>,
    pending: Vec<ChunkUpload>,
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
    pub(crate) fn uploads(&self) -> usize {
        self.uploads
    }

    pub(crate) fn upload_bytes(&self) -> usize {
        self.upload_bytes
    }

    pub(crate) fn live_chunks(&self) -> usize {
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
    pub(super) world_origin: Vec2,
    pub(super) atlas_origin: UVec2,
}

impl ChunkInstance {
    fn new(pos: ChunkPos, slot: AtlasSlot) -> Self {
        Self {
            world_origin: Vec2::new(
                (pos.x * CHUNK_SIZE as i32) as f32,
                (pos.y * CHUNK_SIZE as i32) as f32,
            ),
            atlas_origin: UVec2::new(slot.x, slot.y) * CHUNK_SIZE as u32,
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
