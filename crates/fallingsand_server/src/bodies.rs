use crate::player::{PLAYER_MASS, Players};
use crate::regions::ChunkTickets;
use fallingsand_core::{CellPos, REGION_SIZE_CELLS, RegionPos, Subcell};
use fallingsand_protocol::PlayerId;
use fallingsand_sim::bodies::{ActorDynamics, BodyPose, BodySet, PixelBody, detect_island};
use fallingsand_sim::{ActorAabb, CellWorld};
use std::collections::BTreeMap;
use std::time::Instant;

pub const BODY_GRAVITY: Subcell = Subcell::from_cells_per_second_squared(-400.0);

#[derive(Default)]
pub struct BodyWorld {
    set: BodySet,
    candidates: Vec<CellPos>,
    candidate_work: Vec<CellPos>,
    body_wakes: Vec<CellPos>,
    damage: Vec<CellPos>,
    actor_players: Vec<PlayerId>,
    actors: Vec<ActorDynamics>,
    dormant: BTreeMap<CellPos, BodyPose>,
}

pub struct BodyStepMetrics {
    pub bodies: usize,
}

impl BodyWorld {
    pub fn debug_cells_in(&self, chunk: fallingsand_core::ChunkPos) -> Vec<(u32, CellPos)> {
        self.set.debug_cells_in(chunk)
    }

    pub fn receive_player_contact(&mut self, pos: CellPos, wake: bool) -> Option<bool> {
        self.set.receive_player_contact(pos, wake)
    }

    pub fn settle_overlapping_regions(&mut self, sim: &mut CellWorld, regions: &[RegionPos]) {
        let poses = self.set.settle_quiet_where(sim, |body| {
            regions
                .iter()
                .copied()
                .any(|region| body_overlaps_region(body, region))
        });
        for pose in poses {
            self.dormant.insert(pose.pivot, pose);
        }
    }

    pub fn poses(&self) -> Vec<BodyPose> {
        let mut poses = self.dormant.clone();
        for pose in self.set.poses() {
            poses.insert(pose.pivot, pose);
        }
        poses.into_values().collect()
    }

    pub fn restore_poses(&mut self, poses: impl IntoIterator<Item = BodyPose>) {
        for pose in poses {
            self.candidates.push(pose.pivot);
            self.dormant.insert(pose.pivot, pose);
        }
    }

    pub fn take_dormant_poses(&mut self, region: RegionPos) -> Vec<BodyPose> {
        let pivots: Vec<_> = self
            .dormant
            .keys()
            .copied()
            .filter(|pivot| pivot.chunk().region() == region)
            .collect();
        pivots
            .into_iter()
            .filter_map(|pivot| self.dormant.remove(&pivot))
            .collect()
    }

    pub fn step(
        &mut self,
        sim: &mut CellWorld,
        tickets: &ChunkTickets,
        players: &mut Players,
    ) -> BodyStepMetrics {
        let total_start = Instant::now();
        let before = self.set.counts();

        let damage_start = Instant::now();
        self.damage.extend(sim.drain_damage());
        let damage_events = self.damage.len();
        self.set.apply_damage(sim, &mut self.damage);
        let damage_us = damage_start.elapsed().as_micros() as u64;

        let structural_start = Instant::now();
        self.candidates.extend(sim.drain_structural());
        std::mem::swap(&mut self.candidates, &mut self.candidate_work);
        let structural_raw = self.candidate_work.len();
        self.candidate_work
            .sort_unstable_by_key(|pos| (pos.y, pos.x));
        self.candidate_work.dedup();
        let structural_unique = self.candidate_work.len();
        self.body_wakes.clear();
        let mut island_attempts = 0usize;
        let mut islands_detected = 0usize;
        let mut islands_deferred = 0usize;
        let mut islands_registered = 0usize;
        let mut island_cells_total = 0usize;
        let mut island_cells_max = 0usize;
        for seed in self.candidate_work.drain(..) {
            if sim.get_cell(seed).is_some_and(|cell| cell.is_body()) {
                self.body_wakes.push(seed);
                continue;
            }
            island_attempts += 1;
            let Some(island) = detect_island(sim, seed) else {
                continue;
            };
            islands_detected += 1;
            island_cells_total += island.len();
            island_cells_max = island_cells_max.max(island.len());
            if !island_simulated(sim, tickets, &island) {
                self.candidates.push(seed);
                islands_deferred += 1;
                continue;
            }
            let contained: Vec<_> = self
                .dormant
                .range(..)
                .filter_map(|(&pivot, &pose)| island.contains(&pivot).then_some((pivot, pose)))
                .collect();
            if contained.len() == 1 {
                self.set
                    .register_island_with_pose(sim, &island, contained[0].1);
            } else {
                self.set.register_island(sim, &island);
            }
            islands_registered += 1;
            for (pivot, _) in contained {
                self.dormant.remove(&pivot);
            }
        }
        let wake_seeds = self.body_wakes.len();
        self.set.wake_many(&self.body_wakes);
        let structural_us = structural_start.elapsed().as_micros() as u64;

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

        let dynamics_start = Instant::now();
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
        let dynamics_us = dynamics_start.elapsed().as_micros() as u64;

        let settle_start = Instant::now();
        let settled = self.set.settle_resting(sim);
        let settled_count = settled.len();
        for pose in settled {
            self.dormant.insert(pose.pivot, pose);
        }
        let settle_us = settle_start.elapsed().as_micros() as u64;
        let after = self.set.counts();
        let total_us = total_start.elapsed().as_micros() as u64;
        if before.live != 0
            || after.live != 0
            || damage_events != 0
            || structural_raw != 0
            || total_us >= 1_000
        {
            tracing::info!(
                target: "body_diag",
                tick = sim.tick(),
                total_us,
                damage_us,
                structural_us,
                dynamics_us,
                settle_us,
                damage_events,
                structural_raw,
                structural_unique,
                wake_seeds,
                island_attempts,
                islands_detected,
                islands_deferred,
                islands_registered,
                island_cells_total,
                island_cells_max,
                retry_seeds = self.candidates.len(),
                live_before = before.live,
                live_after = after.live,
                member_cells = after.members,
                awake = after.awake,
                resting = after.resting,
                frozen = after.frozen,
                settled = settled_count,
                dormant = self.dormant.len(),
                "BODY DIAGNOSTICS"
            );
        }
        BodyStepMetrics { bodies: after.live }
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
