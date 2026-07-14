use bevy::log::error;
use fallingsand_core::{CHUNK_AREA, CHUNK_SIZE, Cell, CellOffset, CellPos, ChunkPos, DirtyRect};
use fallingsand_protocol::{ChunkOp, TickFrame, cells_from_wire};
use std::collections::HashMap;

pub struct ViewChunk {
    pub cells: Box<[Cell; CHUNK_AREA]>,
}

pub enum ChunkChange {
    Loaded(ChunkPos),
    Delta(ChunkPos, DirtyRect),
    Unloaded(ChunkPos),
    Cleared,
}

#[derive(Default)]
pub struct WorldView {
    pub chunks: HashMap<ChunkPos, ViewChunk>,
    pub server_tick: u64,
    changes: Vec<ChunkChange>,
}

impl WorldView {
    pub fn get_cell(&self, pos: CellPos) -> Option<Cell> {
        self.chunks
            .get(&pos.chunk())
            .map(|chunk| chunk.cells[pos.offset().index()])
    }

    pub fn clear(&mut self) {
        self.chunks.clear();
        self.server_tick = 0;
        self.changes.push(ChunkChange::Cleared);
    }

    pub fn take_changes(&mut self) -> Vec<ChunkChange> {
        std::mem::take(&mut self.changes)
    }

    pub fn apply(&mut self, tick: &TickFrame) {
        self.server_tick = self.server_tick.max(tick.tick);
        for op in &tick.chunks {
            match op {
                ChunkOp::Load { pos, cells } => match cells_from_wire(cells, CHUNK_AREA) {
                    Ok(decoded) => {
                        let mut buffer = Box::new([Cell::AIR; CHUNK_AREA]);
                        buffer.copy_from_slice(&decoded);
                        self.chunks.insert(*pos, ViewChunk { cells: buffer });
                        self.changes.push(ChunkChange::Loaded(*pos));
                    }
                    Err(_) => error!("bad chunk load payload for {pos:?}"),
                },
                ChunkOp::Unload { pos } => {
                    if self.chunks.remove(pos).is_some() {
                        self.changes.push(ChunkChange::Unloaded(*pos));
                    }
                }
                ChunkOp::Delta { pos, rect, cells } => {
                    if rect.is_empty() {
                        continue;
                    }
                    if rect.max_x as usize >= CHUNK_SIZE || rect.max_y as usize >= CHUNK_SIZE {
                        error!("delta rect out of bounds for {pos:?}");
                        continue;
                    }
                    let count = (rect.width() * rect.height()) as usize;
                    let Ok(decoded) = cells_from_wire(cells, count) else {
                        error!("bad delta payload for {pos:?}");
                        continue;
                    };
                    let Some(chunk) = self.chunks.get_mut(pos) else {
                        continue;
                    };
                    apply_rect(chunk, *rect, &decoded);
                    self.changes.push(ChunkChange::Delta(*pos, *rect));
                }
            }
        }
    }
}

fn apply_rect(chunk: &mut ViewChunk, rect: DirtyRect, cells: &[Cell]) {
    let width = rect.width() as usize;
    for (row, y) in (rect.min_y..=rect.max_y).enumerate() {
        let src = &cells[row * width..(row + 1) * width];
        let base = CellOffset::new(rect.min_x, y).index();
        chunk.cells[base..base + width].copy_from_slice(src);
    }
}
