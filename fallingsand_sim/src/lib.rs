#![feature(drain_filter)]

use chunk_ticket::ChunkTicketKey;

pub mod aabb;
pub mod cell;
pub mod chunk;
pub mod coords;
pub mod myimpl;
pub mod region;
pub mod util;
pub mod chunk_ticket;
pub mod simulator;
pub mod region_simulator;

pub trait ChunkTicketHolder {
    fn get_chunk_ticket(&self) -> Option<ChunkTicketKey>;
}

pub trait Entity<M, R>: Send {
    fn apply_move(&self) -> (i32, i32);
    fn chunk_move_notify(&self) -> Option<M>;
    fn should_remove_and_notify(&mut self) -> Option<R>;
}

pub trait Tile: Send {
}
