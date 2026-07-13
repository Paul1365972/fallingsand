use crate::commands::{PendingCommand, PendingCommands};
use crate::inventory::{Inventory, ItemReg, QueuedSlotAction, SlotActions};
use crate::player::{
    Air, Burning, ChatHistory, Health, Life, Mode, Player, PlayerActor, PlayerRaster,
    player_record, spawn_player,
};
use crate::regions::Store;
use crate::{NetListener, SimWorld, SpawnPoint};
use bevy_ecs::prelude::*;
use ed25519_dalek::{Signature, VerifyingKey};
use fallingsand_core::{CellPos, ChunkPos, HOTBAR_SLOTS};
use fallingsand_net::Connection;
use fallingsand_protocol::{
    ClientMessage, GameMode, InputAction, InputState, LifeState, PROTOCOL_VERSION, PlayerId,
    PlayerUuid, SelfState, ServerMessage, decode_message, encode_message,
};
use rustc_hash::FxHashSet;

const CHAT_MAX_CHARS: usize = 240;
const CHAT_RATE_SECS: f32 = 0.25;
const CHAT_RATE_TICKS: u64 = fallingsand_core::ticks_from_secs(CHAT_RATE_SECS);
const INPUT_HOLD_SECS: f32 = 0.5;
const INPUT_HOLD_TICKS: u64 = fallingsand_core::ticks_from_secs(INPUT_HOLD_SECS);
const HISTORY_CAP: usize = 100;
const NAME_MAX_CHARS: usize = 24;
const HELLO_FRAME_LIMIT: usize = 512;

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
    pub nonce: [u8; 32],
}

impl Session {
    pub fn new(conn: Box<dyn Connection>) -> Self {
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce).expect("secure randomness unavailable");
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
            nonce,
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
    pub next_generation: u64,
}

type PlayerQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Player,
        &'static PlayerActor,
        &'static Health,
        &'static Life,
        &'static Mode,
        &'static Air,
        &'static Burning,
        &'static mut Inventory,
        &'static mut ChatHistory,
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

#[allow(clippy::too_many_arguments)]
fn take_over(
    commands: &mut Commands,
    players: &mut PlayerQuery,
    entity: Entity,
    id: PlayerId,
    name: &str,
    tick: u64,
    generation: u64,
    despawned: &mut Vec<Entity>,
) -> Option<(Entity, CellPos, u8, Vec<String>)> {
    let Ok((mut existing, body, _, _, _, _, _, _, history)) = players.get_mut(entity) else {
        despawned.push(entity);
        commands.entity(entity).despawn();
        return None;
    };
    existing.id = id;
    existing.name = name.to_string();
    existing.input = Default::default();
    existing.jump_pressed = false;
    existing.last_input_tick = tick;
    existing.session_generation = generation;
    existing.revive_requested = false;
    Some((
        entity,
        CellPos::new(body.0.x.floor_cell(), body.0.y.floor_cell()),
        existing.selected_slot,
        history.0.clone(),
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
    mut bodies: ResMut<crate::bodies::PixelBodies>,
    spawn_point: Res<SpawnPoint>,
    store: Res<Store>,
) {
    while let Some(conn) = listener.0.poll_accept() {
        let mut session = Session::new(conn);
        session.send(&ServerMessage::Challenge {
            nonce: session.nonce,
        });
        sessions.sessions.push(session);
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
            if matches!(sessions.sessions[index].state, SessionState::AwaitingHello)
                && bytes.len() > HELLO_FRAME_LIMIT
            {
                tracing::warn!("closing connection: oversized handshake frame");
                sessions.sessions[index]
                    .conn
                    .close("oversized handshake frame");
                break;
            }
            let Ok(message) = decode_message::<ClientMessage>(&bytes) else {
                tracing::warn!("closing connection: malformed message");
                sessions.sessions[index].conn.close("malformed message");
                break;
            };
            match message {
                ClientMessage::Hello {
                    protocol_version,
                    uuid,
                    public_key,
                    signature,
                    name,
                } => {
                    if !matches!(sessions.sessions[index].state, SessionState::AwaitingHello) {
                        continue;
                    }
                    let name = clamp_name(name);
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
                    let authenticated = authenticate_identity(
                        sessions.sessions[index].nonce,
                        uuid,
                        public_key,
                        &signature,
                    );
                    if !authenticated {
                        tracing::warn!("rejected unauthenticated identity for {name}");
                        let session = &mut sessions.sessions[index];
                        session.send(&ServerMessage::Reject {
                            reason: "identity authentication failed".into(),
                        });
                        session.conn.close("identity authentication failed");
                        continue;
                    }
                    let player = PlayerId(sessions.next_player);
                    sessions.next_player += 1;
                    sessions.next_generation = sessions.next_generation.wrapping_add(1).max(1);
                    let generation = sessions.next_generation;

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
                            generation,
                            &mut despawned,
                        )
                    });
                    let (entity, spawn, selected, history) = match takeover {
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
                            let selected = restored
                                .as_ref()
                                .map(|r| r.selected.min(HOTBAR_SLOTS as u8 - 1))
                                .unwrap_or(0);
                            let history = restored
                                .as_ref()
                                .map(|record| record.history.clone())
                                .unwrap_or_default();
                            let entity = spawn_player(
                                &mut commands,
                                &item_reg.0,
                                player,
                                uuid,
                                name.clone(),
                                tick,
                                generation,
                                restored.as_ref(),
                                spawn,
                            );
                            (entity, spawn, selected, history)
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
                        selected,
                    });
                    session.send(&ServerMessage::History { entries: history });
                    announce_join(session, &players, player, &name);
                    tracing::info!("{name} ({uuid}) joined as player {}", player.0);
                    joined.push((player, name));
                }
                ClientMessage::Input(frame) => {
                    if let Some(entity) = sessions.sessions[index].entity
                        && let Ok((mut player, _, _, life, mode, ..)) = players.get_mut(entity)
                    {
                        let generation = player.session_generation;
                        if fresh_input {
                            player.input = if life.0 == LifeState::Alive {
                                frame.state
                            } else {
                                InputState::default()
                            };
                            fresh_input = false;
                        } else {
                            player.input.merge_or(frame.state);
                        }
                        player.last_input_tick = tick;
                        for action in frame.actions {
                            if life.0 == LifeState::Dead {
                                if matches!(action, InputAction::Revive) {
                                    player.revive_requested = true;
                                }
                                continue;
                            }
                            match action {
                                InputAction::Jump => player.jump_pressed = true,
                                InputAction::Revive => {}
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
                                InputAction::Slot(action) => {
                                    slot_actions.0.push(QueuedSlotAction {
                                        entity,
                                        generation,
                                        action,
                                    });
                                }
                            }
                        }
                        if life.0 == LifeState::Dead {
                            player.input = InputState::default();
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
                    if let Ok((.., mut history)) = players.get_mut(entity)
                        && history.0.last() != Some(&text)
                    {
                        history.0.push(text.clone());
                        if history.0.len() > HISTORY_CAP {
                            let excess = history.0.len() - HISTORY_CAP;
                            history.0.drain(..excess);
                        }
                    }
                    if text.starts_with('/') {
                        let generation = players
                            .get(entity)
                            .map(|(player, ..)| player.session_generation)
                            .unwrap_or_default();
                        pending.0.push(PendingCommand {
                            entity,
                            generation,
                            text,
                        });
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
                cursor_mode: player.input.cursor_mode,
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
                if let Ok((player, body, health, life, mode, air, burning, inventory, history)) =
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
                                life,
                                mode,
                                air,
                                burning,
                                inventory,
                                history,
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
            crate::physics::unstamp_and_wake(&mut sim.0, &mut bodies, &mut raster.0);
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

fn authenticate_identity(
    nonce: [u8; 32],
    uuid: PlayerUuid,
    public_key: [u8; 32],
    signature: &[u8; 64],
) -> bool {
    if uuid != PlayerUuid::from_public_key(&public_key) {
        return false;
    }
    let Ok(key) = VerifyingKey::from_bytes(&public_key) else {
        return false;
    };
    let signature = Signature::from_bytes(signature);
    key.verify_strict(&fallingsand_protocol::identity_message(nonce), &signature)
        .is_ok()
}

fn clamp_name(name: String) -> String {
    name.trim().chars().take(NAME_MAX_CHARS).collect()
}
