use crate::world::CellWorld;
use fallingsand_core::{CellPos, Fixed, MaterialRegistry, Phase};
use rustc_hash::FxHashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityBox {
    pub x: Fixed,
    pub y: Fixed,
    pub half_w: Fixed,
    pub half_h: Fixed,
}

impl EntityBox {
    pub fn contains_cell(&self, pos: CellPos) -> bool {
        let (cx, cy) = (Fixed::cell_center(pos.x), Fixed::cell_center(pos.y));
        (cx - self.x).abs() <= self.half_w && (cy - self.y).abs() <= self.half_h
    }
}

#[derive(Default)]
pub struct Obstacles {
    pub entity_boxes: Vec<EntityBox>,
    entity_cells: FxHashSet<CellPos>,
}

impl Obstacles {
    pub fn occupied(&self, pos: CellPos) -> bool {
        self.entity_cells.contains(&pos)
    }

    pub fn rebuild(
        &mut self,
        world: &mut CellWorld,
        registry: &MaterialRegistry,
        entities: &[EntityBox],
    ) {
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
                wake_vacated(world, registry, pos);
            }
        }

        self.entity_boxes = entities.to_vec();
        self.entity_cells = entity_cells;
    }
}

fn wake_vacated(world: &mut CellWorld, registry: &MaterialRegistry, pos: CellPos) {
    let powder = powder_at(world, registry, pos)
        || (-1..=1).any(|dx| powder_at(world, registry, pos.translated(dx, 1)));
    let fluid = fluid_at(world, registry, pos)
        || [(0, 1), (0, -1), (1, 0), (-1, 0)]
            .iter()
            .any(|&(dx, dy)| fluid_at(world, registry, pos.translated(dx, dy)));
    if powder || fluid {
        world.mark_keep(pos);
    }
}

fn powder_at(world: &CellWorld, registry: &MaterialRegistry, pos: CellPos) -> bool {
    world
        .get_cell(pos)
        .is_some_and(|cell| registry.get(cell.material).phase == Phase::Powder)
}

fn fluid_at(world: &CellWorld, registry: &MaterialRegistry, pos: CellPos) -> bool {
    world.get_cell(pos).is_some_and(|cell| {
        matches!(
            registry.get(cell.material).phase,
            Phase::Liquid | Phase::Gas | Phase::Fire
        )
    })
}
