use crate::bodies::PixelBody;
use crate::physics::CellSource;
use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, MaterialRegistry, Phase};
use rustc_hash::{FxHashMap, FxHashSet};

const SKIN: f32 = 1e-4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntityBox {
    pub x: f32,
    pub y: f32,
    pub half_w: f32,
    pub half_h: f32,
}

impl EntityBox {
    pub fn contains_cell(&self, pos: CellPos) -> bool {
        let (cx, cy) = (pos.x as f32 + 0.5, pos.y as f32 + 0.5);
        (cx - self.x).abs() <= self.half_w && (cy - self.y).abs() <= self.half_h
    }
}

#[derive(Default)]
pub struct Obstacles {
    pub entity_boxes: Vec<EntityBox>,
    entity_cells: FxHashSet<CellPos>,
    body_cells: FxHashMap<CellPos, Cell>,
}

impl Obstacles {
    pub fn occupied(&self, pos: CellPos) -> bool {
        self.entity_cells.contains(&pos) || self.body_cells.contains_key(&pos)
    }

    pub fn body_cell(&self, pos: CellPos) -> Option<Cell> {
        self.body_cells.get(&pos).copied()
    }

    pub fn overlay<'a, W: CellSource>(&'a self, base: &'a W) -> ObstacleOverlay<'a, W> {
        ObstacleOverlay {
            base,
            obstacles: self,
        }
    }

    pub fn rebuild(
        &mut self,
        world: &mut CellWorld,
        registry: &MaterialRegistry,
        entities: &[EntityBox],
        bodies: &[PixelBody],
    ) {
        let mut entity_cells = FxHashSet::default();
        for entity in entities {
            let x0 = (entity.x - entity.half_w + SKIN).floor() as i32;
            let x1 = (entity.x + entity.half_w - SKIN).floor() as i32;
            let y0 = (entity.y - entity.half_h + SKIN).floor() as i32;
            let y1 = (entity.y + entity.half_h - SKIN).floor() as i32;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    entity_cells.insert(CellPos::new(x, y));
                }
            }
        }

        let mut body_cells = FxHashMap::default();
        for body in bodies {
            for ly in 0..body.height {
                for lx in 0..body.width {
                    let cell = body.cell_at(lx, ly);
                    if cell.is_air() {
                        continue;
                    }
                    let (wx, wy) = body.local_to_world(lx as f32 + 0.5, ly as f32 + 0.5);
                    body_cells.insert(CellPos::new(wx.floor() as i32, wy.floor() as i32), cell);
                }
            }
        }

        for &pos in self.entity_cells.iter() {
            if !entity_cells.contains(&pos) {
                wake_vacated(world, registry, pos);
            }
        }
        for &pos in self.body_cells.keys() {
            if !body_cells.contains_key(&pos) {
                wake_vacated(world, registry, pos);
            }
        }

        self.entity_boxes = entities.to_vec();
        self.entity_cells = entity_cells;
        self.body_cells = body_cells;
    }
}

fn wake_vacated(world: &mut CellWorld, registry: &MaterialRegistry, pos: CellPos) {
    let powder = powder_at(world, registry, pos)
        || (-1..=1).any(|dx| powder_at(world, registry, pos.translated(dx, 1)));
    if powder {
        world.mark_keep(pos);
    }
}

fn powder_at(world: &CellWorld, registry: &MaterialRegistry, pos: CellPos) -> bool {
    world
        .get_cell(pos)
        .is_some_and(|cell| registry.get(cell.material).phase == Phase::Powder)
}

pub struct ObstacleOverlay<'a, W> {
    base: &'a W,
    obstacles: &'a Obstacles,
}

impl<W: CellSource> CellSource for ObstacleOverlay<'_, W> {
    fn cell_at(&self, pos: CellPos) -> Option<Cell> {
        self.obstacles
            .body_cell(pos)
            .or_else(|| self.base.cell_at(pos))
    }
}
