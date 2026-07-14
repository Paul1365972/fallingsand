use fallingsand_core::{CHUNK_SIZE, Cell, CellPos, Chunk, ChunkPos};

pub const WINDOW_CHUNKS: i32 = 4;
pub const WINDOW_SLOTS: usize = (WINDOW_CHUNKS * WINDOW_CHUNKS) as usize;
pub const SPEED_OF_LIGHT: i32 = CHUNK_SIZE as i32;
const _: () = assert!(SPEED_OF_LIGHT as usize == CHUNK_SIZE && WINDOW_CHUNKS == 4);

pub struct SimWindow<'a> {
    origin: ChunkPos,
    slots: [Option<&'a mut Chunk>; WINDOW_SLOTS],
    tick: u64,
    structural: Vec<CellPos>,
    damage: Vec<CellPos>,
}

pub(crate) struct WindowParts {
    pub structural: Vec<CellPos>,
    pub damage: Vec<CellPos>,
}

impl<'a> SimWindow<'a> {
    pub(crate) fn new(
        origin: ChunkPos,
        slots: [Option<&'a mut Chunk>; WINDOW_SLOTS],
        tick: u64,
    ) -> Self {
        Self {
            origin,
            slots,
            tick,
            structural: Vec::new(),
            damage: Vec::new(),
        }
    }

    pub(crate) fn into_parts(self) -> WindowParts {
        WindowParts {
            structural: self.structural,
            damage: self.damage,
        }
    }

    pub(crate) fn set_slot(&mut self, sx: i32, sy: i32, chunk: &'a mut Chunk) {
        self.slots[(sy * WINDOW_CHUNKS + sx) as usize] = Some(chunk);
    }

    pub fn note_structural(&mut self, pos: CellPos) {
        self.structural.push(pos);
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
        debug_assert!(
            (0..WINDOW_CHUNKS).contains(&sx) && (0..WINDOW_CHUNKS).contains(&sy),
            "speed-of-light ({SPEED_OF_LIGHT}) violation: access at {pos:?} escapes window at {:?}",
            self.origin
        );
        if !(0..WINDOW_CHUNKS).contains(&sx) || !(0..WINDOW_CHUNKS).contains(&sy) {
            return None;
        }
        Some((sy * WINDOW_CHUNKS + sx) as usize)
    }

    pub fn get(&self, pos: CellPos) -> Option<Cell> {
        let slot = self.slot_of(pos)?;
        self.slots[slot].as_ref().map(|c| c.get(pos.offset()))
    }

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
        if old.is_body() && !cell.is_body() {
            self.damage.push(pos);
        }
        self.mark_sim_border(pos);
    }

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

    pub fn swap(&mut self, a: CellPos, b: CellPos) {
        let (Some(mut cell_a), Some(mut cell_b)) = (self.get(a), self.get(b)) else {
            debug_assert!(false, "swap with unloaded cell");
            return;
        };
        let tick_byte = self.tick as u8;
        cell_a.updated = tick_byte;
        cell_b.updated = tick_byte;
        self.set(a, cell_b);
        self.set(b, cell_a);
    }
}
