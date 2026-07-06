use fallingsand_core::{CellPos, ChunkPos, DirtyRect, Fixed, MaterialId};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlayerId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlayerUuid(pub u128);

impl PlayerUuid {
    pub fn to_hex(self) -> String {
        format!("{:032x}", self.0)
    }

    pub fn from_hex(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() || text.len() > 32 {
            return None;
        }
        u128::from_str_radix(text, 16).ok().map(Self)
    }
}

impl fmt::Display for PlayerUuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum GameMode {
    #[default]
    Creative,
    Survival,
}

impl GameMode {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "creative" | "c" => Some(Self::Creative),
            "survival" | "s" => Some(Self::Survival),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Creative => "creative",
            Self::Survival => "survival",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlayerInput {
    pub move_x: i8,
    pub jump: bool,
    pub down: bool,
    pub fly: bool,
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
            fly: false,
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
    pub x: Fixed,
    pub y: Fixed,
    pub hp: f32,
    pub ducking: bool,
    pub mode: GameMode,
    pub burning: bool,
    pub air: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkDebugRects {
    pub pos: ChunkPos,
    pub change: DirtyRect,
    pub keep_alive: DirtyRect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    Hello {
        protocol_version: u16,
        uuid: PlayerUuid,
        name: String,
    },
    Input(PlayerInput),
    Chat {
        text: String,
    },
    SetDebug {
        enabled: bool,
    },
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
    Chat {
        player: PlayerId,
        name: String,
        text: String,
    },
    System {
        text: String,
    },
    Inventory {
        counts: Vec<(MaterialId, u64)>,
    },
    DebugRects {
        chunks: Vec<ChunkDebugRects>,
    },
    TickEnd {
        tick: u64,
        time_of_day: f32,
        day: u32,
    },
}
