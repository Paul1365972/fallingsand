use crate::ClientRegistry;
use crate::net::{EmbeddedServerStats, ServerMsg, Session};
use crate::player::{Hotbar, InputState};
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

#[derive(Resource, Default)]
struct DeltaFlashes(Vec<(ChunkPos, DirtyRect, f32)>);

const FLASH_SECS: f32 = 0.4;

impl Plugin for DebugOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .insert_resource(DebugVisible(true))
            .init_resource::<BordersVisible>()
            .init_resource::<F3ComboUsed>()
            .init_resource::<DeltaFlashes>()
            .add_systems(Startup, setup_overlay)
            .add_systems(Update, (toggle_overlay, update_overlay, screenshot))
            .add_systems(
                Update,
                (track_deltas, draw_borders)
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
) {
    if keys.pressed(KeyCode::F3) && keys.just_pressed(KeyCode::KeyG) {
        borders.0 = !borders.0;
        combo.0 = true;
    }
    if keys.just_released(KeyCode::F3) {
        if !combo.0 {
            visible.0 = !visible.0;
        }
        combo.0 = false;
    }
}

fn track_deltas(
    mut flashes: ResMut<DeltaFlashes>,
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
    flashes.0.retain(|(_, _, at)| now - at < FLASH_SECS);
    for ServerMsg(message) in messages.read() {
        if let fallingsand_protocol::ServerMessage::ChunkDelta { pos, rect, .. } = message {
            flashes.0.push((*pos, *rect, now));
        }
    }
}

fn draw_borders(
    borders: Res<BordersVisible>,
    flashes: Res<DeltaFlashes>,
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
    for (pos, rect, at) in &flashes.0 {
        let alpha = (1.0 - (now - at) / FLASH_SECS).clamp(0.0, 1.0) * 0.8;
        let origin = Vec2::new(pos.x as f32 * chunk, pos.y as f32 * chunk);
        let corner = origin + Vec2::new(rect.min_x as f32, rect.min_y as f32);
        let size = Vec2::new(rect.width() as f32, rect.height() as f32);
        gizmos.rect_2d(
            Isometry2d::from_translation(corner + size / 2.0),
            size,
            Color::srgba(1.0, 0.9, 0.2, alpha),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn update_overlay(
    visible: Res<DebugVisible>,
    diagnostics: Res<DiagnosticsStore>,
    server: Res<EmbeddedServerStats>,
    session: Option<Res<Session>>,
    view: Res<WorldView>,
    visuals: Res<ChunkVisuals>,
    hotbar: Res<Hotbar>,
    input: Res<InputState>,
    registry: Res<ClientRegistry>,
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
    let selected = hotbar
        .materials
        .get(hotbar.selected)
        .map(|&id| registry.0.get(id).name.as_str())
        .unwrap_or("none");
    let cursor_material = view
        .get_cell(input.aim)
        .map(|cell| registry.0.get(cell.material).name.as_str())
        .unwrap_or("unloaded");
    let (rx_per_sec, rx_bytes) = session
        .map(|session| (session.rx_per_sec, session.rx_bytes))
        .unwrap_or((0, 0));

    ***text = format!(
        "fps: {fps:.0}\n\
         server tick: {} ({} us sim)\n\
         chunks: {} loaded / {} active / {} border / {} awake (server), {} client, {} uploads ({})\n\
         net rx: {}/s ({} total), server tx: {}/tick\n\
         players: {}, pixel bodies: {}\n\
         cursor: {},{} [{}]\n\
         selected [1-9]: {}\n\
         keys: AD move, space jump, LMB dig, RMB place, wheel zoom, IJKL pan, O follow, F3+G borders, esc pause",
        server.tick,
        server.sim_micros,
        server.loaded_chunks,
        server.active_chunks,
        server.border_chunks,
        server.awake_chunks,
        view.chunks.len(),
        visuals.uploads,
        human_bytes(visuals.upload_bytes as u64),
        human_bytes(rx_per_sec),
        human_bytes(rx_bytes),
        human_bytes(server.replicated_bytes),
        server.players,
        server.pixel_bodies,
        input.aim.x,
        input.aim.y,
        cursor_material,
        selected,
    );
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
