use crate::ClientRegistry;
use crate::net::{EmbeddedServerStats, ServerMsg, Session, Supervisor};
use crate::player::{Hotbar, InputState, PlayerNames};
use crate::render::ChunkVisuals;
use crate::worldview::WorldView;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use fallingsand_core::{CHUNK_SIZE, ChunkPos, DirtyRect, REGION_SIZE_CELLS};

pub struct DebugOverlayPlugin;

#[derive(Component)]
pub struct DebugText;

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

impl Plugin for DebugOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .insert_resource(DebugVisible(true))
            .init_resource::<BordersVisible>()
            .init_resource::<F3ComboUsed>()
            .init_resource::<RectFlashes>()
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
    commands.spawn((
        DebugText,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(13.0),
            ..default()
        },
        TextColor(Color::srgba(0.9, 0.95, 1.0, 0.95)),
        Node {
            position_type: PositionType::Absolute,
            top: px(5),
            left: px(5),
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
    camera: Single<(&Camera, &GlobalTransform)>,
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

#[allow(clippy::too_many_arguments)]
fn update_overlay(
    visible: Res<DebugVisible>,
    diagnostics: Res<DiagnosticsStore>,
    game_state: Option<Res<State<crate::GameState>>>,
    supervisor: Res<Supervisor>,
    server: Res<EmbeddedServerStats>,
    session: Option<Res<Session>>,
    view: Res<WorldView>,
    visuals: Res<ChunkVisuals>,
    names: Res<PlayerNames>,
    hotbar: Res<Hotbar>,
    input: Res<InputState>,
    registry: Res<ClientRegistry>,
    mode: Res<crate::player::LocalMode>,
    fly: Res<crate::player::FlyToggle>,
    mut text: Single<&mut Text, With<DebugText>>,
) {
    if !visible.0 {
        if !text.is_empty() {
            text.clear();
        }
        return;
    }

    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let game_state = game_state.map(|state| *state.get());
    let embedded = supervisor.target.is_none();
    let (rx_per_sec, rx_bytes) = session
        .map(|session| (session.rx_per_sec, session.rx_bytes))
        .unwrap_or((0, 0));

    let mut lines = vec![
        format!("fps: {fps:.0}"),
        format!("fallingsand v{}", env!("CARGO_PKG_VERSION")),
    ];
    match game_state {
        None => {}
        Some(crate::GameState::Connecting) => {
            let target = supervisor
                .target
                .as_ref()
                .map(|target| target.url.as_str())
                .unwrap_or("local server");
            let mut conn = format!("conn: {target}, attempt {}", supervisor.attempt);
            if let Some(err) = &supervisor.last_error {
                conn.push_str(&format!(", last error: {err}"));
            }
            lines.push(conn);
            lines.push(format!(
                "net rx: {}/s ({} total)",
                human_bytes(rx_per_sec),
                human_bytes(rx_bytes)
            ));
        }
        Some(crate::GameState::Playing) => {
            let mut tick = format!("tick: {}", view.server_tick);
            if embedded {
                tick.push_str(&format!(" ({} us sim)", server.sim_micros));
            }
            lines.push(tick);

            let mut chunks = "chunks: ".to_string();
            if embedded {
                chunks.push_str(&format!(
                    "{} loaded / {} active / {} border / {} awake (server), ",
                    server.loaded_chunks,
                    server.active_chunks,
                    server.border_chunks,
                    server.awake_chunks,
                ));
            }
            chunks.push_str(&format!(
                "{} client, {} uploads ({})",
                view.chunks.len(),
                visuals.uploads,
                human_bytes(visuals.upload_bytes as u64)
            ));
            lines.push(chunks);

            let mut net = format!(
                "net rx: {}/s ({} total)",
                human_bytes(rx_per_sec),
                human_bytes(rx_bytes)
            );
            if embedded {
                net.push_str(&format!(
                    ", server tx: {}/tick",
                    human_bytes(server.replicated_bytes)
                ));
            }
            lines.push(net);

            let mut population = if embedded {
                format!("players: {}", server.players)
            } else {
                format!("players: {}", names.0.len())
            };
            if embedded {
                population.push_str(&format!(", pixel bodies: {}", server.pixel_bodies));
            }
            lines.push(population);

            let cursor_material = view
                .get_cell(input.aim)
                .map(|cell| registry.0.get(cell.material).name.as_str())
                .unwrap_or("unloaded");
            lines.push(format!(
                "cursor: {},{} [{}]",
                input.aim.x, input.aim.y, cursor_material
            ));
            let selected = registry
                .0
                .try_get(hotbar.selected)
                .map(|material| material.name.as_str())
                .unwrap_or("none");
            lines.push(format!("selected [1-0, brackets]: {selected}"));
            let fly = if fly.0 { ", flying" } else { "" };
            lines.push(format!("mode: {}{fly} (F3+N switch)", mode.0.label()));
            lines.push(
                "keys: AD move, space jump (2x fly), LMB dig, RMB place, wheel zoom, F3+G borders+rects, esc pause"
                    .to_string(),
            );
        }
    }
    let joined = lines.join("\n");
    if ***text != joined {
        ***text = joined;
    }
}

fn human_bytes(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MiB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
