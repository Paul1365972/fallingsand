use fallingsand_core::{CellPos, ChunkPos, DirtyRect, MaterialId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlayerId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlayerInput {
    pub move_x: i8,
    pub jump: bool,
    pub down: bool,
    pub primary: bool,
    pub secondary: bool,
    pub aim: CellPos,
    pub selected: MaterialId,
}

impl Default for PlayerInput {
    fn default() -> Self {
        Self {
            move_x: 0,
            jump: false,
            down: false,
            primary: false,
            secondary: false,
            aim: CellPos::new(0, 0),
            selected: MaterialId::AIR,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EntityState {
    pub player: PlayerId,
    pub x: f32,
    pub y: f32,
    pub hp: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PixelBodyState {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub angle: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    Hello { protocol_version: u16, name: String },
    Input(PlayerInput),
    Goodbye,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    HelloAck {
        protocol_version: u16,
        registry_hash: u64,
        player: PlayerId,
        tick: u64,
        spawn: CellPos,
    },
    Reject {
        reason: String,
    },
    ChunkLoad {
        pos: ChunkPos,
        cells: Vec<u8>,
    },
    ChunkUnload {
        pos: ChunkPos,
    },
    ChunkDelta {
        pos: ChunkPos,
        rect: DirtyRect,
        cells: Vec<u8>,
    },
    EntityStates {
        entities: Vec<EntityState>,
    },
    PlayerJoined {
        player: PlayerId,
        name: String,
    },
    PlayerLeft {
        player: PlayerId,
    },
    PixelBodySpawn {
        id: u32,
        width: u8,
        height: u8,
        com_x: f32,
        com_y: f32,
        cells: Vec<u8>,
    },
    PixelBodyDespawn {
        id: u32,
    },
    PixelBodyStates {
        bodies: Vec<PixelBodyState>,
    },
    TickEnd {
        tick: u64,
    },
}
