use std::num::NonZeroU16;

use itertools::Itertools;
use rustc_hash::FxHashSet;

use crate::{coords::WorldChunkCoords, aabb::AABB};


#[derive(Hash, PartialEq, Eq)]
pub struct ChunkTicketKey(NonZeroU16);

pub struct ChunkTicket {
    key: ChunkTicketKey,
    coords: WorldChunkCoords,
    shape: ChunkTicketShape,
}

enum ChunkTicketShape {
    RECT(u8)
}

impl ChunkTicket {
    pub fn new(key: ChunkTicketKey, coords: WorldChunkCoords, shape: ChunkTicketShape) -> Self { Self { key, coords, shape } }

    pub fn translate(&mut self, offset: (i32, i32)) -> ChunkTicketTransition {
        let original = &self.coords;
        self.coords = original + offset;
        match self.shape {
            ChunkTicketShape::RECT(size) => {
                let before = FxHashSet::from_iter(AABB::from_radius(size as i32).iter().map(|x| original + x));
                let after = FxHashSet::from_iter(AABB::from_radius(size as i32).iter().map(|x| &self.coords + x));
                let load = &before - &after;
                let unload = &before - &after;
                ChunkTicketTransition { load, unload }
            },
        }
    }

    pub fn build_chunk_list(&self) -> ChunkTicketTransition {
        match self.shape {
            ChunkTicketShape::RECT(size ) => {
                let result = ChunkTicketTransition::default();
                result.load.extend(AABB::from_radius(size as i32).iter().map(|offset| &self.coords + offset));
                result
            },
        }
    }
}

#[derive(Default)]
pub struct ChunkTicketTransition {
    load: FxHashSet<WorldChunkCoords>,
    unload: FxHashSet<WorldChunkCoords>,
}

impl ChunkTicketTransition {
    pub fn load(&self) -> &FxHashSet<WorldChunkCoords> {
        &self.load
    }

    pub fn unload(&self) -> &FxHashSet<WorldChunkCoords> {
        &self.unload
    }
}
