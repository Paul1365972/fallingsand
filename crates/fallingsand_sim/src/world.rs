use crate::edits::WorldEdit;
use fallingsand_core::{Cell, CellPos, Chunk, ChunkPos, MaterialId};
use fallingsand_rng::Hash;
use rustc_hash::FxHashMap;

#[derive(Default)]
pub struct CellWorld {
    chunks: FxHashMap<ChunkPos, Chunk>,
    edits: Vec<WorldEdit>,
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

    pub(crate) fn chunk_map_mut(&mut self) -> &mut FxHashMap<ChunkPos, Chunk> {
        &mut self.chunks
    }

    pub fn get_cell(&self, pos: CellPos) -> Option<Cell> {
        self.chunks.get(&pos.chunk()).map(|c| c.get(pos.offset()))
    }

    pub fn set_cell(&mut self, pos: CellPos, mut cell: Cell) {
        let Some(chunk) = self.chunks.get_mut(&pos.chunk()) else {
            return;
        };
        if chunk.sleeping {
            chunk.normalize_updated(self.tick as u8);
            chunk.sleeping = false;
        }
        let old = chunk.get(pos.offset());
        cell.updated = self.tick as u8;
        chunk.set(pos.offset(), cell);
        if old.is_body() && !cell.is_body() {
            self.damage.push(pos);
        }
    }

    pub(crate) fn set_cell_raw(&mut self, pos: CellPos, mut cell: Cell) {
        let Some(chunk) = self.chunks.get_mut(&pos.chunk()) else {
            return;
        };
        if chunk.sleeping {
            chunk.normalize_updated(self.tick as u8);
            chunk.sleeping = false;
        }
        cell.updated = self.tick as u8;
        chunk.set(pos.offset(), cell);
    }

    pub fn place_material(&mut self, pos: CellPos, material: MaterialId) {
        let shade = Hash::new().pos(pos.x, pos.y).bits(4) as u8;
        self.set_cell(pos, Cell::new(material, shade));
    }

    pub fn mark_keep(&mut self, pos: CellPos) {
        let Some(chunk) = self.chunks.get_mut(&pos.chunk()) else {
            return;
        };
        if chunk.sleeping {
            chunk.normalize_updated(self.tick as u8);
            chunk.sleeping = false;
        }
        chunk.keep_bounds.mark(pos.offset());
    }

    pub fn queue_edit(&mut self, edit: WorldEdit) {
        self.edits.push(edit);
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

    pub(crate) fn apply_edits(&mut self) {
        let edits = std::mem::take(&mut self.edits);
        for edit in edits {
            match edit {
                WorldEdit::SetCell { pos, material } => self.place_material(pos, material),
                WorldEdit::FillRect { min, max, material } => {
                    for y in min.y..=max.y {
                        for x in min.x..=max.x {
                            self.place_material(CellPos::new(x, y), material);
                        }
                    }
                }
            }
        }
    }

    pub fn awake_chunk_count(&self) -> usize {
        self.chunks
            .values()
            .filter(|chunk| !chunk.sim_dirty().is_empty())
            .count()
    }

    pub fn awake_cell_count(&self) -> u64 {
        self.chunks
            .values()
            .filter(|chunk| !chunk.sim_dirty().is_empty())
            .map(|chunk| {
                let rect = chunk.dirty();
                rect.width() as u64 * rect.height() as u64
            })
            .sum()
    }

    pub fn count_material(&self, material: MaterialId) -> usize {
        self.chunks
            .values()
            .flat_map(|chunk| chunk.cells().iter())
            .filter(|cell| cell.material == material)
            .count()
    }
}
