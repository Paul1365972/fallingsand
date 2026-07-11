use fallingsand_core::{CellPos, ChunkPos, DirtyRect, ItemId, ItemStack};
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
pub struct InputState {
    pub move_x: i8,
    pub jump: bool,
    pub down: bool,
    pub primary: bool,
    pub secondary: bool,
    pub aim: CellPos,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            move_x: 0,
            jump: false,
            down: false,
            primary: false,
            secondary: false,
            aim: CellPos::new(0, 0),
        }
    }
}

impl InputState {
    pub fn merge_or(&mut self, next: Self) {
        if next.move_x != 0 {
            self.move_x = next.move_x;
        }
        self.jump |= next.jump;
        self.down |= next.down;
        self.primary |= next.primary;
        self.secondary |= next.secondary;
        self.aim = next.aim;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InputAction {
    Jump,
    ToggleFlight,
    SelectSlot(u8),
    SetBrush(u8),
    Slot(SlotAction),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InputFrame {
    pub state: InputState,
    pub actions: Vec<InputAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SlotAction {
    LeftClick { slot: u16 },
    RightClick { slot: u16 },
    QuickMove { slot: u16 },
    Trash,
    Craft { recipe: u16, all: bool },
    CreativeGrab { item: ItemId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerState {
    pub player: PlayerId,
    pub cx: i32,
    pub cy: i32,
    pub height: u8,
    pub burning: bool,
    pub facing_left: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkDebugRects {
    pub pos: ChunkPos,
    pub change: DirtyRect,
    pub sim: DirtyRect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelfState {
    pub hp: f32,
    pub air: f32,
    pub mode: GameMode,
    pub biome: String,
    pub band: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChunkOp {
    Load {
        pos: ChunkPos,
        cells: Vec<u8>,
    },
    Unload {
        pos: ChunkPos,
    },
    Delta {
        pos: ChunkPos,
        rect: DirtyRect,
        cells: Vec<u8>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TickFrame {
    pub tick: u64,
    pub world_age: u64,
    pub chunks: Vec<ChunkOp>,
    pub players: Vec<PlayerState>,
    pub inventory: Vec<(u16, Option<ItemStack>)>,
    pub cursor: Option<Option<ItemStack>>,
    pub trash: Option<Option<ItemStack>>,
    pub self_state: Option<SelfState>,
    pub debug: Vec<ChunkDebugRects>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    Hello {
        protocol_version: u16,
        uuid: PlayerUuid,
        name: String,
    },
    Input(InputFrame),
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
        player: PlayerId,
        spawn: CellPos,
    },
    Reject {
        reason: String,
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
    TickFrame(TickFrame),
}
