use crate::player::{PLAYER_MASS_UNITS, Players};
use crate::regions::ChunkTickets;
use fallingsand_core::{CellPos, RegionPos, Subcell};
use fallingsand_sim::CellWorld;
use fallingsand_sim::bodies::{Ambient, BodySet, Shove, Stander, detect_detached_island};

pub const BODY_GRAVITY: Subcell = Subcell::from_cells_per_second_squared(-400);

#[derive(Default)]
pub struct BodyWorld {
    set: BodySet,
    pending_checks: Vec<CellPos>,
    checks: Vec<CellPos>,
}

impl BodyWorld {
    pub fn push(&mut self, pos: CellPos, dvx: Subcell, dvy: Subcell) -> bool {
        self.set.push(pos, dvx, dvy, PLAYER_MASS_UNITS)
    }

    pub fn settle_overlapping_regions(&mut self, sim: &mut CellWorld, regions: &[RegionPos]) {
        self.set.settle_regions(sim, regions);
    }

    pub fn debug_rasters(&self) -> impl Iterator<Item = Vec<CellPos>> + '_ {
        self.set.rasters().map(Iterator::collect)
    }

    pub fn drain_shoves(&mut self) -> impl Iterator<Item = Shove> + '_ {
        self.set.drain_shoves()
    }

    pub fn step(
        &mut self,
        sim: &mut CellWorld,
        tickets: &ChunkTickets,
        players: &Players,
    ) -> BodyStepMetrics {
        self.set.reconcile(sim);
        self.pending_checks.extend(sim.drain_unseated());
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
        self.set.step(
            sim,
            &Ambient {
                gravity: BODY_GRAVITY,
                simulated: &|chunk| tickets.simulates(chunk),
                stander: &|pos| stander_at(players, pos),
            },
        );
        BodyStepMetrics {
            bodies: self.set.body_count(),
        }
    }
}

pub struct BodyStepMetrics {
    pub bodies: usize,
}

fn stander_at(players: &Players, pos: CellPos) -> Option<Stander> {
    players.iter().find_map(|(_, player)| {
        player
            .avatar()
            .filter(|avatar| avatar.stamp.covers(pos))
            .map(|avatar| Stander {
                mass: PLAYER_MASS_UNITS,
                vx: avatar.actor.vx,
                vy: avatar.actor.vy,
            })
    })
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
