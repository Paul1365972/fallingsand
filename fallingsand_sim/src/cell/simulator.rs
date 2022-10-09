use itertools::Itertools;
use rayon::prelude::{IntoParallelRefMutIterator, ParallelIterator};
use rustc_hash::FxHashMap;

use crate::{
    coords::{ChunkCoords, WorldChunkCoords},
    region::DisjointRegion,
    util::DrainFilterMap,
    world::{Entity}, chunk_ticket::ChunkTicketKey,
};

use super::cell::{CellBuilder, SimulationCell, TileTransitionFn};

pub struct TileSimulationSubStep<'a, T> {
    cells: Vec<SimulationCell<'a, T>>,
}

impl<'a, T: Send> TileSimulationSubStep<'a, T> {
    fn new<E>(
        field: &'a mut DisjointRegion<T, E>,
        offset: (i32, i32),
    ) -> TileSimulationSubStep<'a, T> {
        let mut cell_map = FxHashMap::default();
        for (k, v) in field.chunks_iter_mut() {
            let coords = k.to_world_cell_coords(offset);
            let values = cell_map.entry(coords).or_insert_with(|| CellBuilder::new());
            values.add_unique(k.to_cell_chunk_index(offset), v.tile_chunk_mut());
        }
        let cells = cell_map
            .into_iter()
            .map(|e| e.1.build())
            .flatten()
            .collect_vec();
        TileSimulationSubStep { cells }
    }

    fn step_tiles<F>(&mut self, tile_transition_fn: F)
    where
        F: TileTransitionFn<T>,
    {
        // self.cells.iter_mut().for_each(|x| transition_function.clone()(x));
        self.cells
            .par_iter_mut()
            .for_each(|x: &mut SimulationCell<T>| tile_transition_fn.clone()(x));
    }
}

#[derive(Debug)]
pub struct ActiveChunks {
    chunks: Vec<WorldChunkCoords>,
}

impl ActiveChunks {
    fn new(chunks: Vec<WorldChunkCoords>) -> Self {
        Self { chunks }
    }

    fn get_chunks(&self) -> &Vec<WorldChunkCoords> {
        &self.chunks
    }
}

pub struct EntityStepResult {
    events: Vec<EntityStepEvent>,
}

impl EntityStepResult {
    pub fn events(&self) -> &[EntityStepEvent] {
        self.events.as_ref()
    }
}

pub enum EntityStepEvent {
    TicketEntityMoved(ChunkTicketKey, (i32, i32)),
    TicketEntityRemoved(ChunkTicketKey),
}

impl<T: Send, E: Entity> DisjointRegion<T, E> {
    pub fn step_tiles<F>(&mut self, tile_transition_fn: F)
    where
        F: TileTransitionFn<T>,
    {
        for offset in [(0, 0), (0, 2), (2, 0), (2, 2)] {
            let mut substep = TileSimulationSubStep::new(self, offset);
            substep.step_tiles(tile_transition_fn.clone());
        }
    }

    pub fn step_entities(&mut self) -> EntityStepResult {
        // Do step entity stuff

        let events = Vec::new();

        self.chunks_iter_mut().for_each(|(coords, chunk)| {
            let removed = chunk
                .entity_chunk_mut()
                .entities_mut()
                .drain_filter(|e| e.should_remove());
            events.extend(
                removed
                    .filter_map(|x| x.get_chunk_ticket())
                    .map(|x| EntityStepEvent::TicketEntityRemoved(x)),
            );

            let moved = chunk.entity_chunk_mut().entities_mut().drain_filter_map(
                |e| e.apply_move(),
                |_, o| o != (0, 0),
                |e, o| (e, o),
            );

            events.extend(moved.iter().filter_map(|(e, o)| {
                e.get_chunk_ticket()
                    .map(|x| EntityStepEvent::TicketEntityMoved(x, *o))
            }));

            moved.into_iter().for_each(|(e, o)| {
                self.get_mut(coords + o)
                    .unwrap()
                    .entity_chunk_mut()
                    .entities_mut()
                    .push(e);
            });
        });
        EntityStepResult { events }
    }

    pub fn build_active_chunks(&self) -> ActiveChunks {
        let mut chunks = Vec::new();
        for offset in [(0, 0), (0, 2), (2, 0), (2, 2)] {
            let mut cell_map = FxHashMap::default();
            for (k, _) in self.chunks_iter() {
                let coords = k.to_world_cell_coords(offset);
                let chunks = cell_map.entry(coords).or_insert(0);
                *chunks += 1;
            }

            let chunks_coords = cell_map
                .into_iter()
                .filter(|x| x.1 == 16)
                .map(|((x, y), _)| (x * 4 + offset.0, y * 4 + offset.1))
                .flat_map(|(x, y)| {
                    [
                        (x + 1, y + 1),
                        (x + 1, y + 2),
                        (x + 2, y + 1),
                        (x + 2, y + 2),
                    ]
                })
                .map(|(x, y)| WorldChunkCoords::new(x, y));
            chunks.extend(chunks_coords)
        }
        chunks.sort();
        ActiveChunks::new(chunks)
    }
}
