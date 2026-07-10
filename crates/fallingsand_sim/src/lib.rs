pub mod bodies;
mod kernel;
pub mod physics;
pub mod player;
mod rules;
mod window;
mod world;

pub use bodies::PixelBody;
pub use kernel::step_scoped;
pub use physics::ActorAabb;
pub use player::PlayerStamp;
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
