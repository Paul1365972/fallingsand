use std::marker::PhantomData;

use crate::{region::DisjointRegion, cell::cell::TileTransitionFn, chunk_ticket::ChunkTicketKey, Entity, Tile};

pub struct RegionSimulationConfig<T, F> where F: TileTransitionFn<T> {
    tile_transition_fn: F,
    marker: PhantomData<T>,
}

enum DisjointRegionCommand {
    StepTiles,
    StepEntities,
    AddChunk,
}

enum DisjointRegionResult {
    TicketEntityMoved(ChunkTicketKey, (i32, i32)),
    TicketEntityRemoved(ChunkTicketKey),
}

// impl<T: Tile, E: Entity<M, R>, M, R> DisjointRegion<T, E> {
    // pub fn apply_command<F, G>(&mut self, command: DisjointRegionCommand, config: &RegionSimulationConfig<T, G>, result_handler: F)
    // where F: FnMut(DisjointRegionResult),
    // G: TileTransitionFn<T>,
    // {
        // match command {
            // DisjointRegionCommand::StepTiles => self.step_tiles(config.tile_transition_fn),
        // }
    // }
// }
