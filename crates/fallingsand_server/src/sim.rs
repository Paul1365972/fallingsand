use crate::TickStats;
use crate::regions::ChunkTickets;
use fallingsand_sim::CellWorld;
use std::time::Instant;

const PEAK_SIM_WINDOW_TICKS: u64 = 2 * crate::TICK_RATE as u64;

pub fn step_simulation(sim: &mut CellWorld, tickets: &ChunkTickets, stats: &mut TickStats) {
    let start = Instant::now();
    fallingsand_sim::step_scoped(sim, &|pos| tickets.simulates(pos), &|pos| {
        tickets.random_ticks(pos)
    });
    stats.tick = sim.tick();
    stats.sim_micros = start.elapsed().as_micros() as u64;
    if stats.tick.is_multiple_of(PEAK_SIM_WINDOW_TICKS) {
        stats.peak_sim_micros = stats.sim_micros;
    } else {
        stats.peak_sim_micros = stats.peak_sim_micros.max(stats.sim_micros);
    }
    stats.active_chunks = tickets.active.len();
    stats.border_chunks = tickets.border.len();
}
