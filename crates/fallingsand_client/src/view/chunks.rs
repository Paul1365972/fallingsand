use super::Game;
use crate::game::world::ChunkChange;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use fallingsand_core::{CHUNK_AREA, Cell, CellOffset, ChunkPos, DirtyRect};

const INITIAL_ATLAS_SIDE: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasSlot {
    pub x: u32,
    pub y: u32,
}

pub struct ChunkUpload {
    pub slot: AtlasSlot,
    pub rect: DirtyRect,
    pub data: Vec<u8>,
}

#[derive(Resource)]
pub struct ChunkRenderState {
    pub chunk_entities: HashMap<ChunkPos, AtlasSlot>,
    pub uploads: usize,
    pub upload_bytes: usize,
    pub atlas_side: u32,
    pub atlas_generation: u64,
    pub instance_generation: u64,
    free: Vec<AtlasSlot>,
    pub(crate) pending: Vec<ChunkUpload>,
}

impl Default for ChunkRenderState {
    fn default() -> Self {
        let mut state = Self {
            chunk_entities: HashMap::default(),
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

impl ChunkRenderState {
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
        self.chunk_entities.clear();
        self.pending.clear();
        self.free.clear();
        self.add_slots(0, self.atlas_side);
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

enum Plan {
    Full,
    Rects(Vec<DirtyRect>),
}

pub fn sync_chunks(mut game: ResMut<Game>, mut state: ResMut<ChunkRenderState>) {
    state.uploads = 0;
    state.upload_bytes = 0;

    let Some(ingame) = game.0.ingame_mut() else {
        if !state.chunk_entities.is_empty() {
            state.clear();
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
                state.clear();
                plans.clear();
            }
            ChunkChange::Loaded(pos) => {
                plans.insert(pos, Plan::Full);
            }
            ChunkChange::Unloaded(pos) => {
                plans.remove(&pos);
                if let Some(slot) = state.chunk_entities.remove(&pos) {
                    state.free.push(slot);
                    state.instance_generation = state.instance_generation.wrapping_add(1);
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

    let old_generation = state.atlas_generation;
    for (&pos, plan) in &plans {
        if matches!(plan, Plan::Full) && !state.chunk_entities.contains_key(&pos) {
            let slot = state.allocate();
            state.chunk_entities.insert(pos, slot);
            state.instance_generation = state.instance_generation.wrapping_add(1);
        }
    }

    if state.atlas_generation != old_generation {
        state.pending.clear();
        let live: Vec<_> = state
            .chunk_entities
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
        let Some(&slot) = state.chunk_entities.get(&pos) else {
            continue;
        };
        let rects = match plan {
            Plan::Full => vec![DirtyRect::FULL],
            Plan::Rects(rects) => rects,
        };
        for rect in rects {
            let data = pack_rect(&chunk.cells, rect);
            state.uploads += 1;
            state.upload_bytes += data.len();
            state.pending.push(ChunkUpload { slot, rect, data });
        }
    }
}
