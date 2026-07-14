use crate::persistence::{Persistence, restore_player};
use crate::player::{Player, PlayerLife, Players};
use crate::replication::SessionReplication;
use ed25519_dalek::{Signature, VerifyingKey};
use fallingsand_core::HOTBAR_SLOTS;
use fallingsand_net::{Connection, ConnectionStatus, Listener};
use fallingsand_protocol::{
    ClientMessage, GameMode, InputAction, InputState, MAX_INPUT_ACTIONS_PER_FRAME,
    PROTOCOL_VERSION, PlayerId, PlayerUuid, ServerMessage, decode_message, encode_message,
};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

const CHAT_MAX_CHARS: usize = 240;
const CHAT_RATE_SECS: f32 = 0.25;
const CHAT_RATE_TICKS: u64 = fallingsand_core::ticks_from_secs(CHAT_RATE_SECS);
const INPUT_HOLD_SECS: f32 = 0.5;
const INPUT_HOLD_TICKS: u64 = fallingsand_core::ticks_from_secs(INPUT_HOLD_SECS);
const HISTORY_CAP: usize = 100;
const NAME_MAX_CHARS: usize = 24;
const HELLO_FRAME_LIMIT: usize = 512;
const ACTIVE_FRAME_LIMIT: usize = 16 * 1024;
const MAX_MESSAGES_PER_TICK: usize = 64;
const MAX_SESSIONS: usize = 128;
const HANDSHAKE_TIMEOUT_TICKS: u64 = fallingsand_core::ticks_from_secs(10.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(pub u64);

pub enum SessionPhase {
    Challenged { nonce: [u8; 32], deadline_tick: u64 },
    Active { player: PlayerId },
}

pub struct Session {
    pub id: SessionId,
    pub conn: Box<dyn Connection>,
    pub phase: SessionPhase,
    pub replication: SessionReplication,
}

impl Session {
    fn new(id: SessionId, conn: Box<dyn Connection>, tick: u64) -> Self {
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce).expect("secure randomness unavailable");
        Self {
            id,
            conn,
            phase: SessionPhase::Challenged {
                nonce,
                deadline_tick: tick.saturating_add(HANDSHAKE_TIMEOUT_TICKS),
            },
            replication: SessionReplication::default(),
        }
    }

    pub fn player(&self) -> Option<PlayerId> {
        match self.phase {
            SessionPhase::Challenged { .. } => None,
            SessionPhase::Active { player } => Some(player),
        }
    }

    pub fn send(&mut self, message: &ServerMessage) {
        let bytes = encode_message(message);
        self.replication.sent_bytes += bytes.len() as u64;
        self.conn.send(bytes);
    }
}

#[derive(Default)]
pub struct Sessions {
    pub entries: BTreeMap<SessionId, Session>,
    player_to_session: FxHashMap<PlayerId, SessionId>,
    next_id: u64,
}

impl Sessions {
    fn allocate_id(&mut self) -> Option<SessionId> {
        let id = SessionId(self.next_id);
        self.next_id = self.next_id.checked_add(1)?;
        Some(id)
    }

    fn bind(&mut self, session: SessionId, player: PlayerId) -> Option<SessionId> {
        let old = self.player_to_session.insert(player, session);
        self.entries
            .get_mut(&session)
            .expect("session exists")
            .phase = SessionPhase::Active { player };
        old.filter(|old| *old != session)
    }

    pub fn controls(&self, session: SessionId, player: PlayerId) -> bool {
        self.player_to_session.get(&player) == Some(&session)
    }

    pub fn active_player(&self, session: SessionId) -> Option<PlayerId> {
        let player = self.entries.get(&session)?.player()?;
        self.controls(session, player).then_some(player)
    }

    pub fn send_to_player(&mut self, player: PlayerId, message: &ServerMessage) {
        let Some(session) = self.player_to_session.get(&player).copied() else {
            return;
        };
        if let Some(session) = self.entries.get_mut(&session) {
            session.send(message);
        }
    }

    pub fn active_iter_mut(&mut self) -> impl Iterator<Item = &mut Session> {
        let current = &self.player_to_session;
        self.entries.values_mut().filter(move |session| {
            session
                .player()
                .is_some_and(|player| current.get(&player) == Some(&session.id))
        })
    }
}

pub fn drain_network(
    listener: &mut dyn Listener,
    sessions: &mut Sessions,
    players: &mut Players,
    spawn: fallingsand_core::CellPos,
    tick: u64,
    persistence: &mut Persistence,
) -> Vec<PlayerId> {
    while let Some(mut conn) = listener.poll_accept() {
        if sessions.entries.len() >= MAX_SESSIONS {
            conn.close("server session limit reached");
            continue;
        }
        let Some(id) = sessions.allocate_id() else {
            conn.close("server session id space exhausted");
            continue;
        };
        let mut session = Session::new(id, conn, tick);
        let SessionPhase::Challenged { nonce, .. } = session.phase else {
            unreachable!();
        };
        session.send(&ServerMessage::Challenge { nonce });
        sessions.entries.insert(id, session);
    }

    let ids: Vec<_> = sessions.entries.keys().copied().collect();
    let mut roster_upserts = Vec::new();
    let mut chats = Vec::new();

    for id in ids {
        let messages = poll_messages(sessions, id);
        for message in messages {
            match message {
                ClientMessage::Hello {
                    protocol_version,
                    uuid,
                    public_key,
                    signature,
                    name,
                } => {
                    let name = clamp_name(name);
                    if !handle_hello(
                        sessions,
                        players,
                        persistence,
                        id,
                        protocol_version,
                        uuid,
                        public_key,
                        &signature,
                        name,
                        spawn,
                        tick,
                        &mut roster_upserts,
                    ) {
                        break;
                    }
                }
                ClientMessage::Input(frame) => {
                    if frame.actions.len() > MAX_INPUT_ACTIONS_PER_FRAME {
                        reject(sessions, id, "too many input actions in one frame".into());
                        break;
                    }
                    let Some(player_id) = sessions.active_player(id) else {
                        continue;
                    };
                    let Some(player) = players.get_mut(player_id) else {
                        continue;
                    };
                    player.control.input = if player.is_alive() {
                        frame.state
                    } else {
                        InputState::default()
                    };
                    player.control.last_input_tick = tick;
                    for action in frame.actions {
                        apply_input_action(player, action);
                    }
                }
                ClientMessage::Chat { text } => {
                    let Some(player_id) = sessions.active_player(id) else {
                        continue;
                    };
                    let Some(player) = players.get_mut(player_id) else {
                        continue;
                    };
                    if player.control.last_chat_tick != 0
                        && tick.saturating_sub(player.control.last_chat_tick) < CHAT_RATE_TICKS
                    {
                        continue;
                    }
                    let text: String = text.trim().chars().take(CHAT_MAX_CHARS).collect();
                    if text.is_empty() {
                        continue;
                    }
                    player.control.last_chat_tick = tick;
                    if player.profile.history.last() != Some(&text) {
                        player.profile.history.push(text.clone());
                        if player.profile.history.len() > HISTORY_CAP {
                            let excess = player.profile.history.len() - HISTORY_CAP;
                            player.profile.history.drain(..excess);
                        }
                    }
                    if text.starts_with('/') {
                        player.control.pending_commands.push(text);
                    } else {
                        chats.push((player_id, player.name.clone(), text));
                    }
                }
                ClientMessage::SetDebug { enabled } => {
                    if sessions.active_player(id).is_some()
                        && let Some(session) = sessions.entries.get_mut(&id)
                    {
                        session.replication.debug = enabled;
                    }
                }
                ClientMessage::Goodbye => {
                    if let Some(session) = sessions.entries.get_mut(&id) {
                        session.conn.close("client goodbye");
                    }
                }
            }
        }
    }

    for (_, player) in players.iter_mut() {
        if tick.saturating_sub(player.control.last_input_tick) > INPUT_HOLD_TICKS {
            player.control.input = InputState {
                aim: player.control.input.aim,
                cursor_mode: player.control.input.cursor_mode,
                ..Default::default()
            };
        }
    }

    for session in sessions.entries.values_mut() {
        if matches!(
            session.phase,
            SessionPhase::Challenged { deadline_tick, .. } if tick >= deadline_tick
        ) {
            session.conn.close("handshake timed out");
        }
    }

    let roster_removes = remove_closed_sessions(sessions);

    for session in sessions.active_iter_mut() {
        for (player, name) in &roster_upserts {
            session.send(&ServerMessage::RosterUpsert {
                player: *player,
                name: name.clone(),
            });
        }
        for player in &roster_removes {
            session.send(&ServerMessage::RosterRemove { player: *player });
        }
        for (player, name, text) in &chats {
            session.send(&ServerMessage::Chat {
                player: *player,
                name: name.clone(),
                text: text.clone(),
            });
        }
    }
    roster_removes
}

fn poll_messages(sessions: &mut Sessions, id: SessionId) -> Vec<ClientMessage> {
    let mut messages = Vec::new();
    let Some(session) = sessions.entries.get_mut(&id) else {
        return messages;
    };
    while messages.len() < MAX_MESSAGES_PER_TICK
        && let Some(bytes) = session.conn.poll()
    {
        let frame_limit = match session.phase {
            SessionPhase::Challenged { .. } => HELLO_FRAME_LIMIT,
            SessionPhase::Active { .. } => ACTIVE_FRAME_LIMIT,
        };
        if bytes.len() > frame_limit {
            tracing::warn!("closing connection: oversized client frame");
            session.conn.close("oversized client frame");
            break;
        }
        match decode_message::<ClientMessage>(&bytes) {
            Ok(message) => messages.push(message),
            Err(_) => {
                tracing::warn!("closing connection: malformed message");
                session.conn.close("malformed message");
                break;
            }
        }
    }
    messages
}

#[allow(clippy::too_many_arguments)]
fn handle_hello(
    sessions: &mut Sessions,
    players: &mut Players,
    persistence: &mut Persistence,
    session_id: SessionId,
    protocol_version: u16,
    uuid: PlayerUuid,
    public_key: [u8; 32],
    signature: &[u8; 64],
    name: String,
    spawn: fallingsand_core::CellPos,
    tick: u64,
    roster_upserts: &mut Vec<(PlayerId, String)>,
) -> bool {
    let Some(session) = sessions.entries.get(&session_id) else {
        return false;
    };
    let SessionPhase::Challenged { nonce, .. } = session.phase else {
        return true;
    };
    if protocol_version != PROTOCOL_VERSION {
        reject(
            sessions,
            session_id,
            format!(
                "protocol version mismatch: server {PROTOCOL_VERSION}, client {protocol_version}"
            ),
        );
        return false;
    }
    if !authenticate_identity(nonce, uuid, public_key, signature) {
        tracing::warn!("rejected unauthenticated identity for {name}");
        reject(
            sessions,
            session_id,
            "identity authentication failed".into(),
        );
        return false;
    }

    let (player_id, joined, renamed) = match players.id_for_uuid(uuid) {
        Some(player_id) => {
            let player = players.get_mut(player_id).expect("uuid index is valid");
            let renamed = player.name != name;
            player.name = name.clone();
            player.control.reset_transient(tick);
            (player_id, false, renamed)
        }
        None => {
            let Some(player_id) = players.allocate_id() else {
                reject(
                    sessions,
                    session_id,
                    "server player id space exhausted".into(),
                );
                return false;
            };
            let restored = match persistence.load_player(uuid) {
                Ok(record) => record,
                Err(err) => {
                    tracing::error!("failed to load player {uuid}: {err}");
                    reject(
                        sessions,
                        session_id,
                        "player data could not be loaded".into(),
                    );
                    return false;
                }
            };
            let restored = restored.map(restore_player);
            players.insert(Player::new(
                player_id,
                uuid,
                name.clone(),
                tick,
                restored,
                spawn,
            ));
            (player_id, true, false)
        }
    };

    if let Some(old) = sessions.bind(session_id, player_id)
        && let Some(old) = sessions.entries.get_mut(&old)
    {
        old.send(&ServerMessage::Reject {
            reason: "superseded by a new session".into(),
        });
        old.conn.close("superseded by a new session");
    }

    let player = players.get(player_id).expect("player inserted");
    let ack = ServerMessage::HelloAck {
        protocol_version: PROTOCOL_VERSION,
        player: player_id,
        selected: player.profile.selected_slot,
    };
    let history = ServerMessage::History {
        entries: player.profile.history.clone(),
    };
    let roster: Vec<_> = players
        .iter()
        .map(|(&id, player)| (id, player.name.clone()))
        .collect();
    if let Some(session) = sessions.entries.get_mut(&session_id) {
        session.send(&ack);
        session.send(&history);
        for (id, name) in roster {
            session.send(&ServerMessage::RosterUpsert { player: id, name });
        }
    }
    if joined || renamed {
        roster_upserts.push((player_id, name.clone()));
    }
    tracing::info!("{name} ({uuid}) joined as player {}", player_id.0);
    true
}

fn apply_input_action(player: &mut Player, action: InputAction) {
    if !player.is_alive() {
        if matches!(player.life, PlayerLife::Dead(_)) && matches!(action, InputAction::Revive) {
            player.control.revive_requested = true;
        }
        return;
    }
    match action {
        InputAction::Jump => player.control.jump_pressed = true,
        InputAction::Revive => {}
        InputAction::ToggleFlight => {
            if player.profile.mode == GameMode::Creative
                && let Some(avatar) = player.avatar_mut()
            {
                avatar.flying = !avatar.flying;
            }
        }
        InputAction::SelectSlot(slot) => {
            if (slot as usize) < HOTBAR_SLOTS && player.profile.selected_slot != slot {
                player.profile.selected_slot = slot;
            }
        }
        InputAction::Slot(action) => player.control.pending_slot_actions.push(action),
    }
}

fn remove_closed_sessions(sessions: &mut Sessions) -> Vec<PlayerId> {
    let closed: Vec<_> = sessions
        .entries
        .iter()
        .filter_map(|(&id, session)| {
            matches!(session.conn.status(), ConnectionStatus::Closed { .. }).then_some(id)
        })
        .collect();
    let mut removed = Vec::new();
    for id in closed {
        let player_id = sessions.entries.get(&id).and_then(Session::player);
        let current = player_id.is_some_and(|player| sessions.controls(id, player));
        if current {
            let player_id = player_id.expect("current session is active");
            sessions.player_to_session.remove(&player_id);
            removed.push(player_id);
        }
        sessions.entries.remove(&id);
    }
    removed
}

fn reject(sessions: &mut Sessions, id: SessionId, reason: String) {
    if let Some(session) = sessions.entries.get_mut(&id) {
        session.send(&ServerMessage::Reject {
            reason: reason.clone(),
        });
        session.conn.close(&reason);
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
