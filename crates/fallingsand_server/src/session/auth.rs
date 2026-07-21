use super::{NAME_MAX_CHARS, SessionId, SessionPhase, Sessions, reject};
use crate::persistence::{Persistence, StoreError};
use crate::player::{Player, Players};
use ed25519_dalek::{Signature, VerifyingKey};
use fallingsand_protocol::{PROTOCOL_VERSION, PlayerId, PlayerUuid, ServerMessage};

pub(super) struct Hello {
    pub protocol_version: u16,
    pub uuid: PlayerUuid,
    pub public_key: [u8; 32],
    pub signature: [u8; 64],
    pub name: String,
}

pub(super) struct Handshake<'a> {
    pub sessions: &'a mut Sessions,
    pub players: &'a mut Players,
    pub persistence: &'a mut Persistence,
    pub spawn: fallingsand_core::CellPos,
    pub tick: u64,
    pub roster_upserts: &'a mut Vec<(PlayerId, String)>,
}

impl Handshake<'_> {
    pub fn handle(self, session_id: SessionId, hello: Hello) -> Result<bool, StoreError> {
        let Self {
            sessions,
            players,
            persistence,
            spawn,
            tick,
            roster_upserts,
        } = self;
        let Hello {
            protocol_version,
            uuid,
            public_key,
            signature,
            name,
        } = hello;
        let name: String = name.trim().chars().take(NAME_MAX_CHARS).collect();
        let Some(session) = sessions.entries.get(&session_id) else {
            return Ok(false);
        };
        let SessionPhase::Challenged { nonce, .. } = session.phase else {
            return Ok(true);
        };
        if protocol_version != PROTOCOL_VERSION {
            reject(
                sessions,
                session_id,
                format!(
                    "protocol version mismatch: server {PROTOCOL_VERSION}, client {protocol_version}"
                ),
            );
            return Ok(false);
        }
        if !authenticate_identity(nonce, uuid, public_key, &signature) {
            tracing::warn!("rejected unauthenticated identity for {name}");
            reject(
                sessions,
                session_id,
                "identity authentication failed".into(),
            );
            return Ok(false);
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
                    return Ok(false);
                };
                let restored = persistence.load_player(uuid)?;
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
        Ok(true)
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
