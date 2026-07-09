use crate::world::CellWorld;
use fallingsand_core::{CellPos, Fixed};
use rustc_hash::FxHashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActorAabb {
    pub x: Fixed,
    pub y: Fixed,
    pub half_w: Fixed,
    pub half_h: Fixed,
}

impl ActorAabb {
    pub fn contains_cell(&self, pos: CellPos) -> bool {
        let (cx, cy) = (Fixed::cell_center(pos.x), Fixed::cell_center(pos.y));
        (cx - self.x).abs() <= self.half_w && (cy - self.y).abs() <= self.half_h
    }
}

#[derive(Default)]
pub struct Obstacles {
    pub entity_boxes: Vec<ActorAabb>,
    entity_cells: FxHashSet<CellPos>,
}

impl Obstacles {
    pub fn occupied(&self, pos: CellPos) -> bool {
        self.entity_cells.contains(&pos)
    }

    pub fn rebuild(&mut self, world: &mut CellWorld, entities: &[ActorAabb]) {
        let mut entity_cells = FxHashSet::default();
        for entity in entities {
            let x0 = (entity.x - entity.half_w).floor_cell();
            let x1 = (entity.x + entity.half_w).max_cell();
            let y0 = (entity.y - entity.half_h).floor_cell();
            let y1 = (entity.y + entity.half_h).max_cell();
            for y in y0..=y1 {
                for x in x0..=x1 {
                    entity_cells.insert(CellPos::new(x, y));
                }
            }
        }

        for &pos in self.entity_cells.iter() {
            if !entity_cells.contains(&pos) {
                wake_around(world, pos);
            }
        }
        for &pos in entity_cells.iter() {
            if !self.entity_cells.contains(&pos) {
                wake_around(world, pos);
            }
        }

        self.entity_boxes = entities.to_vec();
        self.entity_cells = entity_cells;
    }
}

fn wake_around(world: &mut CellWorld, pos: CellPos) {
    for dy in -1..=1 {
        for dx in -1..=1 {
            world.mark_keep(pos.translated(dx, dy));
        }
    }
}
