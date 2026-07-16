const PEAK_WINDOW_TICKS: u64 = fallingsand_core::ticks_from_secs(2.0);

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct TickProfile {
    pub network: u32,
    pub player_input: u32,
    pub regions: u32,
    pub sim_simulate: u32,
    pub sim_random_tick: u32,
    pub physics: u32,
    pub bodies: u32,
    pub hazards: u32,
    pub lifecycle: u32,
    pub replicate: u32,
    pub persistence: u32,
    pub total: u32,
    pub peak_sim: u32,
    pub peak_total: u32,
}

impl TickProfile {
    pub const PHASE_GROUPS: [usize; 4] = [3, 2, 4, 2];
    pub const PHASE_COUNT: usize = {
        let mut count = 0;
        let mut i = 0;
        while i < Self::PHASE_GROUPS.len() {
            count += Self::PHASE_GROUPS[i];
            i += 1;
        }
        count
    };

    pub fn sim(&self) -> u32 {
        self.sim_simulate + self.sim_random_tick
    }

    pub fn phases(&self) -> [(&'static str, u32); Self::PHASE_COUNT] {
        [
            ("network", self.network),
            ("input", self.player_input),
            ("regions", self.regions),
            ("simulate", self.sim_simulate),
            ("random", self.sim_random_tick),
            ("physics", self.physics),
            ("bodies", self.bodies),
            ("hazards", self.hazards),
            ("lifecycle", self.lifecycle),
            ("replicate", self.replicate),
            ("persist", self.persistence),
        ]
    }

    pub fn finish(&mut self, tick: u64, total_micros: u32) {
        self.total = total_micros;
        let sim = self.sim();
        if tick.is_multiple_of(PEAK_WINDOW_TICKS) {
            self.peak_sim = sim;
            self.peak_total = total_micros;
        } else {
            self.peak_sim = self.peak_sim.max(sim);
            self.peak_total = self.peak_total.max(total_micros);
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ServerStats {
    pub tick: u64,
    pub tps: f32,
    pub slew_ms: u32,
    pub loaded_chunks: usize,
    pub active_chunks: usize,
    pub border_chunks: usize,
    pub awake_chunks: usize,
    pub awake_cells: u64,
    pub loaded_regions: u32,
    pub dirty_regions: u32,
    pub pixel_bodies: usize,
    pub players: usize,
    pub replicated_bytes: u64,
    pub timing: TickProfile,
}
