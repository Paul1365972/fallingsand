use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, Phase, content};
use rustc_hash::FxHashSet;
use std::collections::VecDeque;

const MAX_BODY_EXTENT: i32 = 48;
const MAX_ISLAND_CELLS: usize = 2048;

fn rigid_terrain(world: &CellWorld, pos: CellPos) -> Option<Cell> {
    let cell = world.get_cell(pos)?;
    (!cell.is_body()
        && content::phase(cell.material) == Phase::Solid
        && content::is_rigid_capable(cell.material))
    .then_some(cell)
}

pub fn detect_detached_island(world: &CellWorld, seed: CellPos) -> Option<Vec<CellPos>> {
    let seed_cell = rigid_terrain(world, seed)?;
    let mut visited = FxHashSet::default();
    let mut queue = VecDeque::new();
    visited.insert(seed);
    queue.push_back((seed, seed_cell.material));
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (seed.x, seed.x, seed.y, seed.y);

    while let Some((pos, material)) = queue.pop_front() {
        if supported(world, pos, material) {
            return None;
        }
        for (dx, dy) in fallingsand_core::CARDINAL_NEIGHBORS {
            let next = pos.translated(dx, dy);
            if visited.contains(&next) {
                continue;
            }
            let Some(cell) = rigid_terrain(world, next) else {
                continue;
            };
            if !content::bonds(material, cell.material) {
                continue;
            }
            min_x = min_x.min(next.x);
            max_x = max_x.max(next.x);
            min_y = min_y.min(next.y);
            max_y = max_y.max(next.y);
            if max_x - min_x >= MAX_BODY_EXTENT
                || max_y - min_y >= MAX_BODY_EXTENT
                || visited.len() >= MAX_ISLAND_CELLS
            {
                return None;
            }
            visited.insert(next);
            queue.push_back((next, cell.material));
        }
    }

    let mut island: Vec<_> = visited.into_iter().collect();
    island.sort_unstable_by_key(|pos| (pos.y, pos.x));
    Some(island)
}

fn supported(world: &CellWorld, pos: CellPos, material: fallingsand_core::MaterialId) -> bool {
    let below = pos.translated(0, -1);
    if rigid_terrain(world, below).is_some_and(|cell| content::bonds(material, cell.material)) {
        return false;
    }
    world.get_cell(below).is_some_and(|cell| {
        cell.is_body() || matches!(content::phase(cell.material), Phase::Solid | Phase::Powder)
    })
}
