use crate::region::DisjointRegionTileAccessor;

use super::chunk_ticket::ChunkTicketKey;

pub struct MyEntity {
    pub location: (i16, i16),
    pub velocity: (i16, i16),
    pub variant: MyEntityVariant,
}

pub enum MyEntityVariant {
    Player(Option<ChunkTicketKey>),
}

impl MyEntity {
    pub fn step(&mut self, tiles: DisjointRegionTileAccessor) {
        todo!()
    }

    pub fn apply_move(&mut self) -> (i32, i32) {
        todo!()
    }

    pub fn should_remove(&mut self) -> bool {
        todo!()
    }
}
