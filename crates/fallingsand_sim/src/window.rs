use fallingsand_core::{CHUNK_SIZE, Cell, CellPos, Chunk, ChunkPos, DirtyRect};

pub const WINDOW_CHUNKS: i32 = 4;
pub const WINDOW_SLOTS: usize = (WINDOW_CHUNKS * WINDOW_CHUNKS) as usize;
pub const SPEED_OF_LIGHT: i32 = CHUNK_SIZE as i32;
const _: () = assert!(SPEED_OF_LIGHT as usize == CHUNK_SIZE && WINDOW_CHUNKS == 4);

pub struct SimWindow {
    origin: ChunkPos,
    slots: [Option<Chunk>; WINDOW_SLOTS],
    tick: u64,
    structural: Vec<CellPos>,
    damage: Vec<CellPos>,
}

pub(crate) struct WindowParts {
    pub origin: ChunkPos,
    pub slots: [Option<Chunk>; WINDOW_SLOTS],
    pub structural: Vec<CellPos>,
    pub damage: Vec<CellPos>,
}

impl SimWindow {
    pub(crate) fn new(origin: ChunkPos, slots: [Option<Chunk>; WINDOW_SLOTS], tick: u64) -> Self {
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
            origin: self.origin,
            slots: self.slots,
            structural: self.structural,
            damage: self.damage,
        }
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
        self.slots[(sy * WINDOW_CHUNKS + sx) as usize].as_ref()
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
        chunk.wake((self.tick as u8).wrapping_sub(1));
        let old = chunk.get(pos.offset());
        chunk.set(pos.offset(), cell);
        if old.is_body() && !cell.is_body() {
            self.damage.push(pos);
        }
    }

    pub fn mark(&mut self, pos: CellPos) {
        let Some(slot) = self.slot_of(pos) else {
            return;
        };
        let Some(chunk) = self.slots[slot].as_mut() else {
            return;
        };
        chunk.wake((self.tick as u8).wrapping_sub(1));
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

    pub(crate) fn wake_chunk(&mut self, sx: i32, sy: i32) {
        let idx = (sy * WINDOW_CHUNKS + sx) as usize;
        if let Some(chunk) = self.slots[idx].as_mut() {
            chunk.wake((self.tick as u8).wrapping_sub(1));
        }
    }
}

pub(crate) fn spill(rect: DirtyRect, dx: i32, dy: i32) -> DirtyRect {
    if rect.is_empty() {
        return DirtyRect::EMPTY;
    }
    let size = CHUNK_SIZE as i32;
    let min_x = (rect.min_x as i32 + dx * size - 1).max(0);
    let min_y = (rect.min_y as i32 + dy * size - 1).max(0);
    let max_x = (rect.max_x as i32 + dx * size + 1).min(size - 1);
    let max_y = (rect.max_y as i32 + dy * size + 1).min(size - 1);
    if min_x > max_x || min_y > max_y {
        DirtyRect::EMPTY
    } else {
        DirtyRect::new(min_x as u8, min_y as u8, max_x as u8, max_y as u8)
    }
}
