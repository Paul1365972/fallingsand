use crate::commands::{PendingCommand, PendingCommands};
use crate::persistence::inventory_from_record;
use crate::regions::Store;
use crate::session::{Player, Session, SessionState, Sessions};
use crate::{
    INTEREST_RADIUS_X, INTEREST_RADIUS_Y, MAX_AIR_SECS, NetListener, Registry, SimObstacles,
    SimWorld, SpawnPoint, TickStats,
};
use bevy_ecs::prelude::*;
use fallingsand_core::{CellOffset, CellPos, Fixed, MaterialId, MaterialRegistry, Phase, TICK_DT};
use fallingsand_protocol::{
    ChunkDebugRects, ClientMessage, EntityState, GameMode, PROTOCOL_VERSION, PlayerId,
    ServerMessage, cells_to_wire, decode_message, encode_message,
};
use fallingsand_sim::EntityBox;
use fallingsand_sim::physics::{
    Body, Controller, PlayerParams, StepInput, scatter_powder, step_player,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::time::Instant;

pub const PLAYER_HALF_W: Fixed = Fixed::from_f32(1.9);
pub const PLAYER_HALF_H: Fixed = Fixed::from_f32(5.5);
pub const PLAYER_MASS: f32 = 4.0 * PLAYER_HALF_W.to_f32() * PLAYER_HALF_H.to_f32();
pub use crate::MAX_HP;
pub use fallingsand_core::{BRUSH_RADIUS, REACH, SURVIVAL_REACH};
const CHAT_MAX_CHARS: usize = 240;
const CHAT_RATE_SECS: f32 = 0.25;
const CHAT_RATE_TICKS: u64 = (CHAT_RATE_SECS * crate::TICK_RATE as f32) as u64;

#[derive(Component)]
pub struct PhysicsBody(pub Body);

#[derive(Component, Default)]
pub struct Control(pub Controller);

#[derive(Component)]
pub struct Health {
    pub hp: f32,
    pub last_damage_tick: u64,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            hp: MAX_HP,
            last_damage_tick: 0,
        }
    }
}

#[derive(Component, Default)]
pub struct DigState {
    pub budget: f32,
}

#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
pub struct Mode(pub GameMode);

#[derive(Component)]
pub struct Air {
    pub secs: f32,
}

impl Default for Air {
    fn default() -> Self {
        Self { secs: MAX_AIR_SECS }
    }
}

#[derive(Component, Default)]
pub struct Burning {
    pub secs: f32,
}

impl Burning {
    pub fn active(&self) -> bool {
        self.secs > 0.0
    }
}

#[derive(Component, Default)]
pub struct Inventory {
    pub counts: FxHashMap<MaterialId, u64>,
    pub dirty: bool,
}

impl Inventory {
    pub fn count(&self, material: MaterialId) -> u64 {
        self.counts.get(&material).copied().unwrap_or(0)
    }

    pub fn add(&mut self, material: MaterialId, amount: u64) {
        if amount == 0 {
            return;
        }
        *self.counts.entry(material).or_insert(0) += amount;
        self.dirty = true;
    }

    pub fn remove(&mut self, material: MaterialId, amount: u64) -> bool {
        if amount == 0 {
            return true;
        }
        let Some(count) = self.counts.get_mut(&material) else {
            return false;
        };
        if *count < amount {
            return false;
        }
        *count -= amount;
        if *count == 0 {
            self.counts.remove(&material);
        }
        self.dirty = true;
        true
    }
}

pub fn player_record(
    registry: &MaterialRegistry,
    body: &Body,
    health: &Health,
    mode: &Mode,
    air: &Air,
    burning: &Burning,
    inventory: &Inventory,
) -> crate::persistence::PlayerRecord {
    crate::persistence::PlayerRecord {
        x: body.x,
        y: body.y + (PLAYER_HALF_H - body.half_h),
        hp: health.hp,
        mode: mode.0,
        air: air.secs,
        burning: burning.secs,
        inventory: crate::persistence::inventory_to_record(registry, &inventory.counts),
    }
}

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub fn drain_network(
    mut commands: Commands,
    mut listener: ResMut<NetListener>,
    mut sessions: ResMut<Sessions>,
    mut pending: ResMut<PendingCommands>,
    mut players: Query<(
        &mut Player,
        &PhysicsBody,
        &Health,
        &Mode,
        &Air,
        &Burning,
        &mut Inventory,
    )>,
    registry: Res<Registry>,
    sim: Res<SimWorld>,
    spawn_point: Res<SpawnPoint>,
    store: Res<Store>,
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
                        if let Ok((mut existing, body, _, _, _, _, mut inventory)) =
                            players.get_mut(entity)
                        {
                            existing.id = player;
                            existing.name = name.clone();
                            existing.input = Default::default();
                            inventory.dirty = true;
                            takeover = Some((
                                entity,
                                CellPos::new(body.0.x.floor_cell(), body.0.y.floor_cell()),
                            ));
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
                                Some(record) => {
                                    CellPos::new(record.x.floor_cell(), record.y.floor_cell())
                                }
                                None => spawn_point.0,
                            };
                            let record = restored.as_ref();
                            let entity = commands
                                .spawn((
                                    Player {
                                        id: player,
                                        uuid,
                                        name: name.clone(),
                                        input: Default::default(),
                                    },
                                    PhysicsBody(Body::new(
                                        record.map(|r| r.x).unwrap_or(Fixed::from_cell(spawn.x)),
                                        record.map(|r| r.y).unwrap_or(Fixed::from_cell(spawn.y)),
                                        PLAYER_HALF_W,
                                        PLAYER_HALF_H,
                                    )),
                                    Control::default(),
                                    Health {
                                        hp: record
                                            .map(|r| r.hp)
                                            .filter(|hp| hp.is_finite() && *hp > 0.0)
                                            .unwrap_or(MAX_HP)
                                            .min(MAX_HP),
                                        last_damage_tick: 0,
                                    },
                                    DigState::default(),
                                    Mode(record.map(|r| r.mode).unwrap_or_default()),
                                    Air {
                                        secs: record
                                            .map(|r| r.air)
                                            .filter(|air| air.is_finite())
                                            .unwrap_or(MAX_AIR_SECS)
                                            .clamp(0.0, MAX_AIR_SECS),
                                    },
                                    Burning {
                                        secs: record
                                            .map(|r| r.burning)
                                            .filter(|secs| secs.is_finite())
                                            .unwrap_or(0.0)
                                            .max(0.0),
                                    },
                                    Inventory {
                                        counts: inventory_from_record(
                                            &registry.0,
                                            record.map(|r| r.inventory.as_slice()).unwrap_or(&[]),
                                        ),
                                        dirty: true,
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
                    session
                        .conn
                        .send(encode_message(&ServerMessage::PlayerJoined {
                            player,
                            name: name.clone(),
                        }));
                    for (existing, ..) in players.iter() {
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
                        && let Ok((mut player, ..)) = players.get_mut(entity)
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
                    if text.starts_with('/') {
                        pending.0.push(PendingCommand { entity, text });
                    } else if let Ok((sender, ..)) = players.get(entity) {
                        chats.push((player, sender.name.clone(), text));
                    }
                }
                ClientMessage::SetDebug { enabled } => {
                    sessions.sessions[index].debug = enabled;
                }
                ClientMessage::Goodbye => {
                    sessions.sessions[index].conn.close("client goodbye");
                }
            }
        }
    }

    let mut records: Vec<(
        fallingsand_protocol::PlayerUuid,
        crate::persistence::PlayerRecord,
    )> = Vec::new();
    sessions.sessions.retain(|session| {
        if let fallingsand_net::ConnectionStatus::Closed { reason } = session.conn.status() {
            if let Some(entity) = session.entity {
                if let Ok((player, body, health, mode, air, burning, inventory)) =
                    players.get(entity)
                {
                    tracing::info!("{} left: {reason}", player.name);
                    if let Some(uuid) = session.uuid {
                        records.push((
                            uuid,
                            player_record(
                                &registry.0,
                                &body.0,
                                health,
                                mode,
                                air,
                                burning,
                                inventory,
                            ),
                        ));
                    }
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
    if let Some(store) = store.0.as_ref()
        && let Err(err) = store.save_players(&records)
    {
        tracing::error!("failed to save disconnected players: {err}");
    }

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
    mut query: Query<(&Player, &PhysicsBody, &Mode, &mut DigState, &mut Inventory)>,
) {
    for (player, body, mode, mut dig, mut inventory) in &mut query {
        let input = &player.input;
        let survival = mode.0 == GameMode::Survival;
        if !input.primary {
            dig.budget = 0.0;
        }
        if !input.primary && !input.secondary {
            continue;
        }
        let reach = if survival { SURVIVAL_REACH } else { REACH };
        let dx = (Fixed::from_cell(input.aim.x) - body.0.x).to_f32();
        let dy = (Fixed::from_cell(input.aim.y) - body.0.y).to_f32();
        if dx * dx + dy * dy > reach * reach {
            continue;
        }
        let mut dug = false;
        if input.primary {
            if survival {
                dug = survival_dig(&mut sim.0, &registry.0, &mut dig, &mut inventory, input.aim);
            } else {
                for (_, pos) in brush_cells(input.aim) {
                    let Some(cell) = sim.0.get_cell(pos) else {
                        continue;
                    };
                    if registry.0.get(cell.material).phase != Phase::Empty {
                        sim.0.place_material(pos, MaterialId::AIR);
                        dug = true;
                    }
                }
            }
        } else if registry
            .0
            .try_get(input.selected)
            .is_some_and(|material| material.phase != Phase::Empty)
        {
            'place: for (_, pos) in brush_cells(input.aim) {
                let Some(cell) = sim.0.get_cell(pos) else {
                    continue;
                };
                if !cell.is_air() || cell_overlaps_body(pos, &body.0) || obstacles.0.occupied(pos) {
                    continue;
                }
                if survival && !inventory.remove(input.selected, 1) {
                    break 'place;
                }
                sim.0.place_material(pos, input.selected);
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

fn brush_cells(aim: CellPos) -> impl Iterator<Item = (i32, CellPos)> {
    (-BRUSH_RADIUS..=BRUSH_RADIUS).flat_map(move |oy| {
        (-BRUSH_RADIUS..=BRUSH_RADIUS).filter_map(move |ox| {
            let dist_sq = ox * ox + oy * oy;
            (dist_sq <= BRUSH_RADIUS * BRUSH_RADIUS).then_some((dist_sq, aim.translated(ox, oy)))
        })
    })
}

pub fn survival_dig(
    world: &mut fallingsand_sim::CellWorld,
    registry: &MaterialRegistry,
    dig: &mut DigState,
    inventory: &mut Inventory,
    aim: CellPos,
) -> bool {
    let mut candidates: Vec<(i32, i32, i32)> = brush_cells(aim)
        .filter(|&(_, pos)| {
            world.get_cell(pos).is_some_and(|cell| {
                matches!(
                    registry.get(cell.material).phase,
                    Phase::Solid | Phase::Powder
                )
            })
        })
        .map(|(dist_sq, pos)| (dist_sq, pos.y, pos.x))
        .collect();
    candidates.sort_unstable();
    let Some(&(_, y, x)) = candidates.first() else {
        dig.budget = 0.0;
        return false;
    };
    let closest_cost = world
        .get_cell(CellPos::new(x, y))
        .map(|cell| registry.get(cell.material).hardness)
        .unwrap_or(0.0);
    dig.budget = (dig.budget + TICK_DT).min(closest_cost + TICK_DT);
    let mut dug = false;
    for &(_, y, x) in &candidates {
        let pos = CellPos::new(x, y);
        let Some(cell) = world.get_cell(pos) else {
            continue;
        };
        let cost = registry.get(cell.material).hardness;
        if dig.budget < cost {
            break;
        }
        dig.budget -= cost;
        world.place_material(pos, MaterialId::AIR);
        inventory.add(cell.material, 1);
        dug = true;
    }
    dug
}

pub fn sync_inventories(mut sessions: ResMut<Sessions>, mut query: Query<&mut Inventory>) {
    for session in &mut sessions.sessions {
        if !matches!(session.state, SessionState::Playing) {
            continue;
        }
        let Some(entity) = session.entity else {
            continue;
        };
        let Ok(mut inventory) = query.get_mut(entity) else {
            continue;
        };
        if !inventory.dirty {
            continue;
        }
        inventory.dirty = false;
        let mut counts: Vec<(MaterialId, u64)> = inventory
            .counts
            .iter()
            .filter(|&(_, &count)| count > 0)
            .map(|(&id, &count)| (id, count))
            .collect();
        counts.sort_unstable();
        session
            .conn
            .send(encode_message(&ServerMessage::Inventory { counts }));
    }
}

fn cell_overlaps_body(pos: CellPos, body: &Body) -> bool {
    let cx = Fixed::cell_center(pos.x);
    let cy = Fixed::cell_center(pos.y);
    (cx - body.x).abs() < body.half_w + Fixed::HALF
        && (cy - body.y).abs() < body.half_h + Fixed::HALF
}

pub fn build_obstacles(
    mut sim: ResMut<SimWorld>,
    registry: Res<Registry>,
    mut obstacles: ResMut<SimObstacles>,
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
    obstacles.0.rebuild(&mut sim.0, &registry.0, &boxes);
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

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub fn step_physics(
    mut sim: ResMut<SimWorld>,
    registry: Res<Registry>,
    obstacles: Res<SimObstacles>,
    spawn_point: Res<SpawnPoint>,
    mut bodies: ResMut<crate::bodies::PixelBodies>,
    mut impulses: ResMut<crate::PlayerImpulses>,
    mut crushes: ResMut<crate::hazards::CrushEvents>,
    mut query: Query<(
        Entity,
        &Player,
        &Mode,
        &mut PhysicsBody,
        &mut Control,
        &mut Health,
        &mut Air,
        &mut Burning,
    )>,
) {
    let params = PlayerParams::default();
    for (entity, player, mode, mut body, mut control, mut health, mut air, mut burning) in
        &mut query
    {
        if let Some((jx, jy)) = impulses.0.remove(&entity) {
            let dvx = jx / PLAYER_MASS;
            let dvy = jy / PLAYER_MASS;
            body.0.vx = body.0.vx.add_f32(dvx);
            body.0.vy = body.0.vy.add_f32(dvy);
            crushes.0.push((entity, dvx.hypot(dvy)));
        }
        let result = step_player(
            &sim.0,
            &registry.0,
            &params,
            &mut body.0,
            &mut control.0,
            StepInput {
                move_x: player.input.move_x,
                jump: player.input.jump,
                down: player.input.down,
                fly: player.input.fly && mode.0 == GameMode::Creative,
            },
        );
        if !result.displaced.is_empty() {
            scatter_powder(
                &mut sim.0,
                &registry.0,
                &obstacles.0,
                &body.0,
                &result.displaced,
            );
        }
        for blocked in &result.blocked {
            if !sim
                .0
                .get_cell(blocked.pos)
                .is_some_and(|cell| cell.is_body())
            {
                continue;
            }
            let Some(pixel_body) = bodies.body_at_mut(blocked.pos) else {
                continue;
            };
            if pixel_body.frozen {
                continue;
            }
            let jx = PLAYER_MASS * blocked.dvx;
            let jy = PLAYER_MASS * blocked.dvy;
            let rx = (Fixed::cell_center(blocked.pos.x) - pixel_body.x).to_f32();
            let ry = (Fixed::cell_center(blocked.pos.y) - pixel_body.y).to_f32();
            pixel_body.vx = pixel_body.vx.add_f32(jx * pixel_body.inv_mass);
            pixel_body.vy = pixel_body.vy.add_f32(jy * pixel_body.inv_mass);
            pixel_body.spin += (rx * jy - ry * jx) * pixel_body.inv_inertia;
            pixel_body.rest_secs = 0.0;
            pixel_body.asleep = false;
        }
        if health.hp <= 0.0 {
            health.hp = MAX_HP;
            air.secs = MAX_AIR_SECS;
            burning.secs = 0.0;
            body.0 = Body::new(
                Fixed::from_cell(spawn_point.0.x),
                Fixed::from_cell(spawn_point.0.y),
                PLAYER_HALF_W,
                PLAYER_HALF_H,
            );
            control.0 = Controller::default();
        }
    }
    impulses.0.clear();
}

#[allow(clippy::type_complexity)]
pub fn replicate(
    mut sessions: ResMut<Sessions>,
    sim: Res<SimWorld>,
    mut stats: ResMut<TickStats>,
    query: Query<(
        &Player,
        &PhysicsBody,
        &Control,
        &Health,
        &Mode,
        &Burning,
        &Air,
    )>,
) {
    let entities: Vec<EntityState> = query
        .iter()
        .map(
            |(player, body, control, health, mode, burning, air)| EntityState {
                player: player.id,
                x: body.0.x,
                y: body.0.y,
                hp: health.hp,
                ducking: control.0.ducking(),
                mode: mode.0,
                burning: burning.active(),
                air: air.secs,
            },
        )
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
        let Some((_, body, ..)) = query.get(entity).ok() else {
            continue;
        };

        let center = CellPos::new(body.0.x.floor_cell(), body.0.y.floor_cell()).chunk();
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

        let mut debug_rects = Vec::new();
        for &pos in &interest {
            let chunk = sim.0.chunk(pos).expect("interest chunks are loaded");
            if session.debug {
                let change = chunk.dirty();
                let keep_alive = chunk.keep_dirty();
                if !change.is_empty() || !keep_alive.is_empty() {
                    debug_rects.push(ChunkDebugRects {
                        pos,
                        change,
                        keep_alive,
                    });
                }
            }
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

        if !debug_rects.is_empty() {
            let message = encode_message(&ServerMessage::DebugRects {
                chunks: debug_rects,
            });
            sent_bytes += message.len() as u64;
            session.conn.send(message);
        }

        sent_bytes += entity_message.len() as u64;
        session.conn.send(entity_message.clone());
    }

    stats.players = entities.len();
    stats.awake_chunks = sim.0.awake_chunk_count();
    stats.loaded_chunks = sim.0.chunks().count();
    stats.replicated_bytes = sent_bytes;
}

pub fn advance_clock(mut clock: ResMut<crate::WorldClock>) {
    clock.t += TICK_DT / crate::DAY_SECS;
    while clock.t >= 1.0 {
        clock.t -= 1.0;
        clock.day += 1;
    }
}

pub fn finish_tick(
    mut sessions: ResMut<Sessions>,
    sim: Res<SimWorld>,
    clock: Res<crate::WorldClock>,
) {
    let message = encode_message(&ServerMessage::TickEnd {
        tick: sim.0.tick(),
        time_of_day: clock.t,
        day: clock.day,
    });
    for session in &mut sessions.sessions {
        if matches!(session.state, SessionState::Playing) {
            session.conn.send(message.clone());
        }
    }
}
