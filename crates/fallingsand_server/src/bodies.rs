use crate::player::{PLAYER_MASS, Players};
use crate::regions::ChunkTickets;
use fallingsand_core::{CellPos, Fixed, TICK_DT};
use fallingsand_protocol::ServerStats;
use fallingsand_sim::bodies::{
    ActorDynamics, OwnerMap, PixelBody, apply_damage, detect_island, register_body,
    step_bodies as simulate_bodies, wake_covering,
};
use fallingsand_sim::{ActorAabb, CellWorld};

pub const BODY_GRAVITY: Fixed = Fixed::from_int(-400);
const STANDING_TORQUE: f32 = 0.15;

#[derive(Default)]
pub struct PixelBodies {
    pub bodies: Vec<PixelBody>,
    pub owners: OwnerMap,
    pub next_id: u32,
    pub candidates: Vec<CellPos>,
}

impl PixelBodies {
    pub fn refresh_owners(&mut self) {
        self.owners.rebuild(&self.bodies);
    }

    pub fn body_at_mut(&mut self, pos: CellPos) -> Option<&mut PixelBody> {
        self.owners
            .get(pos)
            .and_then(|index| self.bodies.get_mut(index))
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
    }

    bodies.refresh_owners();
    bodies.candidates.extend(sim.take_structural());
    let mut candidates = std::mem::take(&mut bodies.candidates);
    candidates.sort_unstable_by_key(|pos| (pos.y, pos.x));
    candidates.dedup();
    for seed in candidates {
        if sim.get_cell(seed).is_some_and(|cell| cell.is_body()) {
            wake_covering(&mut bodies.bodies, &bodies.owners, seed);
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
    }
    bodies.refresh_owners();

    let mut actor_players = Vec::new();
    let mut entities: Vec<ActorDynamics> = Vec::new();
    let mut grounded: Vec<bool> = Vec::new();
    for (&id, player) in players.iter() {
        let Some(avatar) = player.avatar() else {
            continue;
        };
        actor_players.push(id);
        grounded.push(avatar.actor.on_ground);
        entities.push(ActorDynamics {
            bbox: ActorAabb::from_footprint(avatar.actor.footprint()),
            vx: avatar.actor.vx.vel_f32(),
            vy: avatar.actor.vy.vel_f32(),
            inv_mass: 1.0 / PLAYER_MASS,
        });
    }

    for (dynamics, on_ground) in entities.iter().zip(&grounded) {
        if *on_ground {
            transfer_standing_weight(sim, bodies, dynamics);
        }
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

fn transfer_standing_weight(world: &CellWorld, bodies: &mut PixelBodies, dynamics: &ActorDynamics) {
    let bbox = dynamics.bbox;
    let row = (bbox.y - bbox.half_h).floor_cell() - 1;
    let x0 = (bbox.x - bbox.half_w).floor_cell();
    let x1 = (bbox.x + bbox.half_w).max_cell();
    let mut supports: Vec<CellPos> = Vec::new();
    for x in x0..=x1 {
        let pos = CellPos::new(x, row);
        if world.get_cell(pos).is_some_and(|cell| cell.is_body()) {
            supports.push(pos);
        }
    }
    if supports.is_empty() {
        return;
    }
    let share = PLAYER_MASS * BODY_GRAVITY.to_f32() * TICK_DT / supports.len() as f32;
    for pos in supports {
        let Some(body) = bodies.body_at_mut(pos) else {
            continue;
        };
        if body.frozen {
            continue;
        }
        let rx = (Fixed::cell_center(pos.x) - body.x).to_f32();
        body.vy = body.vy.add_vel_f32(share * body.inv_mass());
        body.spin += rx * share * body.inv_inertia() * STANDING_TORQUE;
        body.rest_secs = 0.0;
        body.asleep = false;
    }
}
