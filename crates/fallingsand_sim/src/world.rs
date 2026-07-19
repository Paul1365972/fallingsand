use fallingsand_core::{CHUNK_SIZE, Cell, CellPos, Chunk, ChunkPos, MaterialId};
use fallingsand_math::Hash;
use rustc_hash::FxHashMap;

const CELL_SHADE_SALT: Hash = Hash::label("simulation.cell_shade");

#[derive(Default)]
pub struct CellWorld {
    chunks: FxHashMap<ChunkPos, Chunk>,
    structural: Vec<CellPos>,
    damage: Vec<CellPos>,
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

    pub(crate) fn set_cell(&mut self, pos: CellPos, cell: Cell) {
        self.set_cell_with_observers(pos, cell, true);
    }

    fn set_cell_with_observers(&mut self, pos: CellPos, mut cell: Cell, notify: bool) {
        let Some(chunk) = self.chunks.get_mut(&pos.chunk()) else {
            return;
        };
        let old = chunk.get(pos.offset());
        cell.flags = 0;
        chunk.set(pos.offset(), cell);
        if old.is_body() {
            self.damage.push(pos);
        }
        self.mark_sim_border(pos);
        if notify {
            self.note_observers(pos);
        }
    }

    pub(crate) fn set_cell_raw(&mut self, pos: CellPos, cell: Cell) {
        self.set_cell_raw_with_observers(pos, cell, true);
    }

    pub(crate) fn set_cell_raw_quiet(&mut self, pos: CellPos, cell: Cell) {
        self.set_cell_raw_with_observers(pos, cell, false);
    }

    fn set_cell_raw_with_observers(&mut self, pos: CellPos, mut cell: Cell, notify: bool) {
        let Some(chunk) = self.chunks.get_mut(&pos.chunk()) else {
            return;
        };
        cell.flags &= Cell::BODY;
        chunk.set(pos.offset(), cell);
        self.mark_sim_border(pos);
        if notify {
            self.note_observers(pos);
        }
    }

    fn note_observers(&mut self, changed: CellPos) {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let pos = changed.translated(dx, dy);
                if self.get_cell(pos).is_some_and(|cell| {
                    cell.is_body() || fallingsand_core::content::is_rigid_capable(cell.material)
                }) {
                    self.structural.push(pos);
                }
            }
        }
    }

    pub(crate) fn note_interaction(&mut self, pos: CellPos) {
        self.note_observers(pos);
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

    fn material_cell(&self, pos: CellPos, material: MaterialId) -> Cell {
        let shade = Hash::seed(self.tick)
            .salt(CELL_SHADE_SALT)
            .pos(pos.x, pos.y)
            .bits(4) as u8;
        Cell::new(material, shade)
    }

    pub fn clear_cell(&mut self, pos: CellPos) {
        self.set_cell(pos, Cell::AIR);
    }

    pub fn fill_material(&mut self, pos: CellPos, material: MaterialId) -> bool {
        self.fill_material_with_observers(pos, material, true)
    }

    pub fn fill_material_quiet(&mut self, pos: CellPos, material: MaterialId) -> bool {
        self.fill_material_with_observers(pos, material, false)
    }

    fn fill_material_with_observers(
        &mut self,
        pos: CellPos,
        material: MaterialId,
        notify: bool,
    ) -> bool {
        if !self.get_cell(pos).is_some_and(|cell| cell.is_air()) {
            return false;
        }
        let cell = self.material_cell(pos, material);
        self.set_cell_with_observers(pos, cell, notify);
        true
    }

    pub(crate) fn push_structural(&mut self, positions: Vec<CellPos>) {
        self.structural.extend(positions);
    }

    pub fn take_structural(&mut self) -> Vec<CellPos> {
        std::mem::take(&mut self.structural)
    }

    pub(crate) fn push_damage(&mut self, positions: Vec<CellPos>) {
        self.damage.extend(positions);
    }

    pub fn take_damage(&mut self) -> Vec<CellPos> {
        std::mem::take(&mut self.damage)
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
