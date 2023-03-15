use itertools::Itertools;
use rayon::prelude::{IntoParallelRefMutIterator, ParallelIterator};
use rustc_hash::FxHashMap;

use crate::{
    cell::cell::{CellBuilder, SimulationCell},
    region::DisjointRegion,
    world::GlobalContext,
};

pub struct TileSimulationSubStep<'a> {
    cells: Vec<SimulationCell<'a>>,
}

impl<'a> TileSimulationSubStep<'a> {
    pub fn new(field: &'a mut DisjointRegion, offset: (i32, i32)) -> TileSimulationSubStep<'a> {
        let mut cell_map = FxHashMap::default();
        field.for_tile_chunk_mut(|k, v| {
            let coords = k.to_world_cell_coords(offset);
            let values = cell_map.entry(coords).or_insert_with(|| CellBuilder::new());
            values.add_unique(k.to_cell_chunk_index(offset), v);
        });
        let cells = cell_map
            .into_iter()
            .map(|e| e.1.build())
            .flatten()
            .collect_vec();
        TileSimulationSubStep { cells }
    }

    pub fn step_tiles(&mut self, ctx: &GlobalContext) {
        self.cells.par_iter_mut().for_each(|cell| cell.step(ctx));
    }
}
