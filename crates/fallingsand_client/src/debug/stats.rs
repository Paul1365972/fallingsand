use bevy::prelude::*;
use std::collections::VecDeque;

const STAT_WINDOW: f32 = 1.0;

#[derive(Default)]
pub(super) struct StatWindow {
    samples: VecDeque<(f32, f32)>,
}

impl StatWindow {
    fn push(&mut self, now: f32, value: f32) {
        self.samples.push_back((now, value));
        while let Some(&(t, _)) = self.samples.front() {
            if now - t > STAT_WINDOW {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    pub(super) fn avg(&mut self, now: f32, value: f32) -> f32 {
        self.push(now, value);
        let n = self.samples.len();
        if n == 0 {
            value
        } else {
            self.samples.iter().map(|&(_, v)| v).sum::<f32>() / n as f32
        }
    }

    pub(super) fn rate(&mut self, now: f32, value: f32) -> f32 {
        self.push(now, value);
        self.samples.iter().map(|&(_, v)| v).sum::<f32>() / STAT_WINDOW
    }
}

#[derive(Resource, Default)]
pub(super) struct StatWindows {
    pub(super) uploads: StatWindow,
    pub(super) upload_bytes: StatWindow,
    pub(super) rx_per_sec: StatWindow,
    pub(super) sim_ms: StatWindow,
    pub(super) tx_bytes: StatWindow,
    pub(super) slew_ms: StatWindow,
    pub(super) tps: StatWindow,
    pub(super) awake_cells: StatWindow,
    pub(super) active_chunks: StatWindow,
    pub(super) border_chunks: StatWindow,
    pub(super) awake_chunks: StatWindow,
    pub(super) particles: StatWindow,
}

pub(super) fn human_count(n: u64) -> String {
    let s = if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    };
    format!("{s:<6}")
}

pub(super) fn human_bytes(bytes: u64) -> String {
    let (value, unit) = if bytes >= 1u64 << 30 {
        (bytes as f64 / (1u64 << 30) as f64, "GiB")
    } else if bytes >= 1u64 << 20 {
        (bytes as f64 / (1u64 << 20) as f64, "MiB")
    } else if bytes >= 1u64 << 10 {
        (bytes as f64 / (1u64 << 10) as f64, "KiB")
    } else {
        (bytes as f64, "B")
    };
    format!("{value:>6.1} {unit:>3}")
}
