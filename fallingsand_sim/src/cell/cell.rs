use std::ptr;

use super::tile::{MyTile, MyTileVariant};
use crate::{
    chunk::TileChunk,
    util::coords::{CellCoords, TILES_PER_CHUNK},
    world::GlobalContext,
};

pub struct SimulationCell<'a> {
    pub(super) references: [&'a mut TileChunk; 4 * 4],
}

impl<'a> SimulationCell<'a> {
    pub fn step(&mut self, ctx: &GlobalContext) {
        self.sub_step(ctx, 1, 1);
        self.sub_step(ctx, 2, 1);
        self.sub_step(ctx, 1, 2);
        self.sub_step(ctx, 2, 2);
    }

    fn sub_step(&mut self, ctx: &GlobalContext, dx: usize, dy: usize) {
        let chunk = &self.references[dx + dy * 4];
        let min_x = (dx * TILES_PER_CHUNK) as u16 + chunk.bounds.0 as u16;
        let min_y = (dy * TILES_PER_CHUNK) as u16 + chunk.bounds.1 as u16;
        let max_x = (dx * TILES_PER_CHUNK) as u16 + chunk.bounds.2 as u16;
        let max_y = (dy * TILES_PER_CHUNK) as u16 + chunk.bounds.3 as u16;
        for y in min_y..max_y {
            for x in min_x..max_x {
                self.handle_tile(CellCoords::new(x, y), ctx);
            }
        }
    }

    fn handle_tile(&mut self, coords: CellCoords, ctx: &GlobalContext) {
        let tile = self.get_mut(coords);
        if tile.last_update == ctx.ticks as u8 {
            return;
        }
        tile.last_update = ctx.ticks as u8;
        //self.get_mut(coords).temperature += 3;
        match tile.variant {
            MyTileVariant::SAND => {
                let below = if ctx.next_u64(coords) & 2 == 0 {
                    coords.above()
                } else {
                    coords.below()
                };
                if !self.try_swap_solid(coords, below) {
                    if ctx.next_u64(coords) & 1 == 0 {
                        if !self.try_swap_solid(coords, below.left()) {
                            self.try_swap_solid(coords, below.right());
                        }
                    } else {
                        if !self.try_swap_solid(coords, below.right()) {
                            self.try_swap_solid(coords, below.left());
                        }
                    }
                }
            }
            _ => (),
        }
    }

    fn try_swap_solid(&mut self, src: CellCoords, dst: CellCoords) -> bool {
        let dst_tile = self.get(dst);
        match dst_tile.variant {
            MyTileVariant::AIR | MyTileVariant::WATER => {
                self.swap(src, dst);
                true
            }
            _ => false,
        }
    }

    pub fn get(&self, coords: CellCoords) -> MyTile {
        self.references[coords.to_cell_index()].tiles[coords.to_chunk_index()]
    }

    pub fn get_mut(&mut self, coords: CellCoords) -> &mut MyTile {
        &mut self.references[coords.to_cell_index()].tiles[coords.to_chunk_index()]
    }

    pub fn swap(&mut self, a: CellCoords, b: CellCoords) {
        unsafe {
            let pa: *mut MyTile = self.get_mut(a);
            let pb: *mut MyTile = self.get_mut(b);
            ptr::swap(pa, pb);
        }
    }
}
