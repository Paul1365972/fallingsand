mod auth;

use crate::command::Reply;
use crate::persistence::{Persistence, StoreError};
use crate::player::{Player, PlayerLife, Players};
use crate::replication::SessionReplication;
use fallingsand_core::HOTBAR_SLOTS;
use fallingsand_net::{Connection, ConnectionStatus, Listener};
use fallingsand_protocol::{
    ChatEntry, ClientMessage, GameMode, InputAction, InputState, MAX_INPUT_ACTIONS_PER_FRAME,
    PlayerId, ServerMessage, clamp_line, decode_message, encode_message, push_history,
};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

const CHAT_RATE_SECS: f32 = 0.25;
const CHAT_RATE_TICKS: u64 = fallingsand_core::ticks_from_secs(CHAT_RATE_SECS);
const INPUT_HOLD_SECS: f32 = 0.5;
const INPUT_HOLD_TICKS: u64 = fallingsand_core::ticks_from_secs(INPUT_HOLD_SECS);
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

    pub fn broadcast(&mut self, message: &ServerMessage) {
        for session in self.active_iter_mut() {
            session.send(message);
        }
    }

    pub fn deliver(&mut self, reply: Reply) {
        match reply {
            Reply::To(player, entry) => self.send_to_player(player, &ServerMessage::Chat(entry)),
            Reply::All(entry) => self.broadcast(&ServerMessage::Chat(entry)),
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
) -> Result<Vec<PlayerId>, StoreError> {
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
    let mut broadcast: Vec<ChatEntry> = Vec::new();
    let mut direct: Vec<(PlayerId, ChatEntry)> = Vec::new();

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
                    let handshake = auth::Handshake {
                        sessions,
                        players,
                        persistence,
                        spawn,
                        tick,
                        roster_upserts: &mut roster_upserts,
                        broadcast: &mut broadcast,
                    };
                    let hello = auth::Hello {
                        protocol_version,
                        uuid,
                        public_key,
                        signature,
                        name,
                    };
                    if !handshake.handle(id, hello)? {
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
                    let Some((player_id, player)) = accept(sessions, players, id) else {
                        continue;
                    };
                    let text = clamp_line(&text);
                    if text.is_empty() {
                        continue;
                    }
                    if throttled(&mut player.control.last_chat_tick, tick) {
                        direct.push((player_id, ChatEntry::error("sending too fast")));
                        continue;
                    }
                    push_history(&mut player.profile.history, &text);
                    broadcast.push(ChatEntry::say(player_id, player.name.clone(), text));
                }
                ClientMessage::Command { line } => {
                    let Some((player_id, player)) = accept(sessions, players, id) else {
                        continue;
                    };
                    let line = clamp_line(&line);
                    if line.is_empty() {
                        continue;
                    }
                    if throttled(&mut player.control.last_command_tick, tick) {
                        direct.push((player_id, ChatEntry::error("sending too fast")));
                        continue;
                    }
                    push_history(&mut player.profile.history, &format!("/{line}"));
                    player.control.pending_commands.push(line);
                }
                ClientMessage::SetDebug { enabled } => {
                    if sessions.active_player(id).is_some()
                        && let Some(session) = sessions.entries.get_mut(&id)
                    {
                        session.replication.debug = enabled;
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

    let roster_removes = remove_closed_sessions(sessions, players, &mut broadcast);

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
        for entry in &broadcast {
            session.send(&ServerMessage::Chat(entry.clone()));
        }
    }
    for (player, entry) in direct {
        sessions.send_to_player(player, &ServerMessage::Chat(entry));
    }
    Ok(roster_removes)
}

fn accept<'a>(
    sessions: &Sessions,
    players: &'a mut Players,
    id: SessionId,
) -> Option<(PlayerId, &'a mut Player)> {
    let player_id = sessions.active_player(id)?;
    Some((player_id, players.get_mut(player_id)?))
}

fn throttled(last: &mut u64, tick: u64) -> bool {
    if *last != 0 && tick.saturating_sub(*last) < CHAT_RATE_TICKS {
        return true;
    }
    *last = tick;
    false
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
        InputAction::Use { button, cell } => player.control.pending_uses.push((button, cell)),
    }
}

fn remove_closed_sessions(
    sessions: &mut Sessions,
    players: &Players,
    broadcast: &mut Vec<ChatEntry>,
) -> Vec<PlayerId> {
    let closed: Vec<_> = sessions
        .entries
        .iter()
        .filter_map(|(&id, session)| match session.conn.status() {
            ConnectionStatus::Closed { reason } => Some((id, reason)),
            ConnectionStatus::Connected => None,
        })
        .collect();
    let mut removed = Vec::new();
    for (id, reason) in closed {
        let player_id = sessions.entries.get(&id).and_then(Session::player);
        let current = player_id.is_some_and(|player| sessions.controls(id, player));
        if current {
            let player_id = player_id.expect("current session is active");
            let name = players
                .get(player_id)
                .map_or("<unknown>", |player| player.name.as_str());
            tracing::info!("{name} (player {}) left: {reason}", player_id.0);
            broadcast.push(ChatEntry::announce(format!("{name} left")));
            sessions.player_to_session.remove(&player_id);
            removed.push(player_id);
        } else {
            tracing::debug!("session {} closed: {reason}", id.0);
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
