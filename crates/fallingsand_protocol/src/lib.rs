pub mod messages;
pub mod stats;
pub mod wire;

pub use messages::{
    ChunkDebugRects, ChunkOp, ClientMessage, CursorMode, GameMode, InputAction, InputFrame,
    InputState, InteractionState, InteractionStatus, ParticleSpawn, PlayerAvatarState, PlayerId,
    PlayerState, PlayerUuid, SelfAvatarState, SelfLife, SelfState, ServerMessage, SlotAction,
    TickFrame, UseButton,
};
pub use stats::{ServerStats, TickProfile};
pub use wire::{WireError, cells_from_wire, cells_to_wire, decode_message, encode_message};

pub const PROTOCOL_VERSION: u16 = 51;
pub const MAX_INPUT_ACTIONS_PER_FRAME: usize = 64;

const IDENTITY_DOMAIN: &[u8] = b"fallingsand identity v1\0";
pub const IDENTITY_MESSAGE_LEN: usize = IDENTITY_DOMAIN.len() + 32;

pub fn identity_message(nonce: [u8; 32]) -> [u8; IDENTITY_MESSAGE_LEN] {
    let mut message = [0u8; IDENTITY_MESSAGE_LEN];
    message[..IDENTITY_DOMAIN.len()].copy_from_slice(IDENTITY_DOMAIN);
    message[IDENTITY_DOMAIN.len()..].copy_from_slice(&nonce);
    message
}
