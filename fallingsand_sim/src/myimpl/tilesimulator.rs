use std::hash::{Hash, Hasher};

use rustc_hash::FxHasher;

use crate::{
    cell::cell::SimulationCell,
    coords::{CellCoords, TILES_PER_CHUNK},
};

use super::tile::{MyTile, Variant};

#[derive(Default)]
pub struct Context {
    pub tick: u32,
}

impl Context {
    fn next_u64(&self, coords: CellCoords) -> u64 {
        let mut hasher = FxHasher::default();
        self.tick.hash(&mut hasher);
        coords.hash(&mut hasher);
        hasher.finish()
    }
}

impl<'a> SimulationCell<'a, MyTile> {
    pub fn step(&mut self, ctx: &Context) {
        let start = TILES_PER_CHUNK;
        let end = 0u8.wrapping_sub(TILES_PER_CHUNK);
        for y in start..end {
            for x in start..end {
                let coords = CellCoords::new(x, y);
                self.handle_tile(coords, ctx);
            }
        }
    }

    fn handle_tile(&mut self, coords: CellCoords, ctx: &Context) {
        let tile = self.get(coords);
        self.get_mut(coords).temperature += 3;
        match tile.variant {
            Variant::SAND => {
                let below = coords.below();
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
            Variant::AIR | Variant::WATER => {
                self.swap(src, dst);
                true
            }
            _ => false,
        }
    }
}
