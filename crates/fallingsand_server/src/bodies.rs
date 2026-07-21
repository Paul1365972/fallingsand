use crate::player::{PLAYER_MASS, Players};
use crate::regions::ChunkTickets;
use fallingsand_core::{CellPos, REGION_SIZE_CELLS, RegionPos, Subcell};
use fallingsand_protocol::PlayerId;
use fallingsand_sim::bodies::{ActorDynamics, BodySet, PixelBody, detect_island};
use fallingsand_sim::{ActorAabb, CellWorld};

pub const BODY_GRAVITY: Subcell = Subcell::from_cells_per_second_squared(-400.0);

#[derive(Default)]
pub struct BodyWorld {
    set: BodySet,
    candidates: Vec<CellPos>,
    candidate_work: Vec<CellPos>,
    damage: Vec<CellPos>,
    actor_players: Vec<PlayerId>,
    actors: Vec<ActorDynamics>,
}

pub struct BodyStepMetrics {
    pub bodies: usize,
}

impl BodyWorld {
    pub fn receive_player_contact(&mut self, pos: CellPos, wake: bool) -> Option<bool> {
        self.set.receive_player_contact(pos, wake)
    }

    pub fn settle_overlapping_regions(&mut self, sim: &mut CellWorld, regions: &[RegionPos]) {
        self.set.settle_quiet_where(sim, |body| {
            regions
                .iter()
                .copied()
                .any(|region| body_overlaps_region(body, region))
        });
    }

    pub fn step(
        &mut self,
        sim: &mut CellWorld,
        tickets: &ChunkTickets,
        players: &mut Players,
    ) -> BodyStepMetrics {
        self.damage.extend(sim.drain_damage());
        self.set.apply_damage(sim, &mut self.damage);

        self.candidates.extend(sim.drain_structural());
        std::mem::swap(&mut self.candidates, &mut self.candidate_work);
        self.candidate_work
            .sort_unstable_by_key(|pos| (pos.y, pos.x));
        self.candidate_work.dedup();
        for seed in self.candidate_work.drain(..) {
            if sim.get_cell(seed).is_some_and(|cell| cell.is_body()) {
                self.set.wake_at(seed);
                continue;
            }
            let Some(island) = detect_island(sim, seed) else {
                continue;
            };
            if !island_simulated(sim, tickets, &island) {
                self.candidates.push(seed);
                continue;
            }
            self.set.register_island(sim, &island);
        }

        self.actor_players.clear();
        self.actors.clear();
        for (&id, player) in players.iter() {
            let Some(avatar) = player.avatar() else {
                continue;
            };
            self.actor_players.push(id);
            self.actors.push(ActorDynamics {
                bbox: ActorAabb::from_footprint(avatar.actor.footprint()),
                vx: avatar.actor.vx.to_cells_per_second(),
                vy: avatar.actor.vy.to_cells_per_second(),
                inv_mass: 1.0 / PLAYER_MASS,
            });
        }

        {
            let impulses = self.set.step(sim, &self.actors, BODY_GRAVITY, &|pos| {
                tickets.simulates(pos)
            });
            for (&player, &(jx, jy)) in self.actor_players.iter().zip(impulses) {
                if (jx != 0.0 || jy != 0.0)
                    && let Some(avatar) = players
                        .get_mut(player)
                        .and_then(|player| player.avatar_mut())
                {
                    avatar.pending_impulse.0 += jx;
                    avatar.pending_impulse.1 += jy;
                }
            }
        }

        self.set.settle_resting(sim);
        BodyStepMetrics {
            bodies: self.set.len(),
        }
    }
}

fn body_overlaps_region(body: &PixelBody, pos: RegionPos) -> bool {
    let radius = ((body.width() as f32).hypot(body.height() as f32) + 1.0).ceil() as i32;
    let base = pos.base_chunk().base_cell();
    let CellPos { x: cx, y: cy } = body.center_cell();
    cx + radius > base.x
        && cx - radius < base.x + REGION_SIZE_CELLS as i32
        && cy + radius > base.y
        && cy - radius < base.y + REGION_SIZE_CELLS as i32
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
