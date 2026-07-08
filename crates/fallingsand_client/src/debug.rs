use crate::ClientRegistry;
use crate::camera::CameraControl;
use crate::inventory::{LocalInventory, SelectedSlot};
use crate::net::{EmbeddedServerStats, ServerMsg, Session, Supervisor};
use crate::particles::Particle;
use crate::player::{InputState, LocalPlayerState, PlayerNames};
use crate::render::ChunkVisuals;
use crate::sky::{Sky, WorldTime};
use crate::worldview::WorldView;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use fallingsand_core::{
    CHUNK_AREA, CHUNK_SIZE, Cell, ChunkPos, DirtyRect, Phase, REGION_SIZE_CELLS,
};
use std::collections::VecDeque;

pub struct DebugOverlayPlugin;

const MAX_HP: f32 = 100.0;
const BUDGET_MS: f32 = 1000.0 / 60.0;

#[derive(Component)]
pub struct DebugTextLeft;

#[derive(Component)]
pub struct DebugTextRight;

#[derive(Resource, Default)]
pub struct DebugVisible(pub bool);

#[derive(Resource, Default)]
pub struct BordersVisible(pub bool);

#[derive(Resource, Default)]
struct F3ComboUsed(bool);

struct RectFlash {
    pos: ChunkPos,
    rect: DirtyRect,
    keep_alive: bool,
    at: f32,
}

#[derive(Resource, Default)]
struct RectFlashes(Vec<RectFlash>);

const FLASH_SECS: f32 = 0.4;

const STAT_WINDOW: f32 = 1.0;

#[derive(Default)]
struct StatWindow {
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
        let n = self.samples.len();
        if n == 0 {
            value
        } else {
            self.samples.iter().map(|&(_, v)| v).sum::<f32>() / n as f32
        }
    }

    fn rate(&mut self, now: f32, value: f32) -> f32 {
        self.push(now, value);
        self.samples.iter().map(|&(_, v)| v).sum::<f32>() / STAT_WINDOW
    }
}

#[derive(Resource, Default)]
struct StatWindows {
    uploads: StatWindow,
    upload_bytes: StatWindow,
    rx_per_sec: StatWindow,
    sim_ms: StatWindow,
    tx_bytes: StatWindow,
    slew_ms: StatWindow,
    tps: StatWindow,
    awake_cells: StatWindow,
    active_chunks: StatWindow,
    border_chunks: StatWindow,
    awake_chunks: StatWindow,
    particles: StatWindow,
}

impl Plugin for DebugOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .insert_resource(DebugVisible(true))
            .init_resource::<BordersVisible>()
            .init_resource::<F3ComboUsed>()
            .init_resource::<RectFlashes>()
            .init_resource::<StatWindows>()
            .add_systems(Startup, setup_overlay)
            .add_systems(Update, (toggle_overlay, update_overlay, screenshot))
            .add_systems(
                Update,
                (sync_debug_stream, track_rects, draw_borders)
                    .chain()
                    .run_if(in_state(crate::AppState::InGame)),
            );
    }
}

fn screenshot(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::F2) {
        let path = chrono::Local::now()
            .format("screenshot-%Y-%m-%d_%H-%M-%S.png")
            .to_string();
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
}

fn setup_overlay(mut commands: Commands) {
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
        GlobalZIndex(100),
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
        GlobalZIndex(100),
    ));
}

fn toggle_overlay(
    keys: Res<ButtonInput<KeyCode>>,
    mut visible: ResMut<DebugVisible>,
    mut borders: ResMut<BordersVisible>,
    mut combo: ResMut<F3ComboUsed>,
    mode: Res<crate::player::LocalMode>,
    session: Option<ResMut<Session>>,
) {
    if keys.pressed(KeyCode::F3) && keys.just_pressed(KeyCode::KeyG) {
        borders.0 = !borders.0;
        combo.0 = true;
    }
    if keys.pressed(KeyCode::F3) && keys.just_pressed(KeyCode::KeyN) {
        combo.0 = true;
        if let Some(mut session) = session
            && session.player.is_some()
        {
            let target = match mode.0 {
                fallingsand_protocol::GameMode::Creative => "s",
                fallingsand_protocol::GameMode::Survival => "c",
            };
            session.send(&fallingsand_protocol::ClientMessage::Chat {
                text: format!("/gm {target}"),
            });
        }
    }
    if keys.just_released(KeyCode::F3) {
        if !combo.0 {
            visible.0 = !visible.0;
        }
        combo.0 = false;
    }
}

fn sync_debug_stream(
    borders: Res<BordersVisible>,
    session: Option<ResMut<Session>>,
    mut messages: MessageReader<ServerMsg>,
    mut subscribed: Local<bool>,
) {
    let Some(mut session) = session else {
        *subscribed = false;
        return;
    };
    let rejoined = messages.read().any(|ServerMsg(message)| {
        matches!(
            message,
            fallingsand_protocol::ServerMessage::HelloAck { .. }
        )
    });
    if rejoined {
        *subscribed = false;
    }
    if session.player.is_some() && *subscribed != borders.0 {
        session.send(&fallingsand_protocol::ClientMessage::SetDebug { enabled: borders.0 });
        *subscribed = borders.0;
    }
}

fn track_rects(
    mut flashes: ResMut<RectFlashes>,
    mut messages: MessageReader<ServerMsg>,
    time: Res<Time>,
    borders: Res<BordersVisible>,
) {
    if !borders.0 {
        messages.clear();
        if !flashes.0.is_empty() {
            flashes.0.clear();
        }
        return;
    }
    let now = time.elapsed_secs();
    flashes.0.retain(|flash| now - flash.at < FLASH_SECS);
    for ServerMsg(message) in messages.read() {
        let fallingsand_protocol::ServerMessage::DebugRects { chunks } = message else {
            continue;
        };
        for entry in chunks {
            for (rect, keep_alive) in [(entry.change, false), (entry.keep_alive, true)] {
                if rect.is_empty() {
                    continue;
                }
                flashes
                    .0
                    .retain(|flash| flash.pos != entry.pos || flash.keep_alive != keep_alive);
                flashes.0.push(RectFlash {
                    pos: entry.pos,
                    rect,
                    keep_alive,
                    at: now,
                });
            }
        }
    }
}

fn draw_borders(
    borders: Res<BordersVisible>,
    flashes: Res<RectFlashes>,
    camera: Single<(&Camera, &GlobalTransform), With<crate::camera::SkyCamera>>,
    time: Res<Time>,
    mut gizmos: Gizmos,
) {
    if !borders.0 {
        return;
    }
    let (camera, camera_transform) = *camera;
    let Some(viewport) = camera.logical_viewport_size() else {
        return;
    };
    let (Ok(a), Ok(b)) = (
        camera.viewport_to_world_2d(camera_transform, Vec2::ZERO),
        camera.viewport_to_world_2d(camera_transform, viewport),
    ) else {
        return;
    };
    let min = a.min(b);
    let max = a.max(b);

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
        gizmos.line_2d(Vec2::new(x, min.y), Vec2::new(x, max.y), color);
        x += chunk;
    }
    let mut y = (min.y / chunk).floor() * chunk;
    while y <= max.y {
        let color = if y.rem_euclid(region) == 0.0 {
            region_color
        } else {
            chunk_color
        };
        gizmos.line_2d(Vec2::new(min.x, y), Vec2::new(max.x, y), color);
        y += chunk;
    }

    let now = time.elapsed_secs();
    for flash in &flashes.0 {
        let alpha = (1.0 - (now - flash.at) / FLASH_SECS).clamp(0.0, 1.0) * 0.8;
        let origin = Vec2::new(flash.pos.x as f32 * chunk, flash.pos.y as f32 * chunk);
        let corner = origin + Vec2::new(flash.rect.min_x as f32, flash.rect.min_y as f32);
        let size = Vec2::new(flash.rect.width() as f32, flash.rect.height() as f32);
        let color = if flash.keep_alive {
            Color::srgba(0.2, 0.9, 1.0, alpha)
        } else {
            Color::srgba(1.0, 0.9, 0.2, alpha)
        };
        gizmos.rect_2d(
            Isometry2d::from_translation(corner + size / 2.0),
            size,
            color,
        );
    }
}

#[derive(SystemParam)]
struct Overlay<'w, 's> {
    diagnostics: Res<'w, DiagnosticsStore>,
    supervisor: Res<'w, Supervisor>,
    server: Res<'w, EmbeddedServerStats>,
    session: Option<Res<'w, Session>>,
    view: Res<'w, WorldView>,
    visuals: Res<'w, ChunkVisuals>,
    names: Res<'w, PlayerNames>,
    selected: Res<'w, SelectedSlot>,
    inventory: Res<'w, LocalInventory>,
    item_reg: Res<'w, crate::ClientItemRegistry>,
    input: Res<'w, InputState>,
    registry: Res<'w, ClientRegistry>,
    fly: Res<'w, crate::player::FlyToggle>,
    world_time: Res<'w, WorldTime>,
    celestial: Res<'w, Sky>,
    player: Res<'w, LocalPlayerState>,
    camera: Res<'w, CameraControl>,
    particles: Query<'w, 's, (), With<Particle>>,
}

fn update_overlay(
    visible: Res<DebugVisible>,
    game_state: Option<Res<State<crate::GameState>>>,
    time: Res<Time>,
    mut windows: ResMut<StatWindows>,
    ctx: Overlay,
    mut left: Single<&mut Text, (With<DebugTextLeft>, Without<DebugTextRight>)>,
    mut right: Single<&mut Text, (With<DebugTextRight>, Without<DebugTextLeft>)>,
) {
    if !visible.0 {
        if !left.is_empty() {
            left.clear();
        }
        if !right.is_empty() {
            right.clear();
        }
        return;
    }

    let now = time.elapsed_secs();

    let diagnostics = &ctx.diagnostics;
    let supervisor = &ctx.supervisor;
    let server = &ctx.server;
    let view = &ctx.view;
    let visuals = &ctx.visuals;
    let names = &ctx.names;
    let input = &ctx.input;
    let registry = &ctx.registry;
    let fly = &ctx.fly;
    let world_time = &ctx.world_time;
    let player = &ctx.player;
    let camera = &ctx.camera;

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

    let game_state = game_state.map(|state| *state.get());
    let embedded = supervisor.target.is_none();
    let (rx_per_sec, rx_bytes) = ctx
        .session
        .as_ref()
        .map(|session| (session.rx_per_sec, session.rx_bytes))
        .unwrap_or((0, 0));

    let mut left_lines: Vec<String> = Vec::new();
    let mut right_lines: Vec<String> = vec![
        format!("fallingsand v{}", env!("CARGO_PKG_VERSION")),
        format!("{fps:>3.0} fps {frame_ms:>5.1} ms ({frame_min:>5.1}/{frame_max:>5.1})"),
    ];

    match game_state {
        None => {}
        Some(crate::GameState::Connecting) => {
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
        Some(crate::GameState::Playing) => {
            let aim = input.aim;
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
            if player.present {
                let facing = compass(aim.x as f32 - player.pos.x, aim.y as f32 - player.pos.y);
                left_lines.push(format!("facing {facing}"));
            }

            let minute_of_day = world_time.calendar.minute_of_day();
            let eclipse = if ctx.celestial.state.is_solar_eclipse() {
                " solar eclipse"
            } else if ctx.celestial.state.is_lunar_eclipse() {
                " lunar eclipse"
            } else {
                ""
            };
            left_lines.push(String::new());
            left_lines.push(format!(
                "day {} {:02}:{:02} {}{}",
                world_time.calendar.day(),
                minute_of_day / 60,
                minute_of_day % 60,
                moon_name(world_time.moon_phase()),
                eclipse
            ));

            if player.present {
                let burning = if player.burning { " burning" } else { "" };
                let fly = if fly.0 { ", fly" } else { "" };
                left_lines.push(String::new());
                left_lines.push(format!(
                    "hp {:>3.0}/{:.0} air {:>4.1}s{}",
                    player.hp, MAX_HP, player.air, burning
                ));
                left_lines.push(format!(
                    "pos {:.1},{:.1} {}{}",
                    player.pos.x,
                    player.pos.y,
                    player.mode.label(),
                    fly
                ));
            }

            left_lines.push(String::new());
            let cursor = match view.get_cell(aim) {
                Some(cell) => match registry.0.try_get(cell.material) {
                    Some(material) => format!(
                        "cursor: {} [{}] d{:.2}",
                        material.name,
                        phase_label(material.phase),
                        material.density
                    ),
                    None => "cursor: ?".to_string(),
                },
                None => "cursor: unloaded".to_string(),
            };
            left_lines.push(cursor);
            let selected = ctx
                .inventory
                .slots
                .get(ctx.selected.0)
                .copied()
                .flatten()
                .and_then(|stack| {
                    ctx.item_reg
                        .0
                        .try_get(stack.item)
                        .map(|def| def.display.clone())
                })
                .unwrap_or_else(|| "empty".to_string());
            left_lines.push(format!("selected: {selected}"));

            if embedded {
                let sim_ms = windows.sim_ms.avg(now, server.sim_micros as f32 / 1000.0);
                let peak_ms = server.peak_sim_micros as f32 / 1000.0;
                right_lines.push(format!(
                    "sim {sim_ms:>6.2} ms ({:>3.0}%) peak {peak_ms:>6.2}",
                    sim_ms / BUDGET_MS * 100.0
                ));
                right_lines.push(format!(
                    "tick #{} {:>3.0} tps +{:>2.0} ms",
                    server.tick,
                    windows.tps.avg(now, server.tps),
                    windows.slew_ms.avg(now, server.slew_ms as f32)
                ));
            }

            if embedded {
                right_lines.push(format!(
                    "chunks L/A/B/W {:>4}/{:>4.0}/{:>4.0}/{:>4.0} | {:>4} client",
                    server.loaded_chunks,
                    windows.active_chunks.avg(now, server.active_chunks as f32),
                    windows.border_chunks.avg(now, server.border_chunks as f32),
                    windows.awake_chunks.avg(now, server.awake_chunks as f32),
                    view.chunks.len()
                ));
                right_lines.push(format!(
                    "active cells ~{} | regions {:>3}/{:>3} dirty",
                    human_count(windows.awake_cells.avg(now, server.awake_cells as f32) as u64),
                    server.loaded_regions,
                    server.dirty_regions
                ));
            } else {
                right_lines.push(format!(
                    "tick #{} | {:>4} chunks",
                    view.server_tick,
                    view.chunks.len()
                ));
            }

            let mut net = format!(
                "net rx {}/s ({})",
                human_bytes(windows.rx_per_sec.avg(now, rx_per_sec as f32) as u64),
                human_bytes(rx_bytes)
            );
            if embedded {
                net.push_str(&format!(
                    " | tx {}/tick",
                    human_bytes(windows.tx_bytes.avg(now, server.replicated_bytes as f32) as u64)
                ));
            }
            right_lines.push(net);
            right_lines.push(format!(
                "uploads {:>4.0}/s ({}/s) | zoom {:>5.2}x",
                windows.uploads.rate(now, visuals.uploads as f32),
                human_bytes(windows.upload_bytes.rate(now, visuals.upload_bytes as f32) as u64),
                camera.zoom
            ));

            let particle_count = ctx.particles.iter().count();
            if embedded {
                let mem = server.loaded_chunks as u64
                    * CHUNK_AREA as u64
                    * std::mem::size_of::<Cell>() as u64;
                right_lines.push(format!(
                    "players {:>2} | bodies {:>3} | particles {:>4.0}",
                    server.players,
                    server.pixel_bodies,
                    windows.particles.avg(now, particle_count as f32)
                ));
                right_lines.push(format!("mem ~{}", human_bytes(mem)));
            } else {
                right_lines.push(format!(
                    "players {:>2} | particles {:>4.0}",
                    names.0.len(),
                    windows.particles.avg(now, particle_count as f32)
                ));
            }
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

fn moon_name(phase: u32) -> &'static str {
    match phase {
        0 => "new moon",
        1 => "waxing crescent",
        2 => "first quarter",
        3 => "waxing gibbous",
        4 => "full moon",
        5 => "waning gibbous",
        6 => "last quarter",
        _ => "waning crescent",
    }
}

fn phase_label(phase: Phase) -> &'static str {
    match phase {
        Phase::Empty => "empty",
        Phase::Solid => "solid",
        Phase::Powder => "powder",
        Phase::Liquid => "liquid",
        Phase::Gas => "gas",
        Phase::Fire => "fire",
    }
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
