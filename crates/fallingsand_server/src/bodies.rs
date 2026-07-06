use crate::session::{SessionState, Sessions};
use crate::systems::{PLAYER_MASS, PhysicsBody};
use crate::{PlayerImpulses, Registry, SimObstacles, SimWorld, TickStats};
use bevy_ecs::prelude::*;
use fallingsand_core::{CellPos, Fixed, TICK_DT};
use fallingsand_protocol::{PixelBodyState, ServerMessage, encode_message};
use fallingsand_sim::bodies::{
    EntityDynamics, PixelBody, detect_island, extract_body, react_body, refresh_body,
    step_bodies as simulate_bodies, try_stamp,
};
use fallingsand_sim::{EntityBox, Obstacles};
use rustc_hash::FxHashSet;

pub const BODY_GRAVITY: Fixed = Fixed::from_int(-400);

#[derive(Resource, Default)]
pub struct PixelBodies {
    pub bodies: Vec<PixelBody>,
    pub next_id: u32,
    pub candidates: Vec<CellPos>,
    pub spawned: Vec<u32>,
    pub despawned: Vec<u32>,
    pub dirty: Vec<u32>,
}

impl PixelBodies {
    pub fn body_mut(&mut self, id: u32) -> Option<&mut PixelBody> {
        self.bodies.iter_mut().find(|body| body.id == id)
    }
}

pub fn step_bodies(
    mut sim: ResMut<SimWorld>,
    registry: Res<Registry>,
    obstacles: Res<SimObstacles>,
    mut bodies: ResMut<PixelBodies>,
    mut impulses: ResMut<PlayerImpulses>,
    mut stats: ResMut<TickStats>,
    query: Query<(Entity, &PhysicsBody)>,
) {
    let bodies = &mut *bodies;
    bodies.candidates.extend(sim.0.take_structural());
    let mut candidates = std::mem::take(&mut bodies.candidates);
    candidates.sort_unstable_by_key(|pos| (pos.y, pos.x));
    candidates.dedup();
    for seed in candidates {
        let Some(island) = detect_island(&sim.0, &registry.0, seed) else {
            continue;
        };
        let id = bodies.next_id;
        bodies.next_id += 1;
        let body = extract_body(&mut sim.0, &registry.0, id, &island);
        bodies.spawned.push(id);
        bodies.bodies.push(body);
    }

    let mut players: Vec<Entity> = Vec::new();
    let mut entities: Vec<EntityDynamics> = Vec::new();
    let mut grounded: Vec<bool> = Vec::new();
    for (entity, body) in &query {
        players.push(entity);
        grounded.push(body.0.on_ground);
        entities.push(EntityDynamics {
            bbox: EntityBox {
                x: body.0.x,
                y: body.0.y,
                half_w: body.0.half_w,
                half_h: body.0.half_h,
            },
            vx: body.0.vx.to_f32(),
            vy: body.0.vy.to_f32(),
            inv_mass: 1.0 / PLAYER_MASS,
        });
    }

    for (dynamics, on_ground) in entities.iter().zip(&grounded) {
        if *on_ground {
            transfer_standing_weight(&obstacles.0, bodies, dynamics);
        }
    }

    let step = simulate_bodies(
        &sim.0,
        &registry.0,
        &obstacles.0,
        &mut bodies.bodies,
        &entities,
        BODY_GRAVITY,
    );
    for (player, (jx, jy)) in players.iter().zip(step.entity_impulses) {
        if jx != 0.0 || jy != 0.0 {
            let entry = impulses.0.entry(*player).or_insert((0.0, 0.0));
            entry.0 += jx;
            entry.1 += jy;
        }
    }

    let entity_boxes: Vec<EntityBox> = entities.iter().map(|entity| entity.bbox).collect();
    for index in step.settled.into_iter().rev() {
        if try_stamp(
            &mut sim.0,
            &registry.0,
            &entity_boxes,
            &bodies.bodies[index],
        ) {
            let body = bodies.bodies.swap_remove(index);
            bodies.despawned.push(body.id);
        } else {
            bodies.bodies[index].rest_secs = fallingsand_sim::bodies::SETTLE_SECS;
        }
    }

    stats.pixel_bodies = bodies.bodies.len();
}

fn transfer_standing_weight(
    obstacles: &Obstacles,
    bodies: &mut PixelBodies,
    dynamics: &EntityDynamics,
) {
    let bbox = dynamics.bbox;
    let row = (bbox.y - bbox.half_h).floor_cell() - 1;
    let x0 = (bbox.x - bbox.half_w).floor_cell();
    let x1 = (bbox.x + bbox.half_w).max_cell();
    let mut supports: Vec<(u32, CellPos)> = Vec::new();
    for x in x0..=x1 {
        let pos = CellPos::new(x, row);
        if let Some((id, _)) = obstacles.body_at(pos) {
            supports.push((id, pos));
        }
    }
    if supports.is_empty() {
        return;
    }
    let share = PLAYER_MASS * BODY_GRAVITY.to_f32() * TICK_DT / supports.len() as f32;
    for (id, pos) in supports {
        let Some(body) = bodies.body_mut(id) else {
            continue;
        };
        let rx = (Fixed::cell_center(pos.x) - body.x).to_f32();
        body.vy = body.vy.add_f32(share * body.inv_mass);
        body.spin += rx * share * body.inv_inertia;
        body.rest_secs = 0.0;
    }
}

pub fn react_bodies(
    mut sim: ResMut<SimWorld>,
    registry: Res<Registry>,
    mut bodies: ResMut<PixelBodies>,
) {
    let tick = sim.0.tick();
    let bodies = &mut *bodies;
    let previous = std::mem::take(&mut bodies.bodies);
    for mut body in previous {
        if !react_body(&mut sim.0, &registry.0, &mut body, tick) {
            bodies.bodies.push(body);
            continue;
        }
        let parts = refresh_body(&body, &registry.0, || {
            let id = bodies.next_id;
            bodies.next_id += 1;
            id
        });
        if parts.is_empty() {
            bodies.despawned.push(body.id);
            continue;
        }
        for part in parts {
            if part.id == body.id {
                bodies.dirty.push(part.id);
            } else {
                bodies.spawned.push(part.id);
            }
            bodies.bodies.push(part);
        }
    }
}

pub fn replicate_bodies(mut sessions: ResMut<Sessions>, mut bodies: ResMut<PixelBodies>) {
    let mut resend: FxHashSet<u32> = bodies.spawned.iter().copied().collect();
    resend.extend(bodies.dirty.iter().copied());
    let spawn_messages: Vec<Vec<u8>> = resend
        .iter()
        .filter_map(|id| bodies.bodies.iter().find(|body| body.id == *id))
        .map(|body| encode_message(&body_spawn_message(body)))
        .collect();
    let despawn_messages: Vec<Vec<u8>> = bodies
        .despawned
        .iter()
        .map(|id| encode_message(&ServerMessage::PixelBodyDespawn { id: *id }))
        .collect();

    let states = ServerMessage::PixelBodyStates {
        bodies: bodies
            .bodies
            .iter()
            .map(|body| PixelBodyState {
                id: body.id,
                x: body.x,
                y: body.y,
                angle: body.angle,
            })
            .collect(),
    };
    let states_message = encode_message(&states);

    for session in &mut sessions.sessions {
        if !matches!(session.state, SessionState::Playing) {
            continue;
        }
        for message in &spawn_messages {
            session.conn.send(message.clone());
        }
        for message in &despawn_messages {
            session.conn.send(message.clone());
        }
        if !bodies.bodies.is_empty() {
            session.conn.send(states_message.clone());
        }
    }

    bodies.spawned.clear();
    bodies.despawned.clear();
    bodies.dirty.clear();
}

fn body_spawn_message(body: &PixelBody) -> ServerMessage {
    ServerMessage::PixelBodySpawn {
        id: body.id,
        width: body.width,
        height: body.height,
        com_x: body.com_local.0,
        com_y: body.com_local.1,
        cells: fallingsand_protocol::cells_to_wire(&body.cells),
    }
}

pub fn full_body_sync(bodies: &PixelBodies) -> Vec<Vec<u8>> {
    bodies
        .bodies
        .iter()
        .map(|body| encode_message(&body_spawn_message(body)))
        .collect()
}
