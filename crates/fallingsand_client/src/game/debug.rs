use fallingsand_core::{CellPos, ChunkPos, DirtyRect};
use fallingsand_protocol::TickFrame;

pub struct RectFlash {
    pub pos: ChunkPos,
    pub rect: DirtyRect,
    pub is_sim: bool,
}

#[derive(Default)]
pub struct DebugState {
    pub subscribed: bool,
    pub rects: Vec<RectFlash>,
    pub body_cells: Vec<(u32, CellPos)>,
}

impl DebugState {
    pub(super) fn track_rects(&mut self, tick: &TickFrame, borders: bool) {
        if !borders {
            if !self.rects.is_empty() {
                self.rects.clear();
            }
            if !self.body_cells.is_empty() {
                self.body_cells.clear();
            }
            return;
        }
        self.rects.clear();
        self.body_cells.clear();
        for entry in &tick.debug {
            let base = entry.pos.base_cell();
            self.body_cells.extend(entry.bodies.iter().map(|cell| {
                (
                    cell.body,
                    base.translated(cell.offset.x.into(), cell.offset.y.into()),
                )
            }));
            for (rect, is_sim) in [(entry.change, false), (entry.sim, true)] {
                if rect.is_empty() || (is_sim && entry.sim == entry.change) {
                    continue;
                }
                self.rects.push(RectFlash {
                    pos: entry.pos,
                    rect,
                    is_sim,
                });
            }
        }
    }
}
