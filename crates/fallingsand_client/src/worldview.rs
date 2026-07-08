use crate::net::{SessionEnded, TickMessage};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use fallingsand_core::{CHUNK_AREA, CHUNK_SIZE, Cell, CellOffset, CellPos, ChunkPos, DirtyRect};
use fallingsand_protocol::{ChunkOp, cells_from_wire};

pub struct WorldViewPlugin;

pub struct ViewChunk {
    pub cells: Box<[Cell; CHUNK_AREA]>,
    pub dirty: bool,
    pub pending: Vec<DirtyRect>,
}

#[derive(Resource, Default)]
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
}

impl Plugin for WorldViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldView>()
            .add_systems(PreUpdate, apply_updates.after(crate::net::NetSet))
            .add_systems(Update, clear_view.run_if(on_message::<SessionEnded>))
            .add_systems(OnExit(crate::AppState::InGame), clear_view);
    }
}

fn clear_view(mut view: ResMut<WorldView>) {
    view.chunks.clear();
    view.server_tick = 0;
}

fn apply_updates(mut view: ResMut<WorldView>, mut frames: MessageReader<TickMessage>) {
    for TickMessage(tick) in frames.read() {
        view.server_tick = view.server_tick.max(tick.tick);
        for op in &tick.chunks {
            match op {
                ChunkOp::Load { pos, cells } => match cells_from_wire(cells) {
                    Ok(decoded) if decoded.len() == CHUNK_AREA => {
                        let mut buffer = Box::new([Cell::AIR; CHUNK_AREA]);
                        buffer.copy_from_slice(&decoded);
                        view.chunks.insert(
                            *pos,
                            ViewChunk {
                                cells: buffer,
                                dirty: true,
                                pending: Vec::new(),
                            },
                        );
                    }
                    _ => error!("bad chunk load payload for {pos:?}"),
                },
                ChunkOp::Unload { pos } => {
                    view.chunks.remove(pos);
                }
                ChunkOp::Delta { pos, rect, cells } => {
                    let Some(chunk) = view.chunks.get_mut(pos) else {
                        continue;
                    };
                    if rect.is_empty() {
                        continue;
                    }
                    if rect.max_x as usize >= CHUNK_SIZE || rect.max_y as usize >= CHUNK_SIZE {
                        error!("delta rect out of bounds for {pos:?}");
                        continue;
                    }
                    let Ok(decoded) = cells_from_wire(cells) else {
                        error!("bad delta payload for {pos:?}");
                        continue;
                    };
                    if decoded.len() != (rect.width() * rect.height()) as usize {
                        error!("delta size mismatch for {pos:?}");
                        continue;
                    }
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
