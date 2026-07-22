use crate::regions::ChunkTickets;
use fallingsand_core::{CellPos, RegionPos, Subcell};
use fallingsand_sim::CellWorld;
use fallingsand_sim::bodies::{BodySet, detect_detached_island};

pub const BODY_GRAVITY: Subcell = Subcell::from_cells_per_second_squared(-400);

#[derive(Default)]
pub struct BodyWorld {
    set: BodySet,
    pending_checks: Vec<CellPos>,
    checks: Vec<CellPos>,
}

impl BodyWorld {
    pub fn push_at(&mut self, pos: CellPos, dvx: Subcell, dvy: Subcell, source_mass: u32) -> bool {
        self.set.push_at(pos, dvx, dvy, source_mass)
    }

    pub fn settle_overlapping_regions(&mut self, sim: &mut CellWorld, regions: &[RegionPos]) {
        self.set.settle_regions(sim, regions);
    }

    pub fn debug_rasters(&self) -> impl Iterator<Item = Vec<CellPos>> + '_ {
        self.set.rasters().map(Iterator::collect)
    }

    pub fn step(&mut self, sim: &mut CellWorld, tickets: &ChunkTickets) -> BodyStepMetrics {
        self.pending_checks.extend(sim.drain_detachment_checks());
        std::mem::swap(&mut self.pending_checks, &mut self.checks);
        self.checks.sort_unstable_by_key(|pos| (pos.y, pos.x));
        self.checks.dedup();
        for seed in self.checks.drain(..) {
            if sim.get_cell(seed).is_some_and(|cell| cell.is_body()) {
                continue;
            }
            let Some(island) = detect_detached_island(sim, seed) else {
                continue;
            };
            if island_simulated(sim, tickets, &island) {
                self.set.detach(sim, island);
            } else {
                self.pending_checks.push(seed);
            }
        }
        self.set
            .step(sim, BODY_GRAVITY, |chunk| tickets.simulates(chunk));
        BodyStepMetrics {
            bodies: self.set.body_count(),
        }
    }
}

pub struct BodyStepMetrics {
    pub bodies: usize,
}

fn island_simulated(world: &CellWorld, tickets: &ChunkTickets, island: &[CellPos]) -> bool {
    let min_x = island.iter().map(|p| p.x).min().unwrap();
    let max_x = island.iter().map(|p| p.x).max().unwrap();
    let min_y = island.iter().map(|p| p.y).min().unwrap();
    let max_y = island.iter().map(|p| p.y).max().unwrap();
    let min = CellPos::new(min_x - 1, min_y - 1).chunk();
    let max = CellPos::new(max_x + 1, max_y + 1).chunk();
    for y in min.y..=max.y {
        for x in min.x..=max.x {
            let pos = fallingsand_core::ChunkPos::new(x, y);
            if world.chunk(pos).is_none() || !tickets.simulates(pos) {
                return false;
            }
        }
    }
    true
}
