use crate::regions::ChunkTickets;
use fallingsand_sim::{CellWorld, SimTimings, Simulator};

pub struct SimStepMetrics {
    pub tick: u64,
    pub timings: SimTimings,
    pub active_chunks: usize,
    pub border_chunks: usize,
}

pub fn step_simulation(
    simulator: &mut Simulator,
    sim: &mut CellWorld,
    tickets: &ChunkTickets,
) -> SimStepMetrics {
    let timings = simulator.step_scoped(sim, &|pos| tickets.simulates(pos), &|pos| {
        tickets.random_ticks(pos)
    });
    SimStepMetrics {
        tick: sim.tick(),
        timings,
        active_chunks: tickets.active.len(),
        border_chunks: tickets.border.len(),
    }
}
