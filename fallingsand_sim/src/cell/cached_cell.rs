use crate::chunk::TileChunk;

use super::cell::SimulationCell;

pub struct CachedSimulationCell {
    pointers: [*mut TileChunk; 4 * 4],
}

unsafe impl Send for CachedSimulationCell {}
unsafe impl Sync for CachedSimulationCell {}

impl CachedSimulationCell {
    pub fn new(pointers: [*mut TileChunk; 4 * 4]) -> Self {
        Self { pointers }
    }

    pub fn promote<'a>(&'a mut self) -> SimulationCell<'a> {
        let ptrs = self.pointers;
        // Alternative to std::mem::transmute(self.references)
        let references: [&'a mut TileChunk; 4 * 4] = [
            unsafe { &mut *ptrs[0] },
            unsafe { &mut *ptrs[1] },
            unsafe { &mut *ptrs[2] },
            unsafe { &mut *ptrs[3] },
            unsafe { &mut *ptrs[4] },
            unsafe { &mut *ptrs[5] },
            unsafe { &mut *ptrs[6] },
            unsafe { &mut *ptrs[7] },
            unsafe { &mut *ptrs[8] },
            unsafe { &mut *ptrs[9] },
            unsafe { &mut *ptrs[10] },
            unsafe { &mut *ptrs[11] },
            unsafe { &mut *ptrs[12] },
            unsafe { &mut *ptrs[13] },
            unsafe { &mut *ptrs[14] },
            unsafe { &mut *ptrs[15] },
        ];
        SimulationCell { references }
    }
}
