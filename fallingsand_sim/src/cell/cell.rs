use std::{
    mem::{self, MaybeUninit},
    ptr,
};

use crate::{chunk::TileChunk, util::coords::CellCoords};

use super::tile::MyTile;

pub struct SimulationCell<'a> {
    chunks: [&'a mut TileChunk; 4 * 4],
}

impl<'a> SimulationCell<'a> {
    pub fn get(&self, coords: CellCoords) -> MyTile {
        self.chunks[coords.to_cell_chunk_index()]
            .get(coords.to_chunk_coords())
            .clone()
    }

    pub fn get_mut(&mut self, coords: CellCoords) -> &mut MyTile {
        self.chunks[coords.to_cell_chunk_index()].get_mut(coords.to_chunk_coords())
    }

    pub fn swap(&mut self, a: CellCoords, b: CellCoords) {
        unsafe {
            let pa: *mut MyTile = self.get_mut(a);
            let pb: *mut MyTile = self.get_mut(b);
            ptr::swap(pa, pb);
        }
    }
}

pub struct CellBuilder<'a> {
    chunks: [MaybeUninit<&'a mut TileChunk>; 4 * 4],
    init: u16,
}

impl<'a> CellBuilder<'a> {
    pub fn new() -> CellBuilder<'a> {
        CellBuilder {
            chunks: unsafe { MaybeUninit::uninit().assume_init() },
            init: 0,
        }
    }

    pub fn add_unique(&mut self, index: usize, chunk: &'a mut TileChunk) {
        assert!(index <= 16);
        assert!(self.init & (1 << index) == 0);
        self.chunks[index] = MaybeUninit::new(chunk);
        self.init |= 1 << index;
    }

    pub fn build(self) -> Option<SimulationCell<'a>> {
        if self.init == 0xFFFF {
            Some(SimulationCell {
                chunks: unsafe { mem::transmute::<_, [&'a mut TileChunk; 4 * 4]>(self.chunks) },
            })
        } else {
            None
        }
    }
}
