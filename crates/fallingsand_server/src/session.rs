use bevy_ecs::prelude::*;
use fallingsand_core::ChunkPos;
use fallingsand_net::Connection;
use fallingsand_protocol::{EntityId, PlayerId, PlayerInput, PlayerUuid};
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
    pub known_items: FxHashSet<EntityId>,
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
            known_items: FxHashSet::default(),
            last_chat_tick: 0,
            debug: false,
        }
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
    pub input: PlayerInput,
}
