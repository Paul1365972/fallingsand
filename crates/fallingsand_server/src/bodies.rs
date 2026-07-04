use crate::session::{SessionState, Sessions};
use crate::{Registry, SimObstacles, SimWorld, TickStats};
use bevy_ecs::prelude::*;
use fallingsand_core::CellPos;
use fallingsand_protocol::{PixelBodyState, ServerMessage, encode_message};
use fallingsand_sim::bodies::{PixelBody, detect_island, extract_body, stamp_body, step_body};
use rustc_hash::FxHashSet;

const BODY_GRAVITY: f32 = -400.0;
const MAX_BODIES: usize = 64;
const TICK_DT: f32 = 1.0 / crate::TICK_RATE as f32;

#[derive(Resource, Default)]
pub struct PixelBodies {
    pub bodies: Vec<PixelBody>,
    pub next_id: u32,
    pub candidates: FxHashSet<CellPos>,
    pub spawned: Vec<u32>,
    pub despawned: Vec<u32>,
}

pub fn step_bodies(
    mut sim: ResMut<SimWorld>,
    registry: Res<Registry>,
    obstacles: Res<SimObstacles>,
    mut bodies: ResMut<PixelBodies>,
    mut stats: ResMut<TickStats>,
) {
    let bodies = &mut *bodies;
    bodies.spawned.clear();
    bodies.despawned.clear();

    let candidates: Vec<CellPos> = bodies.candidates.drain().collect();
    for seed in candidates {
        if bodies.bodies.len() >= MAX_BODIES {
            break;
        }
        let Some(island) = detect_island(&sim.0, &registry.0, seed) else {
            continue;
        };
        let id = bodies.next_id;
        bodies.next_id += 1;
        let body = extract_body(&mut sim.0, &registry.0, id, &island);
        bodies.spawned.push(id);
        bodies.bodies.push(body);
    }

    let mut settled: Vec<usize> = Vec::new();
    for (index, body) in bodies.bodies.iter_mut().enumerate() {
        if step_body(
            &sim.0,
            &registry.0,
            &obstacles.0.entity_boxes,
            body,
            BODY_GRAVITY,
            TICK_DT,
        ) {
            settled.push(index);
        }
    }
    for index in settled.into_iter().rev() {
        let body = bodies.bodies.swap_remove(index);
        stamp_body(&mut sim.0, &registry.0, &body);
        bodies.despawned.push(body.id);
    }

    stats.pixel_bodies = bodies.bodies.len();
}

pub fn replicate_bodies(mut sessions: ResMut<Sessions>, bodies: Res<PixelBodies>) {
    let spawn_messages: Vec<Vec<u8>> = bodies
        .spawned
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
