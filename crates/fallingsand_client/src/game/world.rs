use bevy::log::error;
use fallingsand_core::{CHUNK_AREA, CHUNK_SIZE, Cell, CellOffset, CellPos, ChunkPos, DirtyRect};
use fallingsand_protocol::{ChunkOp, TickFrame, cells_from_wire};
use std::collections::HashMap;

pub struct ViewChunk {
    pub cells: Box<[Cell; CHUNK_AREA]>,
    dirty: bool,
    pending: Vec<DirtyRect>,
}

impl ViewChunk {
    pub fn take_full(&mut self) -> bool {
        if self.dirty {
            self.dirty = false;
            self.pending.clear();
            true
        } else {
            false
        }
    }

    pub fn take_pending(&mut self) -> Vec<DirtyRect> {
        std::mem::take(&mut self.pending)
    }

    pub fn mark_full(&mut self) {
        self.dirty = true;
    }
}

#[derive(Default)]
pub struct WorldView {
    pub chunks: HashMap<ChunkPos, ViewChunk>,
    pub server_tick: u64,
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
    }

    pub fn apply(&mut self, tick: &TickFrame) {
        self.server_tick = self.server_tick.max(tick.tick);
        for op in &tick.chunks {
            match op {
                ChunkOp::Load { pos, cells } => match cells_from_wire(cells, CHUNK_AREA) {
                    Ok(decoded) => {
                        let mut buffer = Box::new([Cell::AIR; CHUNK_AREA]);
                        buffer.copy_from_slice(&decoded);
                        self.chunks.insert(
                            *pos,
                            ViewChunk {
                                cells: buffer,
                                dirty: true,
                                pending: Vec::new(),
                            },
                        );
                    }
                    Err(_) => error!("bad chunk load payload for {pos:?}"),
                },
                ChunkOp::Unload { pos } => {
                    self.chunks.remove(pos);
                }
                ChunkOp::Delta { pos, rect, cells } => {
                    let Some(chunk) = self.chunks.get_mut(pos) else {
                        continue;
                    };
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
                    apply_rect(chunk, *rect, &decoded);
                    if !chunk.dirty {
                        chunk.pending.push(*rect);
                    }
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
