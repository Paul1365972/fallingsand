mod borders;
mod stats;

use crate::ClientRegistry;
use crate::camera::{CameraState, RenderMode, WORLD_LAYER};
use crate::input::{InputHeld, LocalAction};
use crate::inventory::{LocalInventory, SelectedSlot};
use crate::net::{ServerStats, Session, Supervisor};
use crate::particles::Particle;
use crate::player::{LocalPlayerState, PlayerNames};
use crate::render::ChunkVisuals;
use crate::sky::{Sky, WorldTime};
use crate::worldview::WorldView;
use bevy::camera::visibility::RenderLayers;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use borders::{BordersVisible, RectFlashes, draw_borders, sync_debug_stream, track_rects};
use fallingsand_core::{CHUNK_AREA, Cell, ChunkPos, MAX_HP, Phase, SEASON_DAYS};
use stats::{StatWindows, human_bytes, human_count};

pub struct DebugOverlayPlugin;

const BUDGET_MS: f32 = 1000.0 / 60.0;

#[derive(Component)]
struct DebugTextLeft;

#[derive(Component)]
struct DebugTextRight;

#[derive(Resource, Default)]
struct DebugVisible(bool);

impl Plugin for DebugOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .insert_gizmo_config(
                DefaultGizmoConfigGroup,
                GizmoConfig {
                    render_layers: RenderLayers::layer(WORLD_LAYER),
                    ..default()
                },
            )
            .insert_resource(DebugVisible(true))
            .init_resource::<BordersVisible>()
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

fn screenshot(mut commands: Commands, mut actions: MessageReader<LocalAction>) {
    for action in actions.read() {
        if *action != LocalAction::Screenshot {
            continue;
        }
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
    mut actions: MessageReader<LocalAction>,
    mut visible: ResMut<DebugVisible>,
    mut borders: ResMut<BordersVisible>,
    state: Res<crate::player::LocalPlayerState>,
    mut session: Option<ResMut<Session>>,
) {
    for action in actions.read() {
        match action {
            LocalAction::ToggleDebugOverlay => visible.0 = !visible.0,
            LocalAction::ToggleDebugBorders => borders.0 = !borders.0,
            LocalAction::ToggleGameMode => {
                if let Some(session) = session.as_mut()
                    && session.player.is_some()
                {
                    let target = match state.mode {
                        fallingsand_protocol::GameMode::Creative => "s",
                        fallingsand_protocol::GameMode::Survival => "c",
                    };
                    session.send(&fallingsand_protocol::ClientMessage::Chat {
                        text: format!("/gm {target}"),
                    });
                }
            }
            _ => {}
        }
    }
}

#[derive(SystemParam)]
struct Overlay<'w, 's> {
    diagnostics: Res<'w, DiagnosticsStore>,
    supervisor: Res<'w, Supervisor>,
    server: Res<'w, ServerStats>,
    session: Option<Res<'w, Session>>,
    view: Res<'w, WorldView>,
    visuals: Res<'w, ChunkVisuals>,
    names: Res<'w, PlayerNames>,
    selected: Res<'w, SelectedSlot>,
    inventory: Res<'w, LocalInventory>,
    item_reg: Res<'w, crate::ClientItemRegistry>,
    held: Res<'w, InputHeld>,
    registry: Res<'w, ClientRegistry>,
    world_time: Res<'w, WorldTime>,
    celestial: Res<'w, Sky>,
    player: Res<'w, LocalPlayerState>,
    camera: Res<'w, CameraState>,
    render_mode: Res<'w, RenderMode>,
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
        format!("{fps:>3.0} fps {frame_ms:>5.1} ms ({frame_min:>5.1}/{frame_max:>5.1})"),
    ];

    match game_state.map(|state| *state.get()) {
        None => {}
        Some(crate::GameState::Connecting) => {
            connecting_lines(&ctx, &mut left_lines, &mut right_lines);
        }
        Some(crate::GameState::Playing) => {
            playing_lines(&ctx, &mut windows, now, &mut left_lines, &mut right_lines);
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

fn rx_stats(ctx: &Overlay) -> (u64, u64) {
    ctx.session
        .as_ref()
        .map(|session| (session.rx_per_sec, session.rx_bytes))
        .unwrap_or((0, 0))
}

fn connecting_lines(ctx: &Overlay, left_lines: &mut Vec<String>, right_lines: &mut Vec<String>) {
    let (rx_per_sec, rx_bytes) = rx_stats(ctx);
    let target = ctx
        .supervisor
        .target
        .as_ref()
        .map(|target| target.url.as_str())
        .unwrap_or("local server");
    let mut conn = format!("connecting: {target}, attempt {}", ctx.supervisor.attempt);
    if let Some(err) = &ctx.supervisor.last_error {
        conn.push_str(&format!("\nlast error: {err}"));
    }
    left_lines.push(conn);
    right_lines.push(format!(
        "net rx {}/s ({})",
        human_bytes(rx_per_sec),
        human_bytes(rx_bytes)
    ));
}

fn playing_lines(
    ctx: &Overlay,
    windows: &mut StatWindows,
    now: f32,
    left_lines: &mut Vec<String>,
    right_lines: &mut Vec<String>,
) {
    let server = &ctx.server;
    let view = &ctx.view;
    let player = &ctx.player;
    let world_time = &ctx.world_time;
    let embedded = ctx.supervisor.target.is_none();
    let (rx_per_sec, rx_bytes) = rx_stats(ctx);

    let aim = ctx.held.0.aim;
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
        "day {} {} {}/{} {:02}:{:02} {}{}",
        world_time.calendar.day(),
        world_time.calendar.season().label(),
        world_time.calendar.day_of_year() as u64 % SEASON_DAYS + 1,
        SEASON_DAYS,
        minute_of_day / 60,
        minute_of_day % 60,
        moon_name(world_time.moon_phase()),
        eclipse
    ));

    if player.present {
        let burning = if player.burning { " burning" } else { "" };
        left_lines.push(String::new());
        left_lines.push(format!(
            "hp {:>3.0}/{:.0} air {:>4.1}s{}",
            player.hp, MAX_HP, player.air, burning
        ));
        left_lines.push(format!(
            "pos {:.1},{:.1} {}",
            player.pos.x,
            player.pos.y,
            player.mode.label(),
        ));
    }

    left_lines.push(String::new());
    let cursor = match view.get_cell(aim) {
        Some(cell) => match ctx.registry.0.try_get(cell.material) {
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
        "uploads {:>4.0}/s ({}/s) | {}px/cell {}",
        windows.uploads.rate(now, ctx.visuals.uploads as f32),
        human_bytes(
            windows
                .upload_bytes
                .rate(now, ctx.visuals.upload_bytes as f32) as u64
        ),
        ctx.camera.k,
        ctx.render_mode.label()
    ));

    let particle_count = ctx.particles.iter().count();
    if embedded {
        let mem =
            server.loaded_chunks as u64 * CHUNK_AREA as u64 * std::mem::size_of::<Cell>() as u64;
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
            ctx.names.0.len(),
            windows.particles.avg(now, particle_count as f32)
        ));
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
