use crate::game::world::ChunkChange;
use crate::view::Game;
use bevy::platform::collections::HashMap;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;
use fallingsand_core::{
    CHUNK_AREA, CHUNK_SIZE, Cell, CellOffset, ChunkPos, DirtyRect, FOG_CHUNK_SIDE,
    FOG_CHUNK_TEXELS, FogMask,
};

pub(super) const INITIAL_ATLAS_SIDE: u32 = 16;
const FOG_FADE_SECS: f32 = 0.18;

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

pub(super) struct FogUpload {
    pub slot: AtlasSlot,
    pub data: Vec<u8>,
}

struct ChunkEntry {
    slot: AtlasSlot,
    fade: Box<[u8; FOG_CHUNK_TEXELS]>,
    fading: bool,
}

impl ChunkEntry {
    fn new(slot: AtlasSlot) -> Self {
        Self {
            slot,
            fade: Box::new([0; FOG_CHUNK_TEXELS]),
            fading: false,
        }
    }

    fn snap(&mut self, fog: &FogMask) {
        for (index, value) in self.fade.iter_mut().enumerate() {
            *value = if fog.get(index) { u8::MAX } else { 0 };
        }
        self.fading = false;
    }

    fn advance(&mut self, fog: &FogMask, step: u8) -> bool {
        let mut moved = false;
        let mut pending = false;
        for (index, value) in self.fade.iter_mut().enumerate() {
            if !fog.get(index) || *value == u8::MAX {
                continue;
            }
            *value = value.saturating_add(step);
            moved = true;
            pending |= *value < u8::MAX;
        }
        self.fading = pending;
        moved
    }
}

#[derive(Resource)]
pub(crate) struct ChunkAtlasState {
    slots: HashMap<ChunkPos, ChunkEntry>,
    uploads: usize,
    upload_bytes: usize,
    sync_micros: u32,
    atlas_side: u32,
    atlas_generation: u64,
    instance_generation: u64,
    free: Vec<AtlasSlot>,
    pending: Vec<ChunkUpload>,
    fog_pending: Vec<FogUpload>,
}

impl Default for ChunkAtlasState {
    fn default() -> Self {
        let mut state = Self {
            slots: HashMap::default(),
            uploads: 0,
            upload_bytes: 0,
            sync_micros: 0,
            atlas_side: INITIAL_ATLAS_SIDE,
            atlas_generation: 0,
            instance_generation: 0,
            free: Vec::new(),
            pending: Vec::new(),
            fog_pending: Vec::new(),
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

    pub(crate) fn sync_micros(&self) -> u32 {
        self.sync_micros
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
        self.fog_pending.clear();
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
                .map(|(&pos, entry)| ChunkInstance::new(pos, entry.slot))
                .collect()
        };
        AtlasSnapshot {
            chunks,
            uploads: std::mem::take(&mut self.pending),
            fog_uploads: std::mem::take(&mut self.fog_pending),
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
    Rect(DirtyRect),
}

pub(super) fn sync_chunk_atlas(
    mut game: ResMut<Game>,
    mut state: ResMut<ChunkAtlasState>,
    time: Res<Time>,
) {
    let start = Instant::now();
    collect_uploads(&mut game, &mut state, &time);
    state.sync_micros = start.elapsed().as_micros() as u32;
}

fn collect_uploads(game: &mut Game, state: &mut ChunkAtlasState, time: &Time) {
    state.uploads = 0;
    state.upload_bytes = 0;

    let Some(ingame) = game.0.ingame_mut() else {
        if !state.slots.is_empty() || state.atlas_side != INITIAL_ATLAS_SIDE {
            state.clear();
        }
        return;
    };

    let mut plans: HashMap<ChunkPos, UploadPlan> = HashMap::default();
    let mut revealed: Vec<ChunkPos> = Vec::new();
    for change in ingame.world.take_changes() {
        match change {
            ChunkChange::Cleared => {
                state.clear();
                plans.clear();
                revealed.clear();
            }
            ChunkChange::Loaded(pos) => {
                plans.insert(pos, UploadPlan::Full);
            }
            ChunkChange::Unloaded(pos) => {
                plans.remove(&pos);
                if let Some(entry) = state.slots.remove(&pos) {
                    state.free.push(entry.slot);
                    state.instance_generation = state.instance_generation.wrapping_add(1);
                }
            }
            ChunkChange::Delta(pos, rect) => match plans.get_mut(&pos) {
                Some(UploadPlan::Full) => {}
                Some(UploadPlan::Rect(merged)) => *merged = merged.union(rect),
                None => {
                    plans.insert(pos, UploadPlan::Rect(rect));
                }
            },
            ChunkChange::Revealed(pos) => revealed.push(pos),
        }
    }

    let old_generation = state.atlas_generation;
    for (&pos, plan) in &plans {
        if matches!(plan, UploadPlan::Full) && !state.slots.contains_key(&pos) {
            let slot = state.allocate();
            state.slots.insert(pos, ChunkEntry::new(slot));
            state.instance_generation = state.instance_generation.wrapping_add(1);
        }
    }

    let grown = state.atlas_generation != old_generation;
    if grown {
        state.pending.clear();
        state.fog_pending.clear();
        plans.clear();
        for &pos in ingame.world.chunks.keys() {
            plans.insert(pos, UploadPlan::Full);
        }
    }

    for (pos, plan) in plans {
        let Some(chunk) = ingame.world.chunks.get(&pos) else {
            continue;
        };
        let Some(entry) = state.slots.get_mut(&pos) else {
            continue;
        };
        let slot = entry.slot;
        let rect = match plan {
            UploadPlan::Full => {
                entry.snap(&chunk.fog);
                let data = entry.fade.to_vec();
                state.fog_pending.push(FogUpload { slot, data });
                DirtyRect::FULL
            }
            UploadPlan::Rect(rect) => rect,
        };
        if rect.is_empty() {
            continue;
        }
        let data = pack_rect(&chunk.cells, rect);
        state.uploads += 1;
        state.upload_bytes += data.len();
        state.pending.push(ChunkUpload { slot, rect, data });
    }

    for pos in revealed {
        if let Some(entry) = state.slots.get_mut(&pos) {
            entry.fading = true;
        }
    }

    let step = (u8::MAX as f32 * time.delta_secs() / FOG_FADE_SECS).ceil() as u8;
    let fading: Vec<ChunkPos> = state
        .slots
        .iter()
        .filter(|(_, entry)| entry.fading)
        .map(|(&pos, _)| pos)
        .collect();
    for pos in fading {
        let Some(chunk) = ingame.world.chunks.get(&pos) else {
            continue;
        };
        let Some(entry) = state.slots.get_mut(&pos) else {
            continue;
        };
        if !entry.advance(&chunk.fog, step.max(1)) {
            continue;
        }
        let data = entry.fade.to_vec();
        let slot = entry.slot;
        state.fog_pending.push(FogUpload { slot, data });
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

pub(super) const fn fog_atlas_dimension(side: u32) -> u32 {
    side * FOG_CHUNK_SIDE as u32
}

pub(super) struct AtlasSnapshot {
    pub chunks: Vec<ChunkInstance>,
    pub uploads: Vec<ChunkUpload>,
    pub fog_uploads: Vec<FogUpload>,
    pub side: u32,
    pub atlas_generation: u64,
    pub instance_generation: u64,
}
