use crate::regions::ChunkTickets;
use fallingsand_protocol::ServerStats;
use fallingsand_sim::CellWorld;

pub fn step_simulation(sim: &mut CellWorld, tickets: &ChunkTickets, stats: &mut ServerStats) {
    let timings = fallingsand_sim::step_scoped(sim, &|pos| tickets.simulates(pos), &|pos| {
        tickets.random_ticks(pos)
    });
    stats.tick = sim.tick();
    stats.timing.sim_simulate = timings.simulate_micros;
    stats.timing.sim_random_tick = timings.random_tick_micros;
    stats.active_chunks = tickets.active.len();
    stats.border_chunks = tickets.border.len();
}
