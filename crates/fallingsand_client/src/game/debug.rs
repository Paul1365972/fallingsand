use fallingsand_core::{CARDINAL_NEIGHBORS, CellPos, ChunkPos, DirtyRect};
use fallingsand_protocol::TickFrame;
use std::collections::HashSet;

pub struct RectFlash {
    pub pos: ChunkPos,
    pub rect: DirtyRect,
    pub is_sim: bool,
}

pub struct BodyEdge {
    pub a: CellPos,
    pub b: CellPos,
}

#[derive(Default)]
pub struct DebugState {
    pub subscribed: bool,
    pub rects: Vec<RectFlash>,
    pub body_edges: Vec<BodyEdge>,
}

impl DebugState {
    pub(super) fn update(&mut self, tick: &TickFrame, enabled: bool) {
        self.rects.clear();
        self.body_edges.clear();
        if !enabled {
            return;
        }
        for entry in &tick.debug_rects {
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
        for body in &tick.debug_bodies {
            let cells: HashSet<_> = body.cells.iter().copied().collect();
            for &cell in &body.cells {
                for (dx, dy) in CARDINAL_NEIGHBORS {
                    if cells.contains(&cell.translated(dx, dy)) {
                        continue;
                    }
                    let (a, b) = match (dx, dy) {
                        (0, -1) => (cell, cell.translated(1, 0)),
                        (-1, 0) => (cell, cell.translated(0, 1)),
                        (1, 0) => (cell.translated(1, 0), cell.translated(1, 1)),
                        (0, 1) => (cell.translated(0, 1), cell.translated(1, 1)),
                        _ => unreachable!(),
                    };
                    self.body_edges.push(BodyEdge { a, b });
                }
            }
        }
    }
}
