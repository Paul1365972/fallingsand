use crate::regions::Store;
use crate::session::{Player, Session, SessionState, Sessions};
use crate::{
    INTEREST_RADIUS_X, INTEREST_RADIUS_Y, NetListener, Registry, SimObstacles, SimWorld,
    SpawnPoint, TickStats,
};
use bevy_ecs::prelude::*;
use fallingsand_core::{CellOffset, CellPos, MaterialId, Phase, TICK_DT};
use fallingsand_protocol::{
    ClientMessage, EntityState, PROTOCOL_VERSION, PlayerId, ServerMessage, cells_to_wire,
    decode_message, encode_message,
};
use fallingsand_sim::EntityBox;
use fallingsand_sim::physics::{Body, Controller, PlayerParams, scatter_powder, step_player};
use rustc_hash::FxHashSet;
use std::time::Instant;

pub const PLAYER_HALF_W: f32 = 1.9;
pub const PLAYER_HALF_H: f32 = 5.5;
pub const REACH: f32 = 80.0;
pub const BRUSH_RADIUS: i32 = 3;
pub use crate::MAX_HP;
const CHAT_MAX_CHARS: usize = 240;
const CHAT_RATE_SECS: f32 = 0.25;
const CHAT_RATE_TICKS: u64 = (CHAT_RATE_SECS * crate::TICK_RATE as f32) as u64;
const SAFE_IMPACT_SPEED: f32 = 300.0;
const IMPACT_DAMAGE_SCALE: f32 = 0.3;

#[derive(Component)]
pub struct PhysicsBody(pub Body);

#[derive(Component, Default)]
pub struct Control(pub Controller);

#[derive(Component)]
pub struct Health {
    pub hp: f32,
    pub previous_vy: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            hp: MAX_HP,
            previous_vy: 0.0,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn drain_network(
    mut commands: Commands,
    mut listener: ResMut<NetListener>,
    mut sessions: ResMut<Sessions>,
    mut players: Query<(&mut Player, &PhysicsBody)>,
    registry: Res<Registry>,
    sim: Res<SimWorld>,
    spawn_point: Res<SpawnPoint>,
    store: Res<Store>,
    bodies: Res<crate::bodies::PixelBodies>,
) {
    while let Some(conn) = listener.0.poll_accept() {
        sessions.sessions.push(Session::new(conn));
    }

    let sessions = &mut *sessions;
    let mut joined: Vec<(PlayerId, String)> = Vec::new();
    let mut left: Vec<PlayerId> = Vec::new();
    let mut chats: Vec<(PlayerId, String, String)> = Vec::new();

    for index in 0..sessions.sessions.len() {
        while let Some(bytes) = sessions.sessions[index].conn.poll() {
            let Ok(message) = decode_message::<ClientMessage>(&bytes) else {
                tracing::warn!("closing connection: malformed message");
                sessions.sessions[index].conn.close("malformed message");
                break;
            };
            match message {
                ClientMessage::Hello {
                    protocol_version,
                    uuid,
                    name,
                } => {
                    if !matches!(sessions.sessions[index].state, SessionState::AwaitingHello) {
                        continue;
                    }
                    if protocol_version != PROTOCOL_VERSION {
                        tracing::warn!(
                            "rejected {name}: protocol {protocol_version} != {PROTOCOL_VERSION}"
                        );
                        let session = &mut sessions.sessions[index];
                        session.conn.send(encode_message(&ServerMessage::Reject {
                            reason: format!(
                                "protocol version mismatch: server {PROTOCOL_VERSION}, client {protocol_version}"
                            ),
                        }));
                        session.conn.close("protocol version mismatch");
                        continue;
                    }
                    let player = PlayerId(sessions.next_player);
                    sessions.next_player += 1;

                    let mut taken_entity = None;
                    for other in &mut sessions.sessions {
                        if other.uuid == Some(uuid) {
                            other.conn.send(encode_message(&ServerMessage::Reject {
                                reason: "superseded by a new session".into(),
                            }));
                            other.conn.close("superseded by a new session");
                            other.uuid = None;
                            taken_entity = other.entity.take().or(taken_entity);
                            if let Some(old) = other.player.take() {
                                left.push(old);
                            }
                        }
                    }

                    let mut takeover = None;
                    if let Some(entity) = taken_entity {
                        if let Ok((mut existing, body)) = players.get_mut(entity) {
                            existing.id = player;
                            existing.name = name.clone();
                            existing.input = Default::default();
                            takeover =
                                Some((entity, CellPos::new(body.0.x as i32, body.0.y as i32)));
                        } else {
                            commands.entity(entity).despawn();
                        }
                    }
                    let (entity, spawn) = match takeover {
                        Some(takeover) => takeover,
                        None => {
                            let restored = store
                                .0
                                .as_ref()
                                .and_then(|store| store.load_player(uuid).ok().flatten());
                            let spawn = match &restored {
                                Some(record) => CellPos::new(record.x as i32, record.y as i32),
                                None => spawn_point.0,
                            };
                            let entity = commands
                                .spawn((
                                    Player {
                                        id: player,
                                        uuid,
                                        name: name.clone(),
                                        input: Default::default(),
                                    },
                                    PhysicsBody(Body::new(
                                        restored.as_ref().map(|r| r.x).unwrap_or(spawn.x as f32),
                                        restored.as_ref().map(|r| r.y).unwrap_or(spawn.y as f32),
                                        PLAYER_HALF_W,
                                        PLAYER_HALF_H,
                                    )),
                                    Control::default(),
                                    Health {
                                        hp: restored
                                            .as_ref()
                                            .map(|r| r.hp)
                                            .filter(|hp| hp.is_finite() && *hp > 0.0)
                                            .unwrap_or(MAX_HP)
                                            .min(MAX_HP),
                                        previous_vy: 0.0,
                                    },
                                ))
                                .id();
                            (entity, spawn)
                        }
                    };

                    let session = &mut sessions.sessions[index];
                    session.state = SessionState::Playing;
                    session.entity = Some(entity);
                    session.player = Some(player);
                    session.uuid = Some(uuid);
                    session.conn.send(encode_message(&ServerMessage::HelloAck {
                        protocol_version: PROTOCOL_VERSION,
                        registry_hash: registry.0.hash(),
                        player,
                        tick: sim.0.tick(),
                        spawn,
                    }));
                    for message in crate::bodies::full_body_sync(&bodies) {
                        session.conn.send(message);
                    }
                    session
                        .conn
                        .send(encode_message(&ServerMessage::PlayerJoined {
                            player,
                            name: name.clone(),
                        }));
                    for (existing, _) in players.iter() {
                        if existing.id == player {
                            continue;
                        }
                        session
                            .conn
                            .send(encode_message(&ServerMessage::PlayerJoined {
                                player: existing.id,
                                name: existing.name.clone(),
                            }));
                    }
                    tracing::info!("{name} ({uuid}) joined as player {}", player.0);
                    joined.push((player, name));
                }
                ClientMessage::Input(input) => {
                    if let Some(entity) = sessions.sessions[index].entity
                        && let Ok((mut player, _)) = players.get_mut(entity)
                    {
                        player.input = input;
                    }
                }
                ClientMessage::Chat { text } => {
                    let session = &mut sessions.sessions[index];
                    if !matches!(session.state, SessionState::Playing) {
                        continue;
                    }
                    let (Some(entity), Some(player)) = (session.entity, session.player) else {
                        continue;
                    };
                    let tick = sim.0.tick();
                    if session.last_chat_tick != 0
                        && tick.saturating_sub(session.last_chat_tick) < CHAT_RATE_TICKS
                    {
                        continue;
                    }
                    let text: String = text.trim().chars().take(CHAT_MAX_CHARS).collect();
                    if text.is_empty() {
                        continue;
                    }
                    session.last_chat_tick = tick;
                    if let Ok((sender, _)) = players.get(entity) {
                        chats.push((player, sender.name.clone(), text));
                    }
                }
                ClientMessage::Goodbye => {
                    sessions.sessions[index].conn.close("client goodbye");
                }
            }
        }
    }

    sessions.sessions.retain(|session| {
        if let fallingsand_net::ConnectionStatus::Closed { reason } = session.conn.status() {
            if let Some(entity) = session.entity {
                if let Ok((player, _)) = players.get(entity) {
                    tracing::info!("{} left: {reason}", player.name);
                }
                commands.entity(entity).despawn();
            }
            if let Some(player) = session.player {
                left.push(player);
            }
            false
        } else {
            true
        }
    });

    for session in &mut sessions.sessions {
        if !matches!(session.state, SessionState::Playing) {
            continue;
        }
        for (player, name) in &joined {
            if session.player != Some(*player) {
                session
                    .conn
                    .send(encode_message(&ServerMessage::PlayerJoined {
                        player: *player,
                        name: name.clone(),
                    }));
            }
        }
        for player in &left {
            session
                .conn
                .send(encode_message(&ServerMessage::PlayerLeft {
                    player: *player,
                }));
        }
        for (player, name, text) in &chats {
            session.conn.send(encode_message(&ServerMessage::Chat {
                player: *player,
                name: name.clone(),
                text: text.clone(),
            }));
        }
    }
}

pub fn apply_player_inputs(
    mut sim: ResMut<SimWorld>,
    registry: Res<Registry>,
    obstacles: Res<SimObstacles>,
    mut bodies: ResMut<crate::bodies::PixelBodies>,
    query: Query<(&Player, &PhysicsBody)>,
) {
    for (player, body) in &query {
        let input = &player.input;
        if !input.primary && !input.secondary {
            continue;
        }
        let dx = input.aim.x as f32 - body.0.x;
        let dy = input.aim.y as f32 - body.0.y;
        if dx * dx + dy * dy > REACH * REACH {
            continue;
        }
        let mut dug = false;
        for oy in -BRUSH_RADIUS..=BRUSH_RADIUS {
            for ox in -BRUSH_RADIUS..=BRUSH_RADIUS {
                if ox * ox + oy * oy > BRUSH_RADIUS * BRUSH_RADIUS {
                    continue;
                }
                let pos = input.aim.translated(ox, oy);
                let Some(cell) = sim.0.get_cell(pos) else {
                    continue;
                };
                if input.primary {
                    let phase = registry.0.get(cell.material).phase;
                    if phase != Phase::Empty {
                        sim.0.place_material(pos, MaterialId::AIR);
                        dug = true;
                    }
                } else if cell.is_air()
                    && !cell_overlaps_body(pos, &body.0)
                    && !obstacles.0.occupied(pos)
                {
                    sim.0.place_material(pos, input.selected);
                }
            }
        }
        if dug {
            let ring = BRUSH_RADIUS + 1;
            for oy in -ring..=ring {
                for ox in -ring..=ring {
                    let dist_sq = ox * ox + oy * oy;
                    if dist_sq <= BRUSH_RADIUS * BRUSH_RADIUS || dist_sq > ring * ring {
                        continue;
                    }
                    bodies.candidates.push(input.aim.translated(ox, oy));
                }
            }
        }
    }
}

fn cell_overlaps_body(pos: CellPos, body: &Body) -> bool {
    let cx = pos.x as f32 + 0.5;
    let cy = pos.y as f32 + 0.5;
    (cx - body.x).abs() < body.half_w + 0.5 && (cy - body.y).abs() < body.half_h + 0.5
}

pub fn build_obstacles(
    mut sim: ResMut<SimWorld>,
    registry: Res<Registry>,
    mut obstacles: ResMut<SimObstacles>,
    bodies: Res<crate::bodies::PixelBodies>,
    query: Query<&PhysicsBody>,
) {
    let boxes: Vec<EntityBox> = query
        .iter()
        .map(|body| EntityBox {
            x: body.0.x,
            y: body.0.y,
            half_w: body.0.half_w,
            half_h: body.0.half_h,
        })
        .collect();
    obstacles
        .0
        .rebuild(&mut sim.0, &registry.0, &boxes, &bodies.bodies);
}

pub fn step_simulation(
    mut sim: ResMut<SimWorld>,
    registry: Res<Registry>,
    obstacles: Res<SimObstacles>,
    tickets: Res<crate::regions::ChunkTickets>,
    mut stats: ResMut<TickStats>,
) {
    let start = Instant::now();
    fallingsand_sim::step_scoped(&mut sim.0, &registry.0, &obstacles.0, &|pos| {
        tickets.simulates(pos)
    });
    stats.tick = sim.0.tick();
    stats.sim_micros = start.elapsed().as_micros() as u64;
    stats.active_chunks = tickets.active.len();
    stats.border_chunks = tickets.border.len();
}

pub fn step_physics(
    mut sim: ResMut<SimWorld>,
    registry: Res<Registry>,
    obstacles: Res<SimObstacles>,
    spawn_point: Res<SpawnPoint>,
    mut query: Query<(&Player, &mut PhysicsBody, &mut Control, &mut Health)>,
) {
    let params = PlayerParams::default();
    for (player, mut body, mut control, mut health) in &mut query {
        let falling_speed = -body.0.vy;
        let displaced = {
            let source = obstacles.0.overlay(&sim.0);
            step_player(
                &source,
                &registry.0,
                &params,
                &mut body.0,
                &mut control.0,
                player.input.move_x,
                player.input.jump,
                player.input.down,
                TICK_DT,
            )
        };
        if !displaced.is_empty() {
            scatter_powder(&mut sim.0, &registry.0, &obstacles.0, &body.0, &displaced);
        }
        let landed = body.0.on_ground && health.previous_vy < -SAFE_IMPACT_SPEED;
        if landed {
            health.hp -= (falling_speed - SAFE_IMPACT_SPEED).max(0.0) * IMPACT_DAMAGE_SCALE;
        }
        health.previous_vy = body.0.vy;
        if health.hp <= 0.0 {
            health.hp = MAX_HP;
            health.previous_vy = 0.0;
            body.0 = Body::new(
                spawn_point.0.x as f32,
                spawn_point.0.y as f32,
                PLAYER_HALF_W,
                PLAYER_HALF_H,
            );
            control.0 = Controller::default();
        }
    }
}

pub fn replicate(
    mut sessions: ResMut<Sessions>,
    sim: Res<SimWorld>,
    mut stats: ResMut<TickStats>,
    query: Query<(&Player, &PhysicsBody, &Health)>,
) {
    let entities: Vec<EntityState> = query
        .iter()
        .map(|(player, body, health)| EntityState {
            player: player.id,
            x: body.0.x,
            y: body.0.y,
            hp: health.hp,
        })
        .collect();
    let entity_message = encode_message(&ServerMessage::EntityStates {
        entities: entities.clone(),
    });

    let mut sent_bytes = 0u64;

    for session in &mut sessions.sessions {
        if !matches!(session.state, SessionState::Playing) {
            continue;
        }
        let Some(entity) = session.entity else {
            continue;
        };
        let Some((_, body, _)) = query.get(entity).ok() else {
            continue;
        };

        let center = CellPos::new(body.0.x as i32, body.0.y as i32).chunk();
        let mut interest = FxHashSet::default();
        for dy in -INTEREST_RADIUS_Y..=INTEREST_RADIUS_Y {
            for dx in -INTEREST_RADIUS_X..=INTEREST_RADIUS_X {
                let pos = center.translated(dx, dy);
                if sim.0.chunk(pos).is_some() {
                    interest.insert(pos);
                }
            }
        }

        let mut known = std::mem::take(&mut session.known_chunks);
        known.retain(|&pos| {
            if interest.contains(&pos) {
                return true;
            }
            let message = encode_message(&ServerMessage::ChunkUnload { pos });
            sent_bytes += message.len() as u64;
            session.conn.send(message);
            false
        });

        for &pos in &interest {
            let chunk = sim.0.chunk(pos).expect("interest chunks are loaded");
            if known.insert(pos) {
                let message = encode_message(&ServerMessage::ChunkLoad {
                    pos,
                    cells: cells_to_wire(chunk.cells()),
                });
                sent_bytes += message.len() as u64;
                session.conn.send(message);
                continue;
            }
            let rect = chunk.dirty();
            if rect.is_empty() {
                continue;
            }
            let mut cells = Vec::with_capacity((rect.width() * rect.height()) as usize);
            for y in rect.min_y..=rect.max_y {
                for x in rect.min_x..=rect.max_x {
                    cells.push(chunk.get(CellOffset::new(x, y)));
                }
            }
            let message = encode_message(&ServerMessage::ChunkDelta {
                pos,
                rect,
                cells: cells_to_wire(&cells),
            });
            sent_bytes += message.len() as u64;
            session.conn.send(message);
        }
        session.known_chunks = known;

        sent_bytes += entity_message.len() as u64;
        session.conn.send(entity_message.clone());
    }

    stats.players = entities.len();
    stats.awake_chunks = sim.0.awake_chunk_count();
    stats.loaded_chunks = sim.0.chunks().count();
    stats.replicated_bytes = sent_bytes;
}

pub fn finish_tick(mut sessions: ResMut<Sessions>, sim: Res<SimWorld>) {
    let message = encode_message(&ServerMessage::TickEnd { tick: sim.0.tick() });
    for session in &mut sessions.sessions {
        if matches!(session.state, SessionState::Playing) {
            session.conn.send(message.clone());
        }
    }
}
