use crate::world::{Unseated, obstructs, rigid_seed, unseated};
use fallingsand_core::{CHUNK_SIZE, Cell, CellPos, Chunk, ChunkPos};

pub const WINDOW_CHUNKS: i32 = 4;
pub const WINDOW_SLOTS: usize = (WINDOW_CHUNKS * WINDOW_CHUNKS) as usize;
pub const SPEED_OF_LIGHT: i32 = CHUNK_SIZE as i32;
const _: () = assert!(SPEED_OF_LIGHT as usize <= ((WINDOW_CHUNKS as usize - 2) / 2) * CHUNK_SIZE);

pub struct SimWindow<'a> {
    origin: ChunkPos,
    slots: [Option<&'a mut Chunk>; WINDOW_SLOTS],
    events: &'a mut WindowEvents,
}

#[derive(Default)]
pub(crate) struct WindowEvents {
    unseated: Vec<CellPos>,
}

impl WindowEvents {
    pub(crate) fn clear(&mut self) {
        self.unseated.clear();
    }

    pub(crate) fn drain_unseated(&mut self) -> impl Iterator<Item = CellPos> + '_ {
        self.unseated.drain(..)
    }
}

impl<'a> SimWindow<'a> {
    pub(crate) fn new(
        origin: ChunkPos,
        slots: [Option<&'a mut Chunk>; WINDOW_SLOTS],
        events: &'a mut WindowEvents,
    ) -> Self {
        Self {
            origin,
            slots,
            events,
        }
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
        chunk.set(pos.offset(), cell);
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
        cell.set_body(old.is_body() && obstructs(cell.material));
        chunk.set(pos.offset(), cell);
        self.mark_sim_border(pos);
        self.note(pos, old, cell);
    }

    fn note(&mut self, pos: CellPos, old: Cell, new: Cell) {
        match unseated(old, new) {
            Unseated::Nothing => {}
            Unseated::Written => self.unseat(pos),
            Unseated::Neighbours => {
                for around in pos.neighbourhood() {
                    self.unseat(around);
                }
            }
        }
    }

    fn unseat(&mut self, pos: CellPos) {
        if self.get(pos).is_some_and(rigid_seed) {
            self.events.unseated.push(pos);
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
        moving.flags |= Cell::MOVED;
        displaced.flags |= Cell::MOVED;
        self.set(mover, displaced);
        self.set(target, moving);
        self.note(mover, moving, displaced);
        self.note(target, displaced, moving);
    }
}
