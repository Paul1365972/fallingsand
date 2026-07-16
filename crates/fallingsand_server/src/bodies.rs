use crate::player::{PLAYER_MASS, Players};
use crate::regions::ChunkTickets;
use fallingsand_core::{CellPos, Fixed};
use fallingsand_protocol::ServerStats;
use fallingsand_sim::bodies::{
    ActorDynamics, OwnerMap, PixelBody, SETTLE_SECS, apply_damage, detect_island, register_body,
    settle_body, step_bodies as simulate_bodies, wake_covering,
};
use fallingsand_sim::{ActorAabb, CellWorld};

pub const BODY_GRAVITY: Fixed = Fixed::from_int(-400);

#[derive(Default)]
pub struct PixelBodies {
    pub bodies: Vec<PixelBody>,
    owners: OwnerMap,
    pub next_id: u32,
    pub candidates: Vec<CellPos>,
    owners_stale: bool,
}

impl PixelBodies {
    pub fn mark_owners_stale(&mut self) {
        self.owners_stale = true;
    }

    pub fn ensure_owners(&mut self) -> &OwnerMap {
        if self.owners_stale {
            self.owners.rebuild(&self.bodies);
            self.owners_stale = false;
        }
        &self.owners
    }

    pub fn body_at_mut(&mut self, pos: CellPos) -> Option<&mut PixelBody> {
        self.ensure_owners();
        self.owners
            .get(pos)
            .and_then(|index| self.bodies.get_mut(index))
    }

    pub fn wake_at(&mut self, pos: CellPos) {
        self.ensure_owners();
        wake_covering(&mut self.bodies, &self.owners, pos);
    }
}

pub fn step_bodies(
    sim: &mut CellWorld,
    tickets: &ChunkTickets,
    bodies: &mut PixelBodies,
    players: &mut Players,
    stats: &mut ServerStats,
) {
    let damage = sim.take_damage();
    if !damage.is_empty() {
        let next_id = &mut bodies.next_id;
        apply_damage(sim, &mut bodies.bodies, damage, || {
            let id = *next_id;
            *next_id += 1;
            id
        });
        bodies.mark_owners_stale();
    }

    bodies.candidates.extend(sim.take_structural());
    let mut candidates = std::mem::take(&mut bodies.candidates);
    candidates.sort_unstable_by_key(|pos| (pos.y, pos.x));
    candidates.dedup();
    for seed in candidates {
        if sim.get_cell(seed).is_some_and(|cell| cell.is_body()) {
            bodies.wake_at(seed);
            continue;
        }
        let Some(island) = detect_island(sim, seed) else {
            continue;
        };
        if !island_simulated(sim, tickets, &island) {
            bodies.candidates.push(seed);
            continue;
        }
        let id = bodies.next_id;
        bodies.next_id += 1;
        let body = register_body(sim, id, &island);
        bodies.bodies.push(body);
        bodies.mark_owners_stale();
    }
    bodies.ensure_owners();

    let mut actor_players = Vec::new();
    let mut entities: Vec<ActorDynamics> = Vec::new();
    for (&id, player) in players.iter() {
        let Some(avatar) = player.avatar() else {
            continue;
        };
        actor_players.push(id);
        entities.push(ActorDynamics {
            bbox: ActorAabb::from_footprint(avatar.actor.footprint()),
            vx: avatar.actor.vx.vel_f32(),
            vy: avatar.actor.vy.vel_f32(),
            inv_mass: 1.0 / PLAYER_MASS,
        });
    }

    let entity_impulses = simulate_bodies(
        sim,
        &mut bodies.bodies,
        &mut bodies.owners,
        &entities,
        BODY_GRAVITY,
        &|pos| tickets.simulates(pos),
    );
    for (player, (jx, jy)) in actor_players.into_iter().zip(entity_impulses) {
        if (jx != 0.0 || jy != 0.0)
            && let Some(avatar) = players
                .get_mut(player)
                .and_then(|player| player.avatar_mut())
        {
            avatar.pending_impulse.0 += jx;
            avatar.pending_impulse.1 += jy;
        }
    }

    let mut index = 0;
    while index < bodies.bodies.len() {
        let body = &bodies.bodies[index];
        if !body.frozen && body.rest_secs >= SETTLE_SECS {
            let body = bodies.bodies.swap_remove(index);
            settle_body(sim, &body);
            bodies.mark_owners_stale();
        } else {
            index += 1;
        }
    }

    stats.pixel_bodies = bodies.bodies.len();
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
