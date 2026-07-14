pub mod messages;
pub mod wire;

pub use messages::{
    ChunkDebugRects, ChunkOp, ClientMessage, CursorMode, GameMode, InputAction, InputFrame,
    InputState, InteractionState, InteractionStatus, PlayerAvatarState, PlayerId, PlayerState,
    PlayerUuid, SelfAvatarState, SelfLife, SelfState, ServerMessage, SlotAction, TickFrame,
};
pub use wire::{WireError, cells_from_wire, cells_to_wire, decode_message, encode_message};

pub const PROTOCOL_VERSION: u16 = 43;
pub const MAX_INPUT_ACTIONS_PER_FRAME: usize = 64;

const IDENTITY_DOMAIN: &[u8] = b"fallingsand identity v1\0";
pub const IDENTITY_MESSAGE_LEN: usize = IDENTITY_DOMAIN.len() + 32;

pub fn identity_message(nonce: [u8; 32]) -> [u8; IDENTITY_MESSAGE_LEN] {
    let mut message = [0u8; IDENTITY_MESSAGE_LEN];
    message[..IDENTITY_DOMAIN.len()].copy_from_slice(IDENTITY_DOMAIN);
    message[IDENTITY_DOMAIN.len()..].copy_from_slice(&nonce);
    message
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Stats {
    pub tick: u64,
    pub sim_micros: u64,
    pub peak_sim_micros: u64,
    pub tps: f32,
    pub slew_ms: u32,
    pub awake_chunks: usize,
    pub awake_cells: u64,
    pub loaded_chunks: usize,
    pub active_chunks: usize,
    pub border_chunks: usize,
    pub loaded_regions: u32,
    pub dirty_regions: u32,
    pub players: usize,
    pub replicated_bytes: u64,
    pub pixel_bodies: usize,
}
