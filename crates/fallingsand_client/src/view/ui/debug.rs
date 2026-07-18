use crate::game::{ClientGame, InGame, Phase};
use crate::view::Game;
use crate::view::camera::CameraState;
use crate::view::chunks::ChunkVisuals;
use crate::view::particles::Particle;
use crate::view::sky::Sky;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use fallingsand_core::content;
use fallingsand_core::{
    CHUNK_AREA, CHUNK_SIZE, Cell, ChunkPos, MAX_HEALTH, Phase as MaterialPhase, REGION_SIZE_CELLS,
    SEASON_DAYS, TICK_RATE,
};
use fallingsand_protocol::{ServerStats, TickProfile};
use std::collections::VecDeque;

const BUDGET_MS: f32 = 1000.0 / TICK_RATE as f32;
const STAT_WINDOW: f32 = 1.0;

pub struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StatWindows>();
        app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        #[cfg(all(debug_assertions, not(feature = "tracy")))]
        app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
    }
}

#[derive(Component)]
pub(crate) struct DebugTextLeft;

#[derive(Component)]
pub(crate) struct DebugTextRight;

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

    fn avg(&mut self, now: f32, value: f32) -> f32 {
        self.push(now, value);
        self.samples.iter().map(|&(_, v)| v).sum::<f32>() / self.samples.len() as f32
    }

    fn rate(&mut self, now: f32, value: f32) -> f32 {
        self.push(now, value);
        self.samples.iter().map(|&(_, v)| v).sum::<f32>() / STAT_WINDOW
    }
}

#[derive(Resource, Default)]
pub struct StatWindows {
    uploads: StatWindow,
    upload_bytes: StatWindow,
    rx_per_sec: StatWindow,
    sim_ms: StatWindow,
    tick_ms: StatWindow,
    tx_bytes: StatWindow,
    slew_ms: StatWindow,
    tps: StatWindow,
    awake_cells: StatWindow,
    active_chunks: StatWindow,
    border_chunks: StatWindow,
    awake_chunks: StatWindow,
    particles: StatWindow,
    phases: [StatWindow; TickProfile::PHASE_COUNT],
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

fn human_bytes(bytes: u64) -> String {
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

fn render_pass_line(diagnostics: &DiagnosticsStore) -> Option<String> {
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

pub fn setup_overlay(mut commands: Commands, mut gizmo_configs: ResMut<GizmoConfigStore>) {
    gizmo_configs
        .config_mut::<DefaultGizmoConfigGroup>()
        .0
        .depth_bias = -0.1;
    let font = || TextFont {
        font_size: FontSize::Px(13.0),
        ..default()
    };
    let color = TextColor(Color::srgba(0.9, 0.95, 1.0, 0.95));

    commands.spawn((
        DebugTextLeft,
        Text::new(""),
        font(),
        color,
        Node {
            position_type: PositionType::Absolute,
            top: px(5),
            left: px(5),
            ..default()
        },
        GlobalZIndex(super::depth::DEBUG),
    ));

    commands.spawn((
        DebugTextRight,
        Text::new(""),
        font(),
        color,
        TextLayout::justify(Justify::Right),
        Node {
            position_type: PositionType::Absolute,
            top: px(5),
            right: px(5),
            ..default()
        },
        GlobalZIndex(super::depth::DEBUG),
    ));
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
pub fn update_overlay(
    game: Res<Game>,
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    camera: Res<CameraState>,
    sky: Res<Sky>,
    visuals: Res<ChunkVisuals>,
    particles: Query<(), With<Particle>>,
    mut windows: ResMut<StatWindows>,
    mut left: Single<&mut Text, (With<DebugTextLeft>, Without<DebugTextRight>)>,
    mut right: Single<&mut Text, (With<DebugTextRight>, Without<DebugTextLeft>)>,
) {
    if !game.0.view_prefs.debug_overlay {
        if !left.is_empty() {
            left.clear();
        }
        if !right.is_empty() {
            right.clear();
        }
        return;
    }

    let now = time.elapsed_secs();
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let frame = diagnostics.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME);
    let frame_ms = frame.and_then(|d| d.smoothed()).unwrap_or(0.0);
    let (frame_min, frame_max) = frame
        .map(|d| {
            d.values().fold((f64::INFINITY, 0.0f64), |(mn, mx), &v| {
                (mn.min(v), mx.max(v))
            })
        })
        .filter(|(mn, _)| mn.is_finite())
        .unwrap_or((0.0, 0.0));

    let mut left_lines: Vec<String> = Vec::new();
    let mut right_lines: Vec<String> = vec![
        format!("fallingsand v{}", env!("CARGO_PKG_VERSION")),
        format!("fps {fps:>3.0}  frame {frame_ms:>5.1} ms ({frame_min:>4.1}-{frame_max:>4.1})"),
    ];
    right_lines.extend(render_pass_line(&diagnostics));

    match game.0.ingame() {
        None => {}
        Some(ingame) if ingame.phase == Phase::Connecting => {
            connecting_lines(ingame, &mut left_lines, &mut right_lines);
        }
        Some(ingame) => {
            playing_lines(
                &game.0,
                ingame,
                &sky,
                &camera,
                &visuals,
                particles.iter().count(),
                &mut windows,
                now,
                &mut left_lines,
                &mut right_lines,
            );
        }
    }

    let left_joined = left_lines.join("\n");
    if ***left != left_joined {
        ***left = left_joined;
    }
    let right_joined = right_lines.join("\n");
    if ***right != right_joined {
        ***right = right_joined;
    }
}

fn rx_stats(ingame: &InGame) -> (u64, u64) {
    ingame
        .net
        .session
        .as_ref()
        .map(|session| (session.rx_per_sec, session.rx_bytes))
        .unwrap_or((0, 0))
}

fn connecting_lines(ingame: &InGame, left_lines: &mut Vec<String>, right_lines: &mut Vec<String>) {
    let (rx_per_sec, rx_bytes) = rx_stats(ingame);
    let supervisor = &ingame.net.supervisor;
    let target = supervisor
        .target
        .as_ref()
        .map(|target| target.url.as_str())
        .unwrap_or("local server");
    let mut conn = format!("connecting: {target}, attempt {}", supervisor.attempt);
    if let Some(err) = &supervisor.last_error {
        conn.push_str(&format!("\nlast error: {err}"));
    }
    left_lines.push(conn);
    right_lines.push(format!(
        "net rx {}/s ({})",
        human_bytes(rx_per_sec),
        human_bytes(rx_bytes)
    ));
}

#[allow(clippy::too_many_arguments)]
fn playing_lines(
    game: &ClientGame,
    ingame: &InGame,
    sky: &Sky,
    camera: &CameraState,
    visuals: &ChunkVisuals,
    particle_count: usize,
    windows: &mut StatWindows,
    now: f32,
    left_lines: &mut Vec<String>,
    right_lines: &mut Vec<String>,
) {
    let view = &ingame.world;
    let you = &ingame.you;
    let clock = &ingame.clock;
    let (rx_per_sec, rx_bytes) = rx_stats(ingame);

    let aim = game.input.held.aim;
    let chunk = aim.chunk();
    let off = aim.offset();
    let region = aim.region();
    left_lines.push(format!("cursor {},{}", aim.x, aim.y));
    left_lines.push(format!(
        "chunk {},{} +{:>2},{:>2}",
        chunk.x, chunk.y, off.x, off.y
    ));
    left_lines.push(format!(
        "region {},{} phase {}",
        region.x,
        region.y,
        block_phase(chunk)
    ));
    if let Some(local) = ingame.local_avatar() {
        let facing = compass(aim.x as f32 - local.pos.x, aim.y as f32 - local.pos.y);
        left_lines.push(format!("facing {facing}"));
    }

    let minute_of_day = clock.calendar.minute_of_day();
    let eclipse = if sky.state.is_solar_eclipse() {
        " solar eclipse"
    } else if sky.state.is_lunar_eclipse() {
        " lunar eclipse"
    } else {
        ""
    };
    left_lines.push(String::new());
    left_lines.push(format!(
        "day {} {} {}/{} {:02}:{:02} {}{}",
        clock.calendar.day(),
        clock.calendar.season().label(),
        clock.calendar.day_of_year() as u64 % SEASON_DAYS + 1,
        SEASON_DAYS,
        minute_of_day / 60,
        minute_of_day % 60,
        moon_name(clock.calendar.synodic_fraction()),
        eclipse
    ));
    left_lines.push(format!(
        "brightness {:.3} daylight {:.3}",
        sky.state.light, sky.state.daylight
    ));

    if let (Some(local), Some(avatar)) = (ingame.local_avatar(), you.life.avatar()) {
        let burning = if local.burning { " burning" } else { "" };
        left_lines.push(String::new());
        left_lines.push(format!(
            "hp {:>3.0}/{:.0} air {:>4.1}s{}",
            avatar.hp, MAX_HEALTH, avatar.air, burning
        ));
        left_lines.push(format!(
            "pos {:.1},{:.1} {}",
            local.pos.x,
            local.pos.y,
            you.mode.label(),
        ));
        if !you.biome.is_empty() {
            left_lines.push(format!("biome {} / {}", you.biome, you.band));
        }
    }

    left_lines.push(String::new());
    let cursor = match view.get_cell(aim) {
        Some(cell) if (cell.material.0 as usize) < content::MATERIAL_COUNT => {
            format!(
                "cursor: {} [{}] d{:.2}",
                content::material(cell.material).name,
                phase_label(content::phase(cell.material)),
                content::density_milli(cell.material) as f32 / 1000.0
            )
        }
        Some(_) => "cursor: ?".to_string(),
        None => "cursor: unloaded".to_string(),
    };
    left_lines.push(cursor);
    let selected = ingame
        .inventory
        .slot(ingame.inventory.selected)
        .and_then(|stack| content::try_item(stack.item).map(|info| info.display))
        .unwrap_or("empty")
        .to_string();
    left_lines.push(format!("selected: {selected}"));

    match ingame.net.embedded_stats() {
        Some(server) => server_lines(&server, windows, now, right_lines),
        None => right_lines.push(format!("server tick #{}", view.server_tick)),
    }
    right_lines.push(format!(
        "client {} chunks  {} players  {:.0} particles",
        view.chunks.len(),
        ingame.players.names.len(),
        windows.particles.avg(now, particle_count as f32),
    ));
    right_lines.push(format!(
        "net rx {}/s ({})",
        human_bytes(windows.rx_per_sec.avg(now, rx_per_sec as f32) as u64),
        human_bytes(rx_bytes),
    ));
    right_lines.push(format!(
        "upload {:>4.0}/s ({}/s)  {}px/cell  {}",
        windows.uploads.rate(now, visuals.uploads as f32),
        human_bytes(windows.upload_bytes.rate(now, visuals.upload_bytes as f32) as u64),
        camera.k,
        game.settings.render_mode.label(),
    ));
}

fn server_lines(server: &ServerStats, windows: &mut StatWindows, now: f32, out: &mut Vec<String>) {
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
        "cells ~{} active  regions {}/{} dirty",
        human_count(windows.awake_cells.avg(now, server.awake_cells as f32) as u64),
        server.loaded_regions,
        server.dirty_regions,
    ));
    let mem = server.loaded_chunks as u64 * CHUNK_AREA as u64 * std::mem::size_of::<Cell>() as u64;
    out.push(format!(
        "bodies {}  tx {}/tick  mem ~{}",
        server.pixel_bodies,
        human_bytes(windows.tx_bytes.avg(now, server.replicated_bytes as f32) as u64),
        human_bytes(mem),
    ));
}

pub fn draw_debug_borders(game: Res<Game>, state: Res<CameraState>, mut gizmos: Gizmos) {
    if !game.0.view_prefs.debug_borders {
        return;
    }
    let Some(ingame) = game.0.ingame() else {
        return;
    };
    let half = state.view_cells() / 2.0;
    let min = state.pos - half;
    let max = state.pos + half;
    let k = state.k as f32;
    let to_px = |world: Vec2| (world - state.pos) * k;

    let chunk = CHUNK_SIZE as f32;
    let region = REGION_SIZE_CELLS as f32;
    let chunk_color = Color::srgba(1.0, 1.0, 1.0, 0.12);
    let region_color = Color::srgba(1.0, 0.55, 0.2, 0.6);

    let mut x = (min.x / chunk).floor() * chunk;
    while x <= max.x {
        let color = if x.rem_euclid(region) == 0.0 {
            region_color
        } else {
            chunk_color
        };
        gizmos.line_2d(
            to_px(Vec2::new(x, min.y)),
            to_px(Vec2::new(x, max.y)),
            color,
        );
        x += chunk;
    }
    let mut y = (min.y / chunk).floor() * chunk;
    while y <= max.y {
        let color = if y.rem_euclid(region) == 0.0 {
            region_color
        } else {
            chunk_color
        };
        gizmos.line_2d(
            to_px(Vec2::new(min.x, y)),
            to_px(Vec2::new(max.x, y)),
            color,
        );
        y += chunk;
    }

    for flash in &ingame.debug.rects {
        let origin = Vec2::new(flash.pos.x as f32 * chunk, flash.pos.y as f32 * chunk);
        let corner = origin + Vec2::new(flash.rect.min_x as f32, flash.rect.min_y as f32);
        let size = Vec2::new(flash.rect.width() as f32, flash.rect.height() as f32);
        let color = if flash.is_sim {
            Color::srgba(0.2, 0.9, 1.0, 0.8)
        } else {
            Color::srgba(1.0, 0.9, 0.2, 0.8)
        };
        gizmos.rect_2d(
            Isometry2d::from_translation(to_px(corner + size / 2.0)),
            size * k,
            color,
        );
    }
}

fn block_phase(chunk: ChunkPos) -> u8 {
    ((chunk.x >> 1).rem_euclid(2) + (chunk.y >> 1).rem_euclid(2) * 2) as u8
}

fn compass(dx: f32, dy: f32) -> &'static str {
    if dx == 0.0 && dy == 0.0 {
        return "-";
    }
    const DIRS: [&str; 8] = ["E", "NE", "N", "NW", "W", "SW", "S", "SE"];
    let deg = dy.atan2(dx).to_degrees().rem_euclid(360.0);
    DIRS[(((deg + 22.5) / 45.0) as usize) % 8]
}

fn moon_name(synodic_fraction: f32) -> &'static str {
    const NAMES: [&str; 8] = [
        "new moon",
        "waxing crescent",
        "first quarter",
        "waxing gibbous",
        "full moon",
        "waning gibbous",
        "last quarter",
        "waning crescent",
    ];
    NAMES[((synodic_fraction * NAMES.len() as f32).round() as usize) % NAMES.len()]
}

fn phase_label(phase: MaterialPhase) -> &'static str {
    match phase {
        MaterialPhase::Empty => "empty",
        MaterialPhase::Solid => "solid",
        MaterialPhase::Powder => "powder",
        MaterialPhase::Liquid => "liquid",
        MaterialPhase::Gas => "gas",
    }
}
