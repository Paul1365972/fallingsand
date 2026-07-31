use crate::world::obstructs;
use fallingsand_core::{CHUNK_SIZE, Cell, CellPos, Chunk, ChunkPos, content};

pub const WINDOW_CHUNKS: i32 = 4;
pub const WINDOW_SLOTS: usize = (WINDOW_CHUNKS * WINDOW_CHUNKS) as usize;
pub const SPEED_OF_LIGHT: i32 = CHUNK_SIZE as i32;
const _: () = assert!(SPEED_OF_LIGHT as usize <= ((WINDOW_CHUNKS as usize - 2) / 2) * CHUNK_SIZE);

#[derive(Debug, Clone, Copy)]
pub enum BodyImpulse {
    Strike {
        id: u32,
        body_cell: CellPos,
        grain: CellPos,
        mass: i64,
    },
    Load {
        id: u32,
        body_cell: CellPos,
        jy: i64,
    },
}

pub struct SimWindow<'a> {
    origin: ChunkPos,
    slots: [Option<&'a mut Chunk>; WINDOW_SLOTS],
    pub(crate) impulses: Vec<BodyImpulse>,
    pub(crate) unseated: Vec<CellPos>,
}

impl<'a> SimWindow<'a> {
    pub(crate) fn new(origin: ChunkPos, slots: [Option<&'a mut Chunk>; WINDOW_SLOTS]) -> Self {
        Self {
            origin,
            slots,
            impulses: Vec::new(),
            unseated: Vec::new(),
        }
    }

    pub(crate) fn struck_body(&mut self, mover: Cell, grain: CellPos, body_cell: CellPos) -> bool {
        let Some(id) = self.get(body_cell).and_then(|cell| cell.body_id()) else {
            return false;
        };
        self.impulses.push(BodyImpulse::Strike {
            id,
            body_cell,
            grain,
            mass: i64::from(content::density_milli(mover.material).max(1)),
        });
        true
    }

    pub(crate) fn load_body(&mut self, id: u32, body_cell: CellPos, jy: i64) {
        self.impulses.push(BodyImpulse::Load { id, body_cell, jy });
    }

    pub(crate) fn set_slot(&mut self, sx: i32, sy: i32, chunk: &'a mut Chunk) {
        self.slots[(sy * WINDOW_CHUNKS + sx) as usize] = Some(chunk);
    }

    pub(crate) const fn origin(&self) -> ChunkPos {
        self.origin
    }

    pub(crate) fn chunk_at(&self, sx: i32, sy: i32) -> Option<&Chunk> {
        if !(0..WINDOW_CHUNKS).contains(&sx) || !(0..WINDOW_CHUNKS).contains(&sy) {
            return None;
        }
        self.slots[(sy * WINDOW_CHUNKS + sx) as usize].as_deref()
    }

    fn slot_of(&self, pos: CellPos) -> Option<usize> {
        let chunk = pos.chunk();
        let sx = chunk.x.wrapping_sub(self.origin.x);
        let sy = chunk.y.wrapping_sub(self.origin.y);
        let in_window = (0..WINDOW_CHUNKS).contains(&sx) && (0..WINDOW_CHUNKS).contains(&sy);
        debug_assert!(
            in_window,
            "speed-of-light ({SPEED_OF_LIGHT}) violation: access at {pos:?} escapes window at {:?}",
            self.origin
        );
        if !in_window {
            return None;
        }
        Some((sy * WINDOW_CHUNKS + sx) as usize)
    }

    pub fn get(&self, pos: CellPos) -> Option<Cell> {
        let slot = self.slot_of(pos)?;
        self.slots[slot].as_ref().map(|c| c.get(pos.offset()))
    }

    #[inline]
    pub fn set(&mut self, pos: CellPos, cell: Cell) {
        let Some(slot) = self.slot_of(pos) else {
            return;
        };
        let Some(chunk) = self.slots[slot].as_mut() else {
            debug_assert!(false, "write to unloaded chunk at {pos:?}");
            return;
        };
        let old = chunk.get(pos.offset());
        chunk.set(pos.offset(), cell);
        if obstructs(old.material) && !obstructs(cell.material) {
            self.unseated.push(pos);
        }
        self.mark_sim_border(pos);
    }

    pub fn transform(&mut self, pos: CellPos, mut cell: Cell) {
        let Some(slot) = self.slot_of(pos) else {
            return;
        };
        let Some(chunk) = self.slots[slot].as_mut() else {
            debug_assert!(false, "write to unloaded chunk at {pos:?}");
            return;
        };
        let old = chunk.get(pos.offset());
        match old.body_id() {
            Some(id) => cell.set_body(id),
            None => cell.clear_body(),
        }
        chunk.set(pos.offset(), cell);
        self.mark_sim_border(pos);
        let bondable = |cell: Cell| content::bond_group(cell.material) != u8::MAX;
        if old.material != cell.material
            && (obstructs(old.material) != obstructs(cell.material)
                || bondable(old)
                || bondable(cell))
        {
            self.unseated.push(pos);
        }
    }

    #[inline]
    fn mark_sim_border(&mut self, pos: CellPos) {
        let off = pos.offset();
        let last = (CHUNK_SIZE - 1) as u8;
        if off.x != 0 && off.x != last && off.y != 0 && off.y != last {
            return;
        }
        let home = pos.chunk();
        for dy in -1..=1 {
            for dx in -1..=1 {
                let n = pos.translated(dx, dy);
                if n.chunk() == home {
                    continue;
                }
                if let Some(slot) = self.slot_of(n)
                    && let Some(chunk) = self.slots[slot].as_mut()
                {
                    chunk.sim.mark(n.offset());
                }
            }
        }
    }

    pub fn mark(&mut self, pos: CellPos) {
        let Some(slot) = self.slot_of(pos) else {
            return;
        };
        let Some(chunk) = self.slots[slot].as_mut() else {
            return;
        };
        chunk.sim.mark(pos.offset());
    }

    pub fn swap(&mut self, mover: CellPos, target: CellPos) {
        let (Some(mut moving), Some(mut displaced)) = (self.get(mover), self.get(target)) else {
            debug_assert!(false, "swap with unloaded cell");
            return;
        };
        moving.set_moved();
        moving.clear_stressed();
        displaced.set_moved();
        displaced.clear_stressed();
        self.set(mover, displaced);
        self.set(target, moving);
    }
}
