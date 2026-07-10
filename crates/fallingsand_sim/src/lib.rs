pub mod bodies;
pub mod kernel;
pub mod physics;
pub mod player;
pub mod rules;
pub mod window;
pub mod world;

pub use bodies::PixelBody;
pub use kernel::{step, step_scoped};
pub use physics::{Actor, ActorAabb, Footprint, move_body};
pub use player::PlayerStamp;
pub use window::{SPEED_OF_LIGHT, SimWindow};
pub use world::CellWorld;

pub(crate) fn chebyshev_ring(radius: i32) -> Vec<(i32, i32)> {
    let mut ring = Vec::new();
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx.abs().max(dy.abs()) == radius {
                ring.push((dx, dy));
            }
        }
    }
    ring
}
