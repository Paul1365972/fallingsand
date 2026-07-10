use crate::commands::{PendingCommand, PendingCommands};
use crate::inventory::{Inventory, ItemReg, SlotActions};
use crate::player::{
    Air, Burning, Health, Mode, Player, PlayerActor, PlayerRaster, player_record, spawn_player,
};
use crate::regions::Store;
use crate::{NetListener, SimWorld, SpawnPoint};
use bevy_ecs::prelude::*;
use fallingsand_core::{BRUSH_RADIUS, CellPos, ChunkPos, HOTBAR_SLOTS, MAX_BRUSH};
use fallingsand_net::Connection;
use fallingsand_protocol::{
    ClientMessage, GameMode, InputAction, InputState, PROTOCOL_VERSION, PlayerId, PlayerUuid,
    SelfState, ServerMessage, decode_message, encode_message,
};
use rustc_hash::FxHashSet;

const CHAT_MAX_CHARS: usize = 240;
const CHAT_RATE_SECS: f32 = 0.25;
const CHAT_RATE_TICKS: u64 = fallingsand_core::ticks_from_secs(CHAT_RATE_SECS);
const INPUT_HOLD_SECS: f32 = 0.5;
const INPUT_HOLD_TICKS: u64 = fallingsand_core::ticks_from_secs(INPUT_HOLD_SECS);

pub enum SessionState {
    AwaitingHello,
    Playing,
}

pub struct Session {
    pub conn: Box<dyn Connection>,
    pub state: SessionState,
    pub entity: Option<Entity>,
    pub player: Option<PlayerId>,
    pub uuid: Option<PlayerUuid>,
    pub known_chunks: FxHashSet<ChunkPos>,
    pub last_self: Option<SelfState>,
    pub fresh: bool,
    pub sent_bytes: u64,
    pub last_chat_tick: u64,
    pub debug: bool,
}

impl Session {
    pub fn new(conn: Box<dyn Connection>) -> Self {
        Self {
            conn,
            state: SessionState::AwaitingHello,
            entity: None,
            player: None,
            uuid: None,
            known_chunks: FxHashSet::default(),
            last_self: None,
            fresh: true,
            sent_bytes: 0,
            last_chat_tick: 0,
            debug: false,
        }
    }

    pub fn send(&mut self, message: &ServerMessage) {
        let bytes = encode_message(message);
        self.sent_bytes += bytes.len() as u64;
        self.conn.send(bytes);
    }
}

#[derive(Resource, Default)]
pub struct Sessions {
    pub sessions: Vec<Session>,
    pub next_player: u32,
}

type PlayerQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Player,
        &'static PlayerActor,
        &'static Health,
        &'static Mode,
        &'static Air,
        &'static Burning,
        &'static mut Inventory,
    ),
>;

fn supersede_sessions(
    sessions: &mut [Session],
    uuid: PlayerUuid,
    commands: &mut Commands,
    despawned: &mut Vec<Entity>,
    left: &mut Vec<PlayerId>,
) -> Option<Entity> {
    let mut taken_entity = None;
    for other in sessions.iter_mut() {
        if other.uuid == Some(uuid) {
            other.send(&ServerMessage::Reject {
                reason: "superseded by a new session".into(),
            });
            other.conn.close("superseded by a new session");
            other.uuid = None;
            if let Some(entity) = other.entity.take()
                && let Some(superseded) = taken_entity.replace(entity)
            {
                despawned.push(superseded);
                commands.entity(superseded).despawn();
            }
            if let Some(old) = other.player.take() {
                left.push(old);
            }
        }
    }
    taken_entity
}

fn take_over(
    commands: &mut Commands,
    players: &mut PlayerQuery,
    entity: Entity,
    id: PlayerId,
    name: &str,
    tick: u64,
    despawned: &mut Vec<Entity>,
) -> Option<(Entity, CellPos)> {
    let Ok((mut existing, body, ..)) = players.get_mut(entity) else {
        despawned.push(entity);
        commands.entity(entity).despawn();
        return None;
    };
    existing.id = id;
    existing.name = name.to_string();
    existing.input = Default::default();
    existing.jump_pressed = false;
    existing.selected_slot = 0;
    existing.brush_radius = BRUSH_RADIUS;
    existing.last_input_tick = tick;
    Some((
        entity,
        CellPos::new(body.0.x.floor_cell(), body.0.y.floor_cell()),
    ))
}

fn announce_join(session: &mut Session, players: &PlayerQuery, player: PlayerId, name: &str) {
    session.send(&ServerMessage::PlayerJoined {
        player,
        name: name.to_string(),
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
}

#[allow(clippy::too_many_arguments)]
pub fn drain_network(
    mut commands: Commands,
    mut listener: ResMut<NetListener>,
    mut sessions: ResMut<Sessions>,
    mut pending: ResMut<PendingCommands>,
    mut slot_actions: ResMut<SlotActions>,
    mut players: PlayerQuery,
    mut rasters: Query<&mut PlayerRaster>,
    item_reg: Res<ItemReg>,
    mut sim: ResMut<SimWorld>,
    spawn_point: Res<SpawnPoint>,
    store: Res<Store>,
) {
    while let Some(conn) = listener.0.poll_accept() {
        sessions.sessions.push(Session::new(conn));
    }

    let sessions = &mut *sessions;
    let tick = sim.0.tick();
    let mut joined: Vec<(PlayerId, String)> = Vec::new();
    let mut left: Vec<PlayerId> = Vec::new();
    let mut chats: Vec<(PlayerId, String, String)> = Vec::new();
    let mut despawned: Vec<Entity> = Vec::new();

    for index in 0..sessions.sessions.len() {
        let mut fresh_input = true;
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

                    let taken_entity = supersede_sessions(
                        &mut sessions.sessions,
                        uuid,
                        &mut commands,
                        &mut despawned,
                        &mut left,
                    );
                    let takeover = taken_entity.and_then(|entity| {
                        take_over(
                            &mut commands,
                            &mut players,
                            entity,
                            player,
                            &name,
                            tick,
                            &mut despawned,
                        )
                    });
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
                            let entity = spawn_player(
                                &mut commands,
                                &item_reg.0,
                                player,
                                uuid,
                                name.clone(),
                                tick,
                                restored.as_ref(),
                                spawn,
                            );
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
                    announce_join(session, &players, player, &name);
                    tracing::info!("{name} ({uuid}) joined as player {}", player.0);
                    joined.push((player, name));
                }
                ClientMessage::Input(frame) => {
                    if let Some(entity) = sessions.sessions[index].entity
                        && let Ok((mut player, _, _, mode, ..)) = players.get_mut(entity)
                    {
                        if fresh_input {
                            player.input = frame.state;
                            fresh_input = false;
                        } else {
                            player.input.merge_or(frame.state);
                        }
                        player.last_input_tick = tick;
                        for action in frame.actions {
                            match action {
                                InputAction::Jump => player.jump_pressed = true,
                                InputAction::ToggleFlight => {
                                    if mode.0 == GameMode::Creative {
                                        player.flying = !player.flying;
                                    }
                                }
                                InputAction::SelectSlot(slot) => {
                                    if (slot as usize) < HOTBAR_SLOTS {
                                        player.selected_slot = slot;
                                    }
                                }
                                InputAction::SetBrush(radius) => {
                                    player.brush_radius = radius.min(MAX_BRUSH);
                                }
                                InputAction::Slot(action) => {
                                    slot_actions.0.push((entity, action));
                                }
                            }
                        }
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

    for (mut player, ..) in &mut players {
        if tick.saturating_sub(player.last_input_tick) > INPUT_HOLD_TICKS {
            player.input = InputState {
                aim: player.input.aim,
                ..Default::default()
            };
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
                                player,
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
                despawned.push(entity);
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
    for entity in despawned {
        if let Ok(mut raster) = rasters.get_mut(entity) {
            fallingsand_sim::player::unstamp_player(&mut sim.0, &mut raster.0);
        }
    }
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
