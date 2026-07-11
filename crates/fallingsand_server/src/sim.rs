use crate::{SimWorld, TickStats};
use bevy_ecs::prelude::*;
use std::time::Instant;

const PEAK_SIM_WINDOW_TICKS: u64 = 2 * crate::TICK_RATE as u64;

pub fn step_simulation(
    mut sim: ResMut<SimWorld>,
    tickets: Res<crate::regions::ChunkTickets>,
    mut stats: ResMut<TickStats>,
) {
    let start = Instant::now();
    fallingsand_sim::step_scoped(&mut sim.0, &|pos| tickets.simulates(pos));
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
