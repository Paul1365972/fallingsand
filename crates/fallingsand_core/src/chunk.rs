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

    pub fn union(self, other: Self) -> Self {
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

    pub const fn contains(self, offset: CellOffset) -> bool {
        offset.x >= self.min_x
            && offset.x <= self.max_x
            && offset.y >= self.min_y
            && offset.y <= self.max_y
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

    pub const fn expanded(self, n: u8) -> Self {
        if self.is_empty() {
            return self;
        }
        Self {
            min_x: self.min_x.saturating_sub(n),
            min_y: self.min_y.saturating_sub(n),
            max_x: min_u8(self.max_x.saturating_add(n), (CHUNK_SIZE - 1) as u8),
            max_y: min_u8(self.max_y.saturating_add(n), (CHUNK_SIZE - 1) as u8),
        }
    }

    pub const fn touches_border(self) -> bool {
        !self.is_empty()
            && (self.min_x == 0
                || self.min_y == 0
                || self.max_x == (CHUNK_SIZE - 1) as u8
                || self.max_y == (CHUNK_SIZE - 1) as u8)
    }
}

#[derive(Debug, Clone)]
pub struct Chunk {
    cells: Box<[Cell; CHUNK_AREA]>,
    pub bounds: DirtyRect,
    pub old_bounds: DirtyRect,
    pub keep_bounds: DirtyRect,
    pub old_keep_bounds: DirtyRect,
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
            bounds: DirtyRect::EMPTY,
            old_bounds: DirtyRect::EMPTY,
            keep_bounds: DirtyRect::EMPTY,
            old_keep_bounds: DirtyRect::EMPTY,
            sleeping: true,
        }
    }

    pub fn filled(cell: Cell) -> Self {
        Self {
            cells: Box::new([cell; CHUNK_AREA]),
            bounds: DirtyRect::FULL,
            old_bounds: DirtyRect::EMPTY,
            keep_bounds: DirtyRect::EMPTY,
            old_keep_bounds: DirtyRect::EMPTY,
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
        self.bounds.mark(offset);
    }

    pub fn cells(&self) -> &[Cell; CHUNK_AREA] {
        &self.cells
    }

    pub fn cells_mut(&mut self) -> &mut [Cell; CHUNK_AREA] {
        &mut self.cells
    }

    pub fn swap_bounds(&mut self) {
        self.old_bounds = self.bounds;
        self.bounds = DirtyRect::EMPTY;
        self.old_keep_bounds = self.keep_bounds;
        self.keep_bounds = DirtyRect::EMPTY;
    }

    pub fn dirty(&self) -> DirtyRect {
        self.bounds.union(self.old_bounds)
    }

    pub fn keep_dirty(&self) -> DirtyRect {
        self.keep_bounds.union(self.old_keep_bounds)
    }

    pub fn sim_dirty(&self) -> DirtyRect {
        self.dirty().union(self.keep_dirty())
    }

    pub fn normalize_updated(&mut self, tick: u8) {
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
