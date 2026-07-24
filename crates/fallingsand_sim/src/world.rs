use fallingsand_core::{CHUNK_SIZE, Cell, CellPos, Chunk, ChunkPos, MaterialId, Phase, content};
use fallingsand_math::Hash;
use rustc_hash::FxHashMap;

const CELL_SHADE_SALT: Hash = Hash::label("simulation.cell_shade");

pub(crate) fn obstructs(material: MaterialId) -> bool {
    matches!(content::phase(material), Phase::Solid | Phase::Powder)
}

pub(crate) fn blocking(cell: Cell) -> bool {
    cell.is_body() || obstructs(cell.material)
}

pub(crate) fn mobile(material: MaterialId) -> bool {
    !matches!(content::phase(material), Phase::Empty | Phase::Solid)
}

pub(crate) fn rigid_seed(cell: Cell) -> bool {
    !cell.is_body()
        && content::phase(cell.material) == Phase::Solid
        && content::is_rigid_capable(cell.material)
}

fn support_class(cell: Cell) -> u8 {
    if !obstructs(cell.material) {
        return 0;
    }
    match content::bond_group(cell.material) {
        u8::MAX => 1,
        group => 2 + group,
    }
}

pub(crate) enum Unseated {
    Nothing,
    Written,
    Neighbours,
}

pub(crate) fn unseated(old: Cell, new: Cell) -> Unseated {
    let before = support_class(old);
    if before == support_class(new) {
        Unseated::Nothing
    } else if before != 0 {
        Unseated::Neighbours
    } else {
        Unseated::Written
    }
}

#[derive(Default)]
pub struct CellWorld {
    chunks: FxHashMap<ChunkPos, Chunk>,
    unseated: Vec<CellPos>,
    tick: u64,
}

impl CellWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub const fn tick(&self) -> u64 {
        self.tick
    }

    pub fn set_tick(&mut self, tick: u64) {
        self.tick = tick;
    }

    pub(crate) fn advance_tick(&mut self) {
        self.tick += 1;
    }

    pub fn insert_chunk(&mut self, pos: ChunkPos, chunk: Chunk) {
        self.chunks.insert(pos, chunk);
    }

    pub fn remove_chunk(&mut self, pos: ChunkPos) -> Option<Chunk> {
        self.chunks.remove(&pos)
    }

    pub fn chunk(&self, pos: ChunkPos) -> Option<&Chunk> {
        self.chunks.get(&pos)
    }

    pub fn chunks(&self) -> impl Iterator<Item = (ChunkPos, &Chunk)> {
        self.chunks.iter().map(|(&pos, chunk)| (pos, chunk))
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub(crate) fn chunk_map_mut(&mut self) -> &mut FxHashMap<ChunkPos, Chunk> {
        &mut self.chunks
    }

    pub fn get_cell(&self, pos: CellPos) -> Option<Cell> {
        self.chunks.get(&pos.chunk()).map(|c| c.get(pos.offset()))
    }

    pub(crate) fn set(&mut self, pos: CellPos, mut cell: Cell, notify: bool) {
        cell.flags &= Cell::BODY;
        let Some(chunk) = self.chunks.get_mut(&pos.chunk()) else {
            return;
        };
        let old = chunk.get(pos.offset());
        chunk.set(pos.offset(), cell);
        self.mark_sim_border(pos);
        if !notify {
            return;
        }
        match unseated(old, cell) {
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
        if self.get_cell(pos).is_some_and(rigid_seed) {
            self.unseated.push(pos);
        }
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
                if let Some(chunk) = self.chunks.get_mut(&n.chunk()) {
                    chunk.sim.mark(n.offset());
                }
            }
        }
    }

    pub fn set_material(&mut self, pos: CellPos, material: MaterialId, notify: bool) -> bool {
        let Some(old) = self.get_cell(pos) else {
            return false;
        };
        if material != MaterialId::AIR && !old.is_air() {
            return false;
        }
        let cell = if material == MaterialId::AIR {
            Cell::AIR
        } else {
            let shade = Hash::seed(self.tick)
                .salt(CELL_SHADE_SALT)
                .pos(pos.x, pos.y)
                .bits(4) as u8;
            Cell::new(material, shade)
        };
        self.set(pos, cell, notify);
        true
    }

    pub(crate) fn push_unseated(&mut self, positions: impl IntoIterator<Item = CellPos>) {
        self.unseated.extend(positions);
    }

    pub fn drain_unseated(&mut self) -> impl Iterator<Item = CellPos> + '_ {
        self.unseated.drain(..)
    }

    pub fn awake_counts(&self) -> (usize, u64) {
        let mut chunks = 0;
        let mut cells = 0;
        for chunk in self.chunks.values() {
            let rect = chunk.sim_rect();
            if rect.is_empty() {
                continue;
            }
            chunks += 1;
            cells += rect.width() as u64 * rect.height() as u64;
        }
        (chunks, cells)
    }
}
