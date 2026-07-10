use crate::player::{PLAYER_MASS, PlayerActor};
use crate::regions::ChunkTickets;
use crate::{PlayerImpulses, Registry, SimWorld, TickStats};
use bevy_ecs::prelude::*;
use fallingsand_core::{CellPos, Fixed, TICK_DT};
use fallingsand_sim::bodies::{
    ActorDynamics, PixelBody, apply_damage, detect_island, register_body,
    step_bodies as simulate_bodies, wake_covering,
};
use fallingsand_sim::{ActorAabb, CellWorld};

pub const BODY_GRAVITY: Fixed = Fixed::from_int(-400);

#[derive(Resource, Default)]
pub struct PixelBodies {
    pub bodies: Vec<PixelBody>,
    pub next_id: u32,
    pub candidates: Vec<CellPos>,
}

impl PixelBodies {
    pub fn body_at_mut(&mut self, pos: CellPos) -> Option<&mut PixelBody> {
        self.bodies.iter_mut().find(|body| body.covers(pos))
    }
}

pub fn step_bodies(
    mut sim: ResMut<SimWorld>,
    registry: Res<Registry>,
    tickets: Res<ChunkTickets>,
    mut bodies: ResMut<PixelBodies>,
    mut impulses: ResMut<PlayerImpulses>,
    mut stats: ResMut<TickStats>,
    query: Query<(Entity, &PlayerActor)>,
) {
    let bodies = &mut *bodies;

    let damage = sim.0.take_damage();
    if !damage.is_empty() {
        let next_id = &mut bodies.next_id;
        apply_damage(&mut sim.0, &registry.0, &mut bodies.bodies, damage, || {
            let id = *next_id;
            *next_id += 1;
            id
        });
    }

    bodies.candidates.extend(sim.0.take_structural());
    let mut candidates = std::mem::take(&mut bodies.candidates);
    candidates.sort_unstable_by_key(|pos| (pos.y, pos.x));
    candidates.dedup();
    for seed in candidates {
        if sim.0.get_cell(seed).is_some_and(|cell| cell.is_body()) {
            wake_covering(&mut bodies.bodies, seed);
            continue;
        }
        let Some(island) = detect_island(&sim.0, &registry.0, seed) else {
            continue;
        };
        if !island_simulated(&sim.0, &tickets, &island) {
            continue;
        }
        let id = bodies.next_id;
        bodies.next_id += 1;
        let body = register_body(&mut sim.0, &registry.0, id, &island);
        bodies.bodies.push(body);
    }

    let mut players: Vec<Entity> = Vec::new();
    let mut entities: Vec<ActorDynamics> = Vec::new();
    let mut grounded: Vec<bool> = Vec::new();
    for (entity, body) in &query {
        players.push(entity);
        grounded.push(body.0.on_ground);
        entities.push(ActorDynamics {
            bbox: ActorAabb::from_footprint(body.0.footprint()),
            vx: body.0.vx.to_f32(),
            vy: body.0.vy.to_f32(),
            inv_mass: 1.0 / PLAYER_MASS,
        });
    }

    for (dynamics, on_ground) in entities.iter().zip(&grounded) {
        if *on_ground {
            transfer_standing_weight(&sim.0, bodies, dynamics);
        }
    }

    let entity_impulses = simulate_bodies(
        &mut sim.0,
        &registry.0,
        &mut bodies.bodies,
        &entities,
        BODY_GRAVITY,
        &|pos| tickets.simulates(pos),
    );
    for (player, (jx, jy)) in players.iter().zip(entity_impulses) {
        if jx != 0.0 || jy != 0.0 {
            let entry = impulses.0.entry(*player).or_insert((0.0, 0.0));
            entry.0 += jx;
            entry.1 += jy;
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
        body.vy = body.vy.add_f32(share * body.inv_mass());
        body.spin += rx * share * body.inv_inertia();
        body.rest_secs = 0.0;
        body.asleep = false;
    }
}
