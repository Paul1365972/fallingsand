use bevy_ecs::prelude::*;
use fallingsand_core::ChunkPos;
use fallingsand_net::Connection;
use fallingsand_protocol::{
    InputState, PlayerId, PlayerUuid, SelfState, ServerMessage, encode_message,
};
use rustc_hash::FxHashSet;

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

#[derive(Component)]
pub struct Player {
    pub id: PlayerId,
    pub uuid: PlayerUuid,
    pub name: String,
    pub input: InputState,
    pub jump_pressed: bool,
    pub flying: bool,
    pub selected_slot: u8,
    pub brush_radius: u8,
    pub last_input_tick: u64,
}
