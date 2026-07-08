use crate::commands::{PendingCommand, PendingCommands};
use crate::inventory::{
    DroppedItem, Inventory, ItemActor, ItemReg, NextEntityId, SlotActions, spawn_dropped_item,
};
use crate::persistence::{
    PlayerRecord, player_slots_from_record, slots_to_record, stack_to_record,
};
use crate::regions::Store;
use crate::session::{Player, Session, SessionState, Sessions};
use crate::{
    INTEREST_RADIUS_X, INTEREST_RADIUS_Y, MAX_AIR_SECS, NetListener, Registry, SimObstacles,
    SimWorld, SpawnPoint, TickStats,
};
use bevy_ecs::prelude::*;
use fallingsand_core::{
    CellOffset, CellPos, Fixed, ItemId, ItemRegistry, ItemStack, MaterialId, MaterialRegistry,
    Phase, TICK_DT,
};
use fallingsand_protocol::{
    ChunkDebugRects, ChunkOp, ClientMessage, EntityId, GameMode, ItemDelta, ItemEntityState,
    ItemMove, MAX_BRUSH, PROTOCOL_VERSION, PlayerId, PlayerState, SelfState, ServerMessage,
    TickFrame, cells_to_wire, decode_message,
};
use fallingsand_sim::ActorAabb;
use fallingsand_sim::physics::{
    Actor, BOUNCE_MIN_SPEED, Controller, PlayerParams, StepInput, scatter_powder, step_player,
};
use rustc_hash::FxHashSet;
use std::time::Instant;

pub const PLAYER_HALF_W: Fixed = Fixed::from_f32(1.9);
pub const PLAYER_HALF_H: Fixed = Fixed::from_f32(5.5);
pub const PLAYER_MASS: f32 = 4.0 * PLAYER_HALF_W.to_f32() * PLAYER_HALF_H.to_f32();
pub use crate::MAX_HP;
pub use fallingsand_core::{BRUSH_RADIUS, REACH, SURVIVAL_REACH};
const CHAT_MAX_CHARS: usize = 240;
const CHAT_RATE_SECS: f32 = 0.25;
const CHAT_RATE_TICKS: u64 = fallingsand_core::ticks_from_secs(CHAT_RATE_SECS);
const PEAK_SIM_WINDOW_TICKS: u64 = 2 * crate::TICK_RATE as u64;

#[derive(Component)]
pub struct PlayerActor(pub Actor);

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

pub fn player_record(
    item_reg: &ItemRegistry,
    body: &Actor,
    health: &Health,
    mode: &Mode,
    air: &Air,
    burning: &Burning,
    inventory: &Inventory,
) -> PlayerRecord {
    PlayerRecord {
        x: body.x,
        y: body.y + (PLAYER_HALF_H - body.half_h),
        hp: health.hp,
        mode: mode.0,
        air: air.secs,
        burning: burning.secs,
        inventory: slots_to_record(item_reg, &inventory.inner),
        cursor: stack_to_record(item_reg, inventory.cursor),
    }
}

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub fn drain_network(
    mut commands: Commands,
    mut listener: ResMut<NetListener>,
    mut sessions: ResMut<Sessions>,
    mut pending: ResMut<PendingCommands>,
    mut slot_actions: ResMut<SlotActions>,
    mut players: Query<(
        &mut Player,
        &PlayerActor,
        &Health,
        &Mode,
        &Air,
        &Burning,
        &mut Inventory,
    )>,
    item_reg: Res<ItemReg>,
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
                        session.send(&ServerMessage::Reject {
                            reason: format!(
                                "protocol version mismatch: server {PROTOCOL_VERSION}, client {protocol_version}"
                            ),
                        });
                        session.conn.close("protocol version mismatch");
                        continue;
                    }
                    let player = PlayerId(sessions.next_player);
                    sessions.next_player += 1;

                    let mut taken_entity = None;
                    for other in &mut sessions.sessions {
                        if other.uuid == Some(uuid) {
                            other.send(&ServerMessage::Reject {
                                reason: "superseded by a new session".into(),
                            });
                            other.conn.close("superseded by a new session");
                            other.uuid = None;
                            if let Some(entity) = other.entity.take()
                                && let Some(superseded) = taken_entity.replace(entity)
                            {
                                commands.entity(superseded).despawn();
                            }
                            if let Some(old) = other.player.take() {
                                left.push(old);
                            }
                        }
                    }

                    let mut takeover = None;
                    if let Some(entity) = taken_entity {
                        if let Ok((mut existing, body, _, _, _, _, _)) = players.get_mut(entity) {
                            existing.id = player;
                            existing.name = name.clone();
                            existing.input = Default::default();
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
                                    PlayerActor(Actor::new(
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
                                    Inventory::with(
                                        player_slots_from_record(
                                            &item_reg.0,
                                            record.map(|r| r.inventory.as_slice()).unwrap_or(&[]),
                                        ),
                                        record.and_then(|r| {
                                            crate::persistence::stack_from_record(
                                                &item_reg.0,
                                                &r.cursor,
                                            )
                                        }),
                                    ),
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
                    session.send(&ServerMessage::HelloAck {
                        protocol_version: PROTOCOL_VERSION,
                        player,
                        spawn,
                    });
                    session.send(&ServerMessage::PlayerJoined {
                        player,
                        name: name.clone(),
                    });
                    for (existing, ..) in players.iter() {
                        if existing.id == player {
                            continue;
                        }
                        session.send(&ServerMessage::PlayerJoined {
                            player: existing.id,
                            name: existing.name.clone(),
                        });
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
                ClientMessage::Slot(action) => {
                    if matches!(sessions.sessions[index].state, SessionState::Playing)
                        && let Some(entity) = sessions.sessions[index].entity
                    {
                        slot_actions.0.push((entity, action));
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
                                &item_reg.0,
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
                session.send(&ServerMessage::PlayerJoined {
                    player: *player,
                    name: name.clone(),
                });
            }
        }
        for player in &left {
            session.send(&ServerMessage::PlayerLeft { player: *player });
        }
        for (player, name, text) in &chats {
            session.send(&ServerMessage::Chat {
                player: *player,
                name: name.clone(),
                text: text.clone(),
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn apply_player_inputs(
    mut commands: Commands,
    mut sim: ResMut<SimWorld>,
    registry: Res<Registry>,
    item_reg: Res<ItemReg>,
    obstacles: Res<SimObstacles>,
    mut bodies: ResMut<crate::bodies::PixelBodies>,
    mut next_id: ResMut<NextEntityId>,
    mut query: Query<(&Player, &PlayerActor, &Mode, &mut DigState, &mut Inventory)>,
) {
    let reg = &item_reg.0;
    for (player, body, mode, mut dig, mut inventory) in &mut query {
        let input = &player.input;
        let survival = mode.0 == GameMode::Survival;
        let radius = (input.brush_radius as i32).clamp(0, MAX_BRUSH as i32);
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
                let mut drops = Vec::new();
                dug = survival_dig(
                    &mut sim.0,
                    &registry.0,
                    reg,
                    &mut dig,
                    &mut inventory,
                    input.aim,
                    radius,
                    &mut drops,
                );
                for (pos, stack) in drops {
                    spawn_dropped_item(
                        &mut commands,
                        &mut next_id,
                        stack,
                        Fixed::cell_center(pos.x),
                        Fixed::cell_center(pos.y),
                        0.0,
                        40.0,
                        0,
                        0,
                    );
                }
            } else {
                for (_, pos) in brush_cells(input.aim, radius) {
                    let Some(cell) = sim.0.get_cell(pos) else {
                        continue;
                    };
                    if registry.0.get(cell.material).phase != Phase::Empty {
                        sim.0.place_material(pos, MaterialId::AIR);
                        dug = true;
                    }
                }
            }
        } else if input.secondary {
            let slot = input.selected_slot as usize;
            if slot < fallingsand_core::HOTBAR_SLOTS
                && let Some(stack) = inventory.inner.get(slot)
                && let Some(material) = reg.try_get(stack.item).and_then(|def| def.place)
            {
                let mut placed = 0u32;
                let budget = if survival { stack.count } else { u32::MAX };
                for (_, pos) in brush_cells(input.aim, radius) {
                    if placed >= budget {
                        break;
                    }
                    let Some(cell) = sim.0.get_cell(pos) else {
                        continue;
                    };
                    if !cell.is_air()
                        || cell_overlaps_body(pos, &body.0)
                        || obstacles.0.occupied(pos)
                    {
                        continue;
                    }
                    sim.0.place_material(pos, material);
                    placed += 1;
                }
                if survival && placed > 0 {
                    consume_slot(&mut inventory, slot, placed);
                }
            }
        }
        if dug {
            let ring = radius + 1;
            for oy in -ring..=ring {
                for ox in -ring..=ring {
                    let dist_sq = ox * ox + oy * oy;
                    if dist_sq <= radius * radius || dist_sq > ring * ring {
                        continue;
                    }
                    bodies.candidates.push(input.aim.translated(ox, oy));
                }
            }
        }
    }
}

fn consume_slot(inventory: &mut Inventory, slot: usize, amount: u32) {
    if let Some(stack) = inventory.inner.get(slot) {
        let count = stack.count.saturating_sub(amount);
        inventory
            .inner
            .set(slot, Some(ItemStack::new(stack.item, count)));
    }
    inventory.dirty = true;
}

fn brush_cells(aim: CellPos, radius: i32) -> impl Iterator<Item = (i32, CellPos)> {
    (-radius..=radius).flat_map(move |oy| {
        (-radius..=radius).filter_map(move |ox| {
            let dist_sq = ox * ox + oy * oy;
            (dist_sq <= radius * radius).then_some((dist_sq, aim.translated(ox, oy)))
        })
    })
}

#[allow(clippy::too_many_arguments)]
pub fn survival_dig(
    world: &mut fallingsand_sim::CellWorld,
    registry: &MaterialRegistry,
    item_reg: &ItemRegistry,
    dig: &mut DigState,
    inventory: &mut Inventory,
    aim: CellPos,
    radius: i32,
    drops: &mut Vec<(CellPos, ItemStack)>,
) -> bool {
    let mut candidates: Vec<(i32, i32, i32)> = brush_cells(aim, radius)
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
        let item = item_reg.item_for_material(cell.material);
        if item != ItemId::NONE {
            if let Some(overflow) = inventory
                .inner
                .insert_first_fit(ItemStack::new(item, 1), item_reg)
            {
                drops.push((pos, overflow));
            }
            inventory.dirty = true;
        }
        dug = true;
    }
    dug
}

fn cell_overlaps_body(pos: CellPos, body: &Actor) -> bool {
    let cx = Fixed::cell_center(pos.x);
    let cy = Fixed::cell_center(pos.y);
    (cx - body.x).abs() < body.half_w + Fixed::HALF
        && (cy - body.y).abs() < body.half_h + Fixed::HALF
}

pub fn build_obstacles(
    mut sim: ResMut<SimWorld>,
    registry: Res<Registry>,
    mut obstacles: ResMut<SimObstacles>,
    query: Query<&PlayerActor>,
) {
    let boxes: Vec<ActorAabb> = query
        .iter()
        .map(|body| ActorAabb {
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
    if stats.tick.is_multiple_of(PEAK_SIM_WINDOW_TICKS) {
        stats.peak_sim_micros = stats.sim_micros;
    } else {
        stats.peak_sim_micros = stats.peak_sim_micros.max(stats.sim_micros);
    }
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
        &mut PlayerActor,
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
            body.0 = Actor::new(
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

const PUSH_RESTITUTION: f32 = 0.2;

pub fn push_players(mut query: Query<&mut PlayerActor>) {
    let mut combos = query.iter_combinations_mut::<2>();
    while let Some([mut a, mut b]) = combos.fetch_next() {
        let dx = b.0.x - a.0.x;
        let dy = b.0.y - a.0.y;
        let ox = (a.0.half_w + b.0.half_w) - dx.abs();
        let oy = (a.0.half_h + b.0.half_h) - dy.abs();
        if ox <= Fixed::ZERO || oy <= Fixed::ZERO {
            continue;
        }
        if ox < oy {
            let push = ox.mul(Fixed::HALF);
            let n = if dx >= Fixed::ZERO { 1.0 } else { -1.0 };
            if dx >= Fixed::ZERO {
                a.0.x -= push;
                b.0.x += push;
            } else {
                a.0.x += push;
                b.0.x -= push;
            }
            let rel = (b.0.vx - a.0.vx).to_f32();
            if rel * n < 0.0 {
                let e = if rel.abs() > BOUNCE_MIN_SPEED {
                    PUSH_RESTITUTION
                } else {
                    0.0
                };
                let delta = (1.0 + e) * rel * 0.5;
                b.0.vx = b.0.vx.add_f32(-delta);
                a.0.vx = a.0.vx.add_f32(delta);
            }
        } else {
            let push = oy.mul(Fixed::HALF);
            let n = if dy >= Fixed::ZERO { 1.0 } else { -1.0 };
            if dy >= Fixed::ZERO {
                a.0.y -= push;
                b.0.y += push;
            } else {
                a.0.y += push;
                b.0.y -= push;
            }
            let rel = (b.0.vy - a.0.vy).to_f32();
            if rel * n < 0.0 {
                let e = if rel.abs() > BOUNCE_MIN_SPEED {
                    PUSH_RESTITUTION
                } else {
                    0.0
                };
                let delta = (1.0 + e) * rel * 0.5;
                b.0.vy = b.0.vy.add_f32(-delta);
                a.0.vy = a.0.vy.add_f32(delta);
            }
        }
    }
}

#[derive(Resource, Default)]
pub struct LastPlayers(pub rustc_hash::FxHashMap<PlayerId, PlayerState>);

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
pub fn replicate(
    mut sessions: ResMut<Sessions>,
    sim: Res<SimWorld>,
    clock: Res<crate::WorldClock>,
    regions: Res<crate::regions::RegionMap>,
    mut last_players: ResMut<LastPlayers>,
    mut stats: ResMut<TickStats>,
    query: Query<(
        &Player,
        &PlayerActor,
        &Control,
        &Health,
        &Mode,
        &Burning,
        &Air,
    )>,
    mut inventories: Query<&mut Inventory>,
    dropped: Query<(&DroppedItem, &ItemActor)>,
) {
    let tick = sim.0.tick();
    let world_age = clock.0.age;

    let mut all_players: Vec<PlayerState> = query
        .iter()
        .map(|(player, body, control, _, _, burning, _)| PlayerState {
            player: player.id,
            x: body.0.x,
            y: body.0.y,
            ducking: control.0.ducking(),
            burning: burning.active(),
        })
        .collect();
    all_players.sort_unstable_by_key(|state| state.player.0);
    let changed_players: Vec<PlayerState> = all_players
        .iter()
        .filter(|state| last_players.0.get(&state.player) != Some(*state))
        .copied()
        .collect();
    last_players.0 = all_players
        .iter()
        .map(|state| (state.player, *state))
        .collect();

    let item_views: Vec<(EntityId, fallingsand_core::ChunkPos, ItemEntityState, bool)> = dropped
        .iter()
        .map(|(item, body)| {
            (
                item.id,
                body.0.cell().chunk(),
                ItemEntityState {
                    id: item.id,
                    x: body.0.x,
                    y: body.0.y,
                    stack: item.stack,
                },
                item.moved,
            )
        })
        .collect();

    for session in &mut sessions.sessions {
        if !matches!(session.state, SessionState::Playing) {
            continue;
        }
        let Some(entity) = session.entity else {
            continue;
        };
        let Ok((_, body, _, health, mode, _, air)) = query.get(entity) else {
            continue;
        };

        let center = body.0.cell().chunk();
        let mut interest = FxHashSet::default();
        for dy in -INTEREST_RADIUS_Y..=INTEREST_RADIUS_Y {
            for dx in -INTEREST_RADIUS_X..=INTEREST_RADIUS_X {
                let pos = center.translated(dx, dy);
                if sim.0.chunk(pos).is_some() {
                    interest.insert(pos);
                }
            }
        }

        let mut debug = Vec::new();
        let chunks = build_tiles(
            &mut session.known_chunks,
            session.debug,
            &sim.0,
            &interest,
            &mut debug,
        );
        let players = if session.fresh {
            all_players.clone()
        } else {
            changed_players.clone()
        };
        let items = build_items(&mut session.known_items, &interest, &item_views);
        let (inventory, cursor) = match inventories.get_mut(entity) {
            Ok(mut inv) => inv.delta(session.fresh),
            Err(_) => (Vec::new(), None),
        };
        let current_self = SelfState {
            hp: health.hp,
            air: air.secs,
            mode: mode.0,
        };
        let self_state = if session.last_self != Some(current_self) {
            session.last_self = Some(current_self);
            Some(current_self)
        } else {
            None
        };

        session.fresh = false;
        session.send(&ServerMessage::TickFrame(TickFrame {
            tick,
            world_age,
            chunks,
            players,
            items,
            inventory,
            cursor,
            self_state,
            debug,
        }));
    }

    stats.players = all_players.len();
    (stats.awake_chunks, stats.awake_cells) = sim.0.awake_counts();
    stats.loaded_chunks = sim.0.chunks().count();
    (stats.loaded_regions, stats.dirty_regions) = regions.counts();
    stats.replicated_bytes = sessions.sessions.iter().map(|s| s.sent_bytes).sum();
    for session in &mut sessions.sessions {
        session.sent_bytes = 0;
    }
}

fn build_tiles(
    known: &mut FxHashSet<fallingsand_core::ChunkPos>,
    debug: bool,
    sim: &fallingsand_sim::CellWorld,
    interest: &FxHashSet<fallingsand_core::ChunkPos>,
    debug_rects: &mut Vec<ChunkDebugRects>,
) -> Vec<ChunkOp> {
    let mut ops = Vec::new();
    known.retain(|&pos| {
        if interest.contains(&pos) {
            return true;
        }
        ops.push(ChunkOp::Unload { pos });
        false
    });
    for &pos in interest {
        let chunk = sim.chunk(pos).expect("interest chunks are loaded");
        if debug {
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
            ops.push(ChunkOp::Load {
                pos,
                cells: cells_to_wire(chunk.cells()),
            });
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
        ops.push(ChunkOp::Delta {
            pos,
            rect,
            cells: cells_to_wire(&cells),
        });
    }
    ops
}

fn build_items(
    known: &mut FxHashSet<EntityId>,
    interest: &FxHashSet<fallingsand_core::ChunkPos>,
    views: &[(EntityId, fallingsand_core::ChunkPos, ItemEntityState, bool)],
) -> ItemDelta {
    let mut delta = ItemDelta::default();
    let mut visible: FxHashSet<EntityId> = FxHashSet::default();
    for (id, chunk, state, has_moved) in views {
        if !interest.contains(chunk) {
            continue;
        }
        visible.insert(*id);
        if known.contains(id) {
            if *has_moved {
                delta.moved.push(ItemMove {
                    id: *id,
                    x: state.x,
                    y: state.y,
                });
            }
        } else {
            delta.spawned.push(*state);
        }
    }
    delta.despawned = known
        .iter()
        .filter(|id| !visible.contains(id))
        .copied()
        .collect();
    *known = visible;
    delta
}

pub fn advance_clock(mut clock: ResMut<crate::WorldClock>) {
    clock.0.advance();
}
