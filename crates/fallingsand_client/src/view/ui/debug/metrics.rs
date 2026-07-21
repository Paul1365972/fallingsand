use bevy::diagnostic::DiagnosticsStore;
use bevy::prelude::Resource;
use fallingsand_core::{CHUNK_AREA, Cell, TICK_RATE};
use fallingsand_protocol::{ServerStats, TickProfile};
use std::collections::VecDeque;

const BUDGET_MS: f32 = 1000.0 / TICK_RATE as f32;
const STAT_WINDOW: f32 = 1.0;

#[derive(Default)]
pub(crate) struct StatWindow {
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
        self.samples.iter().map(|&(_, v)| v).sum::<f32>() / self.samples.len() as f32
    }

    pub(super) fn rate(&mut self, now: f32, value: f32) -> f32 {
        self.push(now, value);
        self.samples.iter().map(|&(_, v)| v).sum::<f32>() / STAT_WINDOW
    }
}

#[derive(Resource, Default)]
pub(crate) struct StatWindows {
    pub(super) uploads: StatWindow,
    pub(super) upload_bytes: StatWindow,
    pub(super) rx_per_sec: StatWindow,
    pub(super) sim_ms: StatWindow,
    pub(super) tick_ms: StatWindow,
    pub(super) tx_bytes: StatWindow,
    pub(super) slew_ms: StatWindow,
    pub(super) tps: StatWindow,
    pub(super) awake_cells: StatWindow,
    pub(super) active_chunks: StatWindow,
    pub(super) border_chunks: StatWindow,
    pub(super) awake_chunks: StatWindow,
    pub(super) particles: StatWindow,
    pub(super) phases: [StatWindow; TickProfile::PHASE_COUNT],
}

fn human_count(n: u64) -> String {
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

fn phase_lines(
    timing: &TickProfile,
    windows: &mut [StatWindow; TickProfile::PHASE_COUNT],
    now: f32,
) -> Vec<String> {
    let entries: Vec<String> = timing
        .phases()
        .iter()
        .zip(windows.iter_mut())
        .map(|((label, micros), window)| {
            let ms = window.avg(now, *micros as f32 / 1000.0);
            format!("{label} {ms:>5.2}")
        })
        .collect();
    let mut lines = Vec::new();
    let mut start = 0;
    for len in TickProfile::PHASE_GROUPS {
        lines.push(entries[start..start + len].join("  "));
        start += len;
    }
    lines
}

pub(super) fn render_pass_line(diagnostics: &DiagnosticsStore) -> Option<String> {
    let collect = |suffix: &str| {
        let mut passes: Vec<(&str, f64)> = diagnostics
            .iter()
            .filter_map(|d| {
                let name = d
                    .path()
                    .as_str()
                    .strip_prefix("render/")?
                    .strip_suffix(suffix)?;
                let value = d.smoothed()?;
                (value > 0.0).then_some((name, value))
            })
            .collect();
        passes.sort_by(|a, b| b.1.total_cmp(&a.1));
        passes.truncate(3);
        passes
    };
    let mut passes = collect("/elapsed_gpu");
    if passes.is_empty() {
        passes = collect("/elapsed_cpu");
    }
    if passes.is_empty() {
        return None;
    }
    let joined = passes
        .iter()
        .map(|(name, ms)| format!("{name} {ms:.2}"))
        .collect::<Vec<_>>()
        .join("  ");
    Some(format!("draw {joined}"))
}

pub(super) fn server_lines(
    server: &ServerStats,
    windows: &mut StatWindows,
    now: f32,
    out: &mut Vec<String>,
) {
    let timing = &server.timing;
    let tick_ms = windows.tick_ms.avg(now, timing.total as f32 / 1000.0);
    let sim_ms = windows.sim_ms.avg(now, timing.sim() as f32 / 1000.0);
    out.push(format!(
        "tick {tick_ms:>6.2} ms {:>3.0}%  peak {:>5.2}",
        tick_ms / BUDGET_MS * 100.0,
        timing.peak_total as f32 / 1000.0,
    ));
    out.push(format!(
        "sim  {sim_ms:>6.2} ms {:>3.0}%  peak {:>5.2}",
        sim_ms / BUDGET_MS * 100.0,
        timing.peak_sim as f32 / 1000.0,
    ));
    out.extend(phase_lines(timing, &mut windows.phases, now));
    out.push(format!(
        "{:>3.0} tps  +{:>2.0} ms behind  #{}",
        windows.tps.avg(now, server.tps),
        windows.slew_ms.avg(now, server.slew_ms as f32),
        server.tick,
    ));
    out.push(format!(
        "chunks {} loaded  {:.0} active  {:.0} border  {:.0} awake",
        server.loaded_chunks,
        windows.active_chunks.avg(now, server.active_chunks as f32),
        windows.border_chunks.avg(now, server.border_chunks as f32),
        windows.awake_chunks.avg(now, server.awake_chunks as f32),
    ));
    out.push(format!(
        "cells ~{} active  regions {} loaded",
        human_count(windows.awake_cells.avg(now, server.awake_cells as f32) as u64),
        server.loaded_regions,
    ));
    let mem = server.loaded_chunks as u64 * CHUNK_AREA as u64 * std::mem::size_of::<Cell>() as u64;
    out.push(format!(
        "bodies {}  tx {}/tick  mem ~{}",
        server.pixel_bodies,
        human_bytes(windows.tx_bytes.avg(now, server.replicated_bytes as f32) as u64),
        human_bytes(mem),
    ));
}
