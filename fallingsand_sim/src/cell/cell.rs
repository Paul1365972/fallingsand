use std::{
    mem::{self, MaybeUninit},
    ptr,
};

use crate::{chunk::TileChunk, coords::CellCoords};

pub trait TileTransitionFn<T>: FnMut(&mut SimulationCell<T>) + Send + Sync + Clone {}

impl<U, T> TileTransitionFn<T> for U where U: FnMut(&mut SimulationCell<T>) + Send + Sync + Clone {}

pub struct SimulationCell<'a, T> {
    chunks: [&'a mut TileChunk<T>; 4 * 4],
}

impl<'a, T: Clone> SimulationCell<'a, T> {
    pub fn get(&self, coords: CellCoords) -> T {
        self.chunks[coords.to_cell_chunk_index()]
            .get(coords.to_chunk_coords())
            .clone()
    }

    pub fn get_mut(&mut self, coords: CellCoords) -> &mut T {
        self.chunks[coords.to_cell_chunk_index()].get_mut(coords.to_chunk_coords())
    }

    pub fn swap(&mut self, a: CellCoords, b: CellCoords) {
        unsafe {
            let pa: *mut T = self.get_mut(a);
            let pb: *mut T = self.get_mut(b);
            ptr::swap(pa, pb);
        }
    }
}

pub(crate) struct CellBuilder<'a, T> {
    chunks: [MaybeUninit<&'a mut TileChunk<T>>; 4 * 4],
    init: u16,
}

impl<'a, T> CellBuilder<'a, T> {
    pub fn new() -> CellBuilder<'a, T> {
        CellBuilder {
            chunks: unsafe { MaybeUninit::uninit().assume_init() },
            init: 0,
        }
    }

    pub fn add_unique(&mut self, index: usize, chunk: &'a mut TileChunk<T>) {
        assert!(index <= 16);
        assert!(self.init & (1 << index) == 0);
        self.chunks[index] = MaybeUninit::new(chunk);
        self.init |= 1 << index;
    }

    pub fn build(self) -> Option<SimulationCell<'a, T>> {
        if self.init == 0xFFFF {
            Some(SimulationCell {
                chunks: unsafe { mem::transmute::<_, [&'a mut TileChunk<T>; 4 * 4]>(self.chunks) },
            })
        } else {
            None
        }
    }
}
