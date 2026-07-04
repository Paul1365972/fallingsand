use bevy_ecs::prelude::*;
use fallingsand_core::ChunkPos;
use fallingsand_net::Connection;
use fallingsand_protocol::{PlayerId, PlayerInput};
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
    pub known_chunks: FxHashSet<ChunkPos>,
}

impl Session {
    pub fn new(conn: Box<dyn Connection>) -> Self {
        Self {
            conn,
            state: SessionState::AwaitingHello,
            entity: None,
            player: None,
            known_chunks: FxHashSet::default(),
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
    pub name: String,
    pub input: PlayerInput,
}
