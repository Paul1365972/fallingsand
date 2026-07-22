use fallingsand_core::{CellOffset, CellPos, ChunkPos, DirtyRect, ItemId, ItemStack, MaterialId};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlayerId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlayerUuid(pub u128);

impl PlayerUuid {
    pub fn from_public_key(public_key: &[u8; 32]) -> Self {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(public_key);
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        Self(u128::from_le_bytes(bytes))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CursorMode {
    #[default]
    Smart,
    Precise,
}

impl CursorMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Smart => "smart",
            Self::Precise => "precise",
        }
    }

    pub fn cycled(self) -> Self {
        match self {
            Self::Smart => Self::Precise,
            Self::Precise => Self::Smart,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct InputState {
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub down: bool,
    pub primary: bool,
    pub secondary: bool,
    pub aim: CellPos,
    pub cursor_mode: CursorMode,
}

impl InputState {
    pub fn move_x(&self) -> i8 {
        self.right as i8 - self.left as i8
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            left: false,
            right: false,
            jump: false,
            down: false,
            primary: false,
            secondary: false,
            aim: CellPos::new(0, 0),
            cursor_mode: CursorMode::Smart,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UseButton {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InputAction {
    Jump,
    Revive,
    ToggleFlight,
    SelectSlot(u8),
    Slot(SlotAction),
    Use { button: UseButton, cell: CellPos },
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
    pub avatar: Option<PlayerAvatarState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerAvatarState {
    pub cx: i32,
    pub cy: i32,
    pub height: u8,
    pub burning: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum InteractionStatus {
    #[default]
    None,
    Valid,
    OutOfReach,
    NoTarget,
    Occupied,
    WrongTool,
    Undiggable,
    InventoryFull,
    NotPlaceable,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct InteractionState {
    pub target: CellPos,
    pub status: InteractionStatus,
    pub progress: f32,
    pub dig_material: Option<MaterialId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SelfAvatarState {
    pub hp: f32,
    pub air: f32,
    pub interaction: InteractionState,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum SelfLife {
    #[default]
    Entering,
    Alive(SelfAvatarState),
    Dead,
    Reviving,
}

impl SelfLife {
    pub const fn avatar(self) -> Option<SelfAvatarState> {
        match self {
            Self::Alive(avatar) => Some(avatar),
            Self::Entering | Self::Dead | Self::Reviving => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ParticleSpawn {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub color: [u8; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyDebugCell {
    pub body: u32,
    pub offset: CellOffset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkDebugRects {
    pub pos: ChunkPos,
    pub change: DirtyRect,
    pub sim: DirtyRect,
    pub bodies: Vec<BodyDebugCell>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelfState {
    pub life: SelfLife,
    pub anchor: Option<CellPos>,
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
    pub particles: Vec<ParticleSpawn>,
    pub debug: Vec<ChunkDebugRects>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    Hello {
        protocol_version: u16,
        uuid: PlayerUuid,
        public_key: [u8; 32],
        #[serde(with = "serde_big_array::BigArray")]
        signature: [u8; 64],
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
    Challenge {
        nonce: [u8; 32],
    },
    HelloAck {
        protocol_version: u16,
        player: PlayerId,
        selected: u8,
    },
    Reject {
        reason: String,
    },
    RosterUpsert {
        player: PlayerId,
        name: String,
    },
    RosterRemove {
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
    History {
        entries: Vec<String>,
    },
    TickFrame(Box<TickFrame>),
}
