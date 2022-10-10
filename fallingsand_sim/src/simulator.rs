use itertools::Itertools;
use rayon::{prelude::{IntoParallelRefMutIterator, ParallelIterator}, vec};
use rustc_hash::FxHashMap;

use crate::{
    coords::{ChunkCoords, WorldChunkCoords},
    region::DisjointRegion,
    util::DrainFilterMap,
 chunk_ticket::ChunkTicketKey, Entity, cell::cell::{SimulationCell, CellBuilder, TileTransitionFn}, Tile,
};


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

impl<T, E> DisjointRegion<T, E> {
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

impl<T: Tile, E> DisjointRegion<T, E> {
    pub fn step_tiles<F>(&mut self, tile_transition_fn: F)
    where
        F: TileTransitionFn<T>,
    {
        for offset in [(0, 0), (0, 2), (2, 0), (2, 2)] {
            let mut substep = TileSimulationSubStep::new(self, offset);
            substep.step_tiles(tile_transition_fn.clone());
        }
        // To do maybe collect tile events here and apply them
    }
}

pub trait DisjointRegionStepEntities<E: Entity<M, R>, M, R> {
    fn step_entities<FM, FR>(&mut self, move_notifier: FM, remove_notifier: FR)     where
    FM: FnMut(M),
    FR: FnMut(R);
}

impl<T, E: Entity<M, R>, M, R> DisjointRegionStepEntities<E, M, R> for DisjointRegion<T, E> {

    fn step_entities<FM, FR>(&mut self, move_notifier: FM, remove_notifier: FR)     where
    FM: FnMut(M),
    FR: FnMut(R),
    {
        // Do step entity stuff here

        // Move and remove entities
        self.retain_entities(|k, v| { 
            let remove = v.entity.should_remove_and_notify();
            if let Some(event) = remove {
                self.get_mut(v.chunk_coords).unwrap().entity_chunk_mut().entities_mut().remove(k);
                remove_notifier(event);
                return false
            }
            true
        });

        for (key, value) in self.entities_mut() {
            let movement = e.apply_move();
        }

        let moved = self.entities_mut().iter_mut().for_each(
            |e| e.apply_move(),
            |_, &o| o != (0, 0),
            |e, o| (e, o),
        );

            // events.extend(moved.iter().filter_map(|(e, o)| {
                // e.get_chunk_ticket()
                    // .map(|x| EntityStepEvent::TicketEntityMoved(x, *o))
            // }));

            // moved.into_iter().for_each(|(e, o)| {
                // self.get_mut(coords + o)
                    // .unwrap()
                    // .entity_chunk_mut()
                    // .entities_mut()
                    // .push(e);
            // });
        });
    }
}
