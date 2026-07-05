pub mod messages;
pub mod wire;

pub use messages::{
    ClientMessage, EntityState, PixelBodyState, PlayerId, PlayerInput, PlayerUuid, ServerMessage,
};
pub use wire::{
    CELL_WIRE_BYTES, WireError, cells_from_wire, cells_to_wire, decode_message, encode_message,
};

pub const PROTOCOL_VERSION: u16 = 6;
