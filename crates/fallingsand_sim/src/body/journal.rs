use fallingsand_core::{CellPos, ChunkPos, MaterialId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freedom {
    Turn,
    X,
    Y,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Peer {
    Terrain,
    Body(u32),
    Grain(MaterialId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Committed,
    Rebounded,
    Parked,
    Unloaded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Rests,
    Fast,
    Spinning,
    Unloaded,
    OnBody,
    OnPowder,
    Unsupported,
    Restless,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Detached {
        id: u32,
        cells: usize,
        span: (i32, i32),
        at: CellPos,
        material: MaterialId,
    },
    Split {
        source: u32,
        parts: Vec<u32>,
        at: CellPos,
    },
    Dissolved {
        id: u32,
        at: CellPos,
    },
    Released {
        id: u32,
        at: CellPos,
        material: MaterialId,
    },
    Struck {
        id: u32,
        at: CellPos,
        push: i128,
        drag: i128,
    },
    Loaded {
        id: u32,
        at: CellPos,
        jy: i64,
    },
    Parked {
        id: u32,
        blocker: ChunkPos,
    },
    Woke {
        id: u32,
    },
    Quantum {
        id: u32,
        freedom: Freedom,
        sign: i32,
        outcome: Outcome,
    },
    Contact {
        id: u32,
        at: CellPos,
        normal: (i32, i32),
        peer: Peer,
        push: i128,
        drag: i128,
    },
    Entrained {
        id: u32,
        at: CellPos,
        material: MaterialId,
        jx: i64,
        jy: i64,
    },
    Carried {
        id: u32,
        at: CellPos,
        material: MaterialId,
        impulse: i64,
    },
    Settled {
        id: u32,
        cells: usize,
        at: CellPos,
    },
    Restless {
        id: u32,
        verdict: Verdict,
        residual: (i64, i64, i64),
    },
}

#[derive(Default)]
pub struct Journal {
    enabled: bool,
    events: Vec<Event>,
}

impl Journal {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.events = Vec::new();
        }
    }

    pub fn drain(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub(super) fn record(&mut self, event: impl FnOnce() -> Event) {
        if self.enabled {
            self.events.push(event());
        }
    }

    pub(super) fn records(&self) -> bool {
        self.enabled
    }
}
