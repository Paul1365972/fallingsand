pub mod messages;
pub mod wire;

pub use messages::{
    ChunkDebugRects, ClientMessage, EntityId, EntityState, GameMode, ItemEntityState, PlayerId,
    PlayerInput, PlayerUuid, ServerMessage, SlotAction,
};
pub use wire::{
    CELL_WIRE_BYTES, WireError, cells_from_wire, cells_to_wire, decode_message, encode_message,
};

pub const PROTOCOL_VERSION: u16 = 17;
