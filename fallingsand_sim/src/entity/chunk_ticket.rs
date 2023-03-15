use std::num::NonZeroU16;

use rustc_hash::FxHashSet;

use crate::util::{aabb::AABB, coords::WorldChunkCoords};

#[derive(Hash, PartialEq, Eq, Clone)]
pub struct ChunkTicketKey(NonZeroU16);

pub struct ChunkTicket {
    key: ChunkTicketKey,
    coords: WorldChunkCoords,
    shape: ChunkTicketShape,
}

pub enum ChunkTicketShape {
    RECT(u8),
}

impl ChunkTicket {
    pub fn new_rect(key: ChunkTicketKey, coords: WorldChunkCoords, size: u8) -> Self {
        assert!(size >= 2);
        Self {
            key,
            coords,
            shape: ChunkTicketShape::RECT(size),
        }
    }

    pub fn translate(&mut self, offset: (i32, i32)) -> ChunkTicketTransition {
        let original = &self.coords + (0, 0);
        let updated = &original + offset;
        self.coords = updated;
        match self.shape {
            ChunkTicketShape::RECT(size) => {
                let before = FxHashSet::from_iter(
                    AABB::from_radius(size as i32).iter().map(|x| &original + x),
                );
                let after = FxHashSet::from_iter(
                    AABB::from_radius(size as i32).iter().map(|x| &updated + x),
                );
                let load = &before - &after;
                let unload = &after - &before;
                ChunkTicketTransition { load, unload }
            }
        }
    }

    pub fn build_chunk_list(&self) -> ChunkTicketTransition {
        match self.shape {
            ChunkTicketShape::RECT(size) => {
                let mut result = ChunkTicketTransition::default();
                result.load.extend(
                    AABB::from_radius(size as i32)
                        .iter()
                        .map(|offset| &self.coords + offset),
                );
                result
            }
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