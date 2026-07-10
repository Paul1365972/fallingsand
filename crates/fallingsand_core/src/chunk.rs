use crate::cell::Cell;
use crate::coords::CellOffset;
use serde::{Deserialize, Serialize};

pub const CHUNK_SIZE: usize = 64;
pub const CHUNK_AREA: usize = CHUNK_SIZE * CHUNK_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirtyRect {
    pub min_x: u8,
    pub min_y: u8,
    pub max_x: u8,
    pub max_y: u8,
}

const fn min_u8(a: u8, b: u8) -> u8 {
    if a < b { a } else { b }
}

impl DirtyRect {
    pub const fn new(min_x: u8, min_y: u8, max_x: u8, max_y: u8) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    pub const EMPTY: Self = Self {
        min_x: u8::MAX,
        min_y: u8::MAX,
        max_x: 0,
        max_y: 0,
    };

    pub const FULL: Self = Self {
        min_x: 0,
        min_y: 0,
        max_x: (CHUNK_SIZE - 1) as u8,
        max_y: (CHUNK_SIZE - 1) as u8,
    };

    pub const fn is_empty(self) -> bool {
        self.min_x > self.max_x || self.min_y > self.max_y
    }

    pub fn mark(&mut self, offset: CellOffset) {
        self.min_x = self.min_x.min(offset.x);
        self.min_y = self.min_y.min(offset.y);
        self.max_x = self.max_x.max(offset.x);
        self.max_y = self.max_y.max(offset.y);
    }

    fn mark_neighbourhood(&mut self, offset: CellOffset) {
        let last = (CHUNK_SIZE - 1) as u8;
        self.min_x = self.min_x.min(offset.x.saturating_sub(1));
        self.min_y = self.min_y.min(offset.y.saturating_sub(1));
        self.max_x = self.max_x.max(min_u8(offset.x + 1, last));
        self.max_y = self.max_y.max(min_u8(offset.y + 1, last));
    }

    fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }

    pub const fn width(self) -> u32 {
        if self.is_empty() {
            0
        } else {
            (self.max_x - self.min_x) as u32 + 1
        }
    }

    pub const fn height(self) -> u32 {
        if self.is_empty() {
            0
        } else {
            (self.max_y - self.min_y) as u32 + 1
        }
    }
}

#[derive(Debug, Clone)]
pub struct Chunk {
    cells: Box<[Cell; CHUNK_AREA]>,
    pub change: DirtyRect,
    pub prev_change: DirtyRect,
    pub sim: DirtyRect,
    pub prev_sim: DirtyRect,
    pub sleeping: bool,
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            cells: Box::new([Cell::AIR; CHUNK_AREA]),
            change: DirtyRect::EMPTY,
            prev_change: DirtyRect::EMPTY,
            sim: DirtyRect::EMPTY,
            prev_sim: DirtyRect::EMPTY,
            sleeping: true,
        }
    }

    #[inline]
    pub fn get(&self, offset: CellOffset) -> Cell {
        self.cells[offset.index()]
    }

    #[inline]
    pub fn get_mut(&mut self, offset: CellOffset) -> &mut Cell {
        &mut self.cells[offset.index()]
    }

    #[inline]
    pub fn set(&mut self, offset: CellOffset, cell: Cell) {
        self.cells[offset.index()] = cell;
        self.change.mark(offset);
        self.sim.mark_neighbourhood(offset);
    }

    pub fn cells(&self) -> &[Cell; CHUNK_AREA] {
        &self.cells
    }

    pub fn cells_mut(&mut self) -> &mut [Cell; CHUNK_AREA] {
        &mut self.cells
    }

    pub fn swap_rects(&mut self) {
        self.prev_change = self.change;
        self.change = DirtyRect::EMPTY;
        self.prev_sim = self.sim;
        self.sim = DirtyRect::EMPTY;
    }

    pub fn change_rect(&self) -> DirtyRect {
        self.change.union(self.prev_change)
    }

    pub fn sim_rect(&self) -> DirtyRect {
        self.sim.union(self.prev_sim)
    }

    fn normalize_updated(&mut self, tick: u8) {
        for cell in self.cells.iter_mut() {
            cell.updated = tick;
        }
    }

    pub fn wake(&mut self, tick: u8) {
        if self.sleeping {
            self.normalize_updated(tick);
            self.sleeping = false;
        }
    }
}
