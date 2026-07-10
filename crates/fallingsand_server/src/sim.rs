use crate::player::PlayerActor;
use crate::{Registry, SimObstacles, SimWorld, TickStats};
use bevy_ecs::prelude::*;
use fallingsand_sim::ActorAabb;
use std::time::Instant;

const PEAK_SIM_WINDOW_TICKS: u64 = 2 * crate::TICK_RATE as u64;

pub fn build_obstacles(
    mut sim: ResMut<SimWorld>,
    mut obstacles: ResMut<SimObstacles>,
    query: Query<&PlayerActor>,
) {
    let boxes: Vec<ActorAabb> = query
        .iter()
        .map(|body| ActorAabb {
            x: body.0.x,
            y: body.0.y,
            half_w: body.0.half_w,
            half_h: body.0.half_h,
        })
        .collect();
    obstacles.0.rebuild(&mut sim.0, &boxes);
}

pub fn step_simulation(
    mut sim: ResMut<SimWorld>,
    registry: Res<Registry>,
    obstacles: Res<SimObstacles>,
    tickets: Res<crate::regions::ChunkTickets>,
    mut stats: ResMut<TickStats>,
) {
    let start = Instant::now();
    fallingsand_sim::step_scoped(&mut sim.0, &registry.0, &obstacles.0, &|pos| {
        tickets.simulates(pos)
    });
    stats.tick = sim.0.tick();
    stats.sim_micros = start.elapsed().as_micros() as u64;
    if stats.tick.is_multiple_of(PEAK_SIM_WINDOW_TICKS) {
        stats.peak_sim_micros = stats.sim_micros;
    } else {
        stats.peak_sim_micros = stats.peak_sim_micros.max(stats.sim_micros);
    }
    stats.active_chunks = tickets.active.len();
    stats.border_chunks = tickets.border.len();
}
