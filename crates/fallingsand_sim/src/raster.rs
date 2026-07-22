use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, Phase, content};
use rustc_hash::FxHashSet;

const RELOCATE_RADIUS: i32 = 8;
const SURFACE_PROBE: i32 = 64;

#[derive(Debug, Default)]
pub(crate) struct Raster {
    pub(crate) cells: Vec<(CellPos, u16)>,
    pub(crate) set: FxHashSet<CellPos>,
}

impl Raster {
    pub(crate) fn covers(&self, pos: CellPos) -> bool {
        self.set.contains(&pos)
    }
}

pub(crate) fn commit_stamp(
    world: &mut CellWorld,
    old: &Raster,
    new: &Raster,
    cell_for: impl Fn(u16) -> Cell,
) -> Option<()> {
    let mut displaced = Vec::new();
    for &(pos, _) in &new.cells {
        if old.covers(pos) {
            continue;
        }
        let cell = world.get_cell(pos)?;
        if cell.is_body() {
            return None;
        }
        match content::phase(cell.material) {
            Phase::Solid | Phase::Powder => return None,
            Phase::Empty => {}
            Phase::Liquid | Phase::Gas => displaced.push((pos, cell)),
        }
    }

    let mut vacated: Vec<_> = old
        .set
        .iter()
        .filter(|pos| !new.set.contains(pos))
        .copied()
        .collect();
    vacated.sort_unstable_by_key(|pos| (pos.y, pos.x));
    displaced.sort_unstable_by_key(|&(pos, cell)| {
        (
            std::cmp::Reverse(content::density_milli(cell.material)),
            pos.y,
            pos.x,
        )
    });

    let mut claimed = FxHashSet::default();
    let mut receptacles = vacated.iter();
    let mut writes = Vec::new();
    let mut spill = Vec::new();
    for &(pos, cell) in &displaced {
        if let Some(&target) = receptacles.next() {
            writes.push((target, cell));
        } else {
            spill.push((pos, cell));
        }
    }
    for &target in receptacles {
        writes.push((target, Cell::AIR));
    }
    for (from, cell) in spill {
        let target = relocation_spot(world, &claimed, &new.set, from)?;
        claimed.insert(target);
        writes.push((target, cell));
    }

    for (pos, cell) in writes {
        if world.get_cell(pos) != Some(cell) {
            world.set_cell_raw(pos, cell);
        }
    }
    for &(pos, local) in &new.cells {
        let cell = cell_for(local);
        if world.get_cell(pos) != Some(cell) {
            if old.covers(pos) {
                world.set_cell_raw_quiet(pos, cell);
            } else {
                world.set_cell_raw(pos, cell);
            }
        }
    }
    Some(())
}

fn relocation_spot(
    world: &CellWorld,
    claimed: &FxHashSet<CellPos>,
    exclude: &FxHashSet<CellPos>,
    from: CellPos,
) -> Option<CellPos> {
    for radius in 1..=RELOCATE_RADIUS {
        let mut ring = Vec::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs().max(dy.abs()) == radius {
                    ring.push((dx, dy));
                }
            }
        }
        ring.sort_by_key(|&(dx, dy)| (-dy, dx.abs(), dx));
        for (dx, dy) in ring {
            let pos = from.translated(dx, dy);
            if claimed.contains(&pos) || exclude.contains(&pos) {
                continue;
            }
            if world
                .get_cell(pos)
                .is_some_and(|cell| content::phase(cell.material) == Phase::Empty)
            {
                return Some(pos);
            }
        }
    }

    let mut pos = from;
    for _ in 0..SURFACE_PROBE {
        pos = pos.translated(0, 1);
        if exclude.contains(&pos) {
            continue;
        }
        match world
            .get_cell(pos)
            .map(|cell| content::phase(cell.material))
        {
            Some(Phase::Empty) if !claimed.contains(&pos) => return Some(pos),
            Some(Phase::Empty | Phase::Liquid | Phase::Gas) => {}
            _ => return None,
        }
    }
    None
}
