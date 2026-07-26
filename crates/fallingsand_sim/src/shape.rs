use crate::world::CellWorld;
use fallingsand_core::content;
use fallingsand_core::{Cell, CellPos, Phase, Subcell};
use rustc_hash::FxHashSet;

pub trait CellSource {
    fn cell_at(&self, pos: CellPos) -> Option<Cell>;
}

impl CellSource for CellWorld {
    fn cell_at(&self, pos: CellPos) -> Option<Cell> {
        self.get_cell(pos)
    }
}

pub type OwnCells<'a> = Option<&'a FxHashSet<CellPos>>;

fn own_covers(own: OwnCells, pos: CellPos) -> bool {
    own.is_some_and(|set| set.contains(&pos))
}

pub(crate) fn cell_blocks<W: CellSource>(world: &W, pos: CellPos) -> bool {
    match world.cell_at(pos) {
        Some(cell) => matches!(content::phase(cell.material), Phase::Solid | Phase::Powder),
        None => true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    PosX,
    NegX,
    PosY,
    NegY,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::PosX, Dir::NegX, Dir::PosY, Dir::NegY];

    pub const fn offset(self) -> (i32, i32) {
        match self {
            Dir::PosX => (1, 0),
            Dir::NegX => (-1, 0),
            Dir::PosY => (0, 1),
            Dir::NegY => (0, -1),
        }
    }

    const fn index(self) -> usize {
        match self {
            Dir::PosX => 0,
            Dir::NegX => 1,
            Dir::PosY => 2,
            Dir::NegY => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    w: i32,
    h: i32,
    cells: Vec<(i32, i32)>,
    frontiers: [Vec<(i32, i32)>; 4],
}

impl Shape {
    pub fn rect(w: i32, h: i32) -> Self {
        let w = w.max(1);
        let h = h.max(1);
        let mut cells = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                cells.push((x, y));
            }
        }
        Self::from_offsets(cells)
    }

    fn from_offsets(mut cells: Vec<(i32, i32)>) -> Self {
        cells.sort_unstable_by_key(|&(x, y)| (y, x));
        cells.dedup();
        let w = cells.iter().map(|&(x, _)| x).max().map_or(1, |x| x + 1);
        let h = cells.iter().map(|&(_, y)| y).max().map_or(1, |y| y + 1);
        let occupied: FxHashSet<(i32, i32)> = cells.iter().copied().collect();
        let frontiers = Dir::ALL.map(|dir| {
            let (dx, dy) = dir.offset();
            cells
                .iter()
                .copied()
                .filter(|&(x, y)| !occupied.contains(&(x + dx, y + dy)))
                .collect()
        });
        Self {
            w,
            h,
            cells,
            frontiers,
        }
    }

    pub fn w(&self) -> i32 {
        self.w
    }

    pub fn h(&self) -> i32 {
        self.h
    }

    pub fn origin(&self, cx: Subcell, cy: Subcell) -> (i32, i32) {
        (cx.floor_cell() - self.w / 2, cy.floor_cell() - self.h / 2)
    }

    pub fn footprint(&self, cx: Subcell, cy: Subcell) -> Footprint {
        let (x0, y0) = self.origin(cx, cy);
        Footprint {
            x0,
            y0,
            x1: x0 + self.w - 1,
            y1: y0 + self.h - 1,
        }
    }

    pub fn frontier(&self, dir: Dir) -> &[(i32, i32)] {
        &self.frontiers[dir.index()]
    }

    pub fn bottom(&self) -> &[(i32, i32)] {
        self.frontier(Dir::NegY)
    }

    fn covers_offset(&self, offset: (i32, i32)) -> bool {
        self.cells
            .binary_search_by_key(&(offset.1, offset.0), |&(x, y)| (y, x))
            .is_ok()
    }

    pub fn step_blockage<W: CellSource>(
        &self,
        world: &W,
        own: OwnCells,
        next_origin: (i32, i32),
        dir: Dir,
    ) -> Blockage {
        let mut blockage = Blockage::default();
        for &(dx, dy) in self.frontier(dir) {
            let pos = CellPos::new(next_origin.0 + dx, next_origin.1 + dy);
            if own_covers(own, pos) {
                continue;
            }
            blockage.probe(world, pos);
        }
        blockage
    }

    pub fn blockage_at<W: CellSource>(
        &self,
        world: &W,
        own: OwnCells,
        origin: (i32, i32),
        candidate: (i32, i32),
    ) -> Blockage {
        let mut blockage = Blockage::default();
        for &(dx, dy) in &self.cells {
            let pos = CellPos::new(candidate.0 + dx, candidate.1 + dy);
            if self.covers_offset((pos.x - origin.0, pos.y - origin.1)) || own_covers(own, pos) {
                continue;
            }
            blockage.probe(world, pos);
        }
        blockage
    }

    pub fn blocked_at<W: CellSource>(
        &self,
        world: &W,
        own: OwnCells,
        origin: (i32, i32),
        candidate: (i32, i32),
    ) -> bool {
        self.cells.iter().any(|&(dx, dy)| {
            let pos = CellPos::new(candidate.0 + dx, candidate.1 + dy);
            !self.covers_offset((pos.x - origin.0, pos.y - origin.1))
                && !own_covers(own, pos)
                && cell_blocks(world, pos)
        })
    }

    pub fn supported_at<W: CellSource>(
        &self,
        world: &W,
        own: OwnCells,
        origin: (i32, i32),
    ) -> bool {
        self.bottom().iter().any(|&(dx, dy)| {
            let pos = CellPos::new(origin.0 + dx, origin.1 + dy - 1);
            !own_covers(own, pos) && cell_blocks(world, pos)
        })
    }

    pub fn support_grip<W: CellSource>(
        &self,
        world: &W,
        own: OwnCells,
        origin: (i32, i32),
    ) -> Option<f32> {
        let mut grip: Option<f32> = None;
        for &(dx, dy) in self.bottom() {
            let pos = CellPos::new(origin.0 + dx, origin.1 + dy - 1);
            if own_covers(own, pos) {
                continue;
            }
            if let Some(cell) = world.cell_at(pos)
                && matches!(content::phase(cell.material), Phase::Solid | Phase::Powder)
            {
                let found = content::material(cell.material).entity_grip;
                grip = Some(grip.map_or(found, |best| best.max(found)));
            }
        }
        grip
    }
}

#[derive(Debug, Default)]
pub struct Blockage {
    pub(crate) solid: bool,
    pub(crate) unloaded: bool,
    pub(crate) solids: Vec<CellPos>,
}

impl Blockage {
    fn probe<W: CellSource>(&mut self, world: &W, pos: CellPos) {
        match world.cell_at(pos) {
            None => {
                self.solid = true;
                self.unloaded = true;
            }
            Some(cell) if matches!(content::phase(cell.material), Phase::Solid | Phase::Powder) => {
                self.solid = true;
                self.solids.push(pos);
            }
            Some(_) => {}
        }
    }

    pub fn free(&self) -> bool {
        !self.solid
    }

    pub(crate) fn single_head_hit(&self, head_row: i32) -> Option<CellPos> {
        if self.unloaded {
            return None;
        }
        match self.solids.as_slice() {
            [pos] if pos.y == head_row => Some(*pos),
            _ => None,
        }
    }

    pub(crate) fn step_top(&self) -> Option<i32> {
        self.solids.iter().map(|pos| pos.y).max()
    }

    pub(crate) fn near_col(&self, dir: i32) -> Option<i32> {
        let cols = self.solids.iter().map(|pos| pos.x);
        if dir > 0 { cols.min() } else { cols.max() }
    }

    pub(crate) fn near_row(&self, dir: i32) -> Option<i32> {
        let rows = self.solids.iter().map(|pos| pos.y);
        if dir > 0 { rows.min() } else { rows.max() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Footprint {
    pub fn contains(&self, pos: CellPos) -> bool {
        pos.x >= self.x0 && pos.x <= self.x1 && pos.y >= self.y0 && pos.y <= self.y1
    }
}
