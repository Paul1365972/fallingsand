use crate::cell::cell::SimulationCell;
use crate::util::coords::{CellCoords, TILES_PER_CHUNK};

use crate::world::GlobalContext;

#[derive(Debug, Copy, Clone, Default)]
pub struct MyTile {
    pub variant: MyTileVariant,
    pub clock: u8,
    pub temperature: u16, // fix point (4bit) 0K - 4096K with 0.0625 steps (water freezes at 4371 and boils at 5971, Temp of TNT explosion 56000)
}

#[derive(Debug, Copy, Clone, Default)]
pub enum MyTileVariant {
    #[default]
    NIL,
    AIR,
    SAND,
    STONE,
    WATER,
}

impl<'a> SimulationCell<'a> {
    pub fn step(&mut self, ctx: &GlobalContext) {
        let start = TILES_PER_CHUNK;
        let end = 3 * TILES_PER_CHUNK;
        for y in start..end {
            for x in start..end {
                let coords = CellCoords::new(x, y);
                self.handle_tile(coords, ctx);
            }
        }
    }

    fn handle_tile(&mut self, coords: CellCoords, ctx: &GlobalContext) {
        let tile = self.get(coords);
        self.get_mut(coords).temperature += 3;
        match tile.variant {
            MyTileVariant::SAND => {
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
            MyTileVariant::AIR | MyTileVariant::WATER => {
                self.swap(src, dst);
                true
            }
            _ => false,
        }
    }
}
