pub mod messages;
pub mod wire;

pub use messages::{
    ChunkDebugRects, ChunkOp, ClientMessage, GameMode, InputAction, InputFrame, InputState,
    PlayerId, PlayerState, PlayerUuid, SelfState, ServerMessage, SlotAction, TickFrame,
};
pub use wire::{WireError, cells_from_wire, cells_to_wire, decode_message, encode_message};

pub const PROTOCOL_VERSION: u16 = 29;

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
