use fallingsand_core::{ChunkPos, DirtyRect};
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
}

impl DebugState {
    pub(super) fn update(&mut self, tick: &TickFrame, enabled: bool) {
        self.rects.clear();
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
    }
}
