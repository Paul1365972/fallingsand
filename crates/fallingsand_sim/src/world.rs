use fallingsand_core::{CHUNK_SIZE, Cell, CellPos, Chunk, ChunkPos, MaterialId, Phase, content};
use fallingsand_math::Hash;
use rustc_hash::FxHashMap;

const CELL_SHADE_SALT: Hash = Hash::label("simulation.cell_shade");

pub(crate) fn structural(cell: Cell) -> bool {
    cell.is_body() || matches!(content::phase(cell.material), Phase::Solid | Phase::Powder)
}

#[derive(Default)]
pub struct CellWorld {
    chunks: FxHashMap<ChunkPos, Chunk>,
    structural: Vec<CellPos>,
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
        let Some(chunk) = self.chunks.get_mut(&pos.chunk()) else {
            return;
        };
        let old = chunk.get(pos.offset());
        cell.flags &= Cell::BODY;
        chunk.set(pos.offset(), cell);
        if notify && old != cell && (structural(old) || structural(cell)) {
            self.structural.push(pos);
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

    pub(crate) fn push_structural(&mut self, positions: impl IntoIterator<Item = CellPos>) {
        self.structural.extend(positions);
    }

    pub fn drain_structural(&mut self) -> impl Iterator<Item = CellPos> + '_ {
        self.structural.drain(..)
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
