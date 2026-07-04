use crate::ClientRegistry;
use crate::net::{EmbeddedServerStats, NetStats};
use crate::player::{Hotbar, InputState};
use crate::render::ChunkVisuals;
use crate::worldview::WorldView;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};

pub struct DebugOverlayPlugin;

#[derive(Component)]
pub struct DebugText;

#[derive(Resource, Default)]
pub struct DebugVisible(pub bool);

impl Plugin for DebugOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .insert_resource(DebugVisible(true))
            .add_systems(Startup, setup_overlay)
            .add_systems(Update, (toggle_overlay, update_overlay, screenshot));
    }
}

fn screenshot(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut auto_taken: Local<bool>,
) {
    let auto =
        !*auto_taken && std::env::var_os("FS_AUTOSHOT").is_some() && time.elapsed_secs() > 8.0;
    if keys.just_pressed(KeyCode::F2) || auto {
        *auto_taken = true;
        let path = format!("screenshot-{}.png", time.elapsed_secs() as u32);
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

fn toggle_overlay(keys: Res<ButtonInput<KeyCode>>, mut visible: ResMut<DebugVisible>) {
    if keys.just_pressed(KeyCode::F3) {
        visible.0 = !visible.0;
    }
}

#[allow(clippy::too_many_arguments)]
fn update_overlay(
    visible: Res<DebugVisible>,
    diagnostics: Res<DiagnosticsStore>,
    server: Res<EmbeddedServerStats>,
    net: Res<NetStats>,
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

    ***text = format!(
        "fps: {fps:.0}\n\
         server tick: {} ({} us sim)\n\
         chunks: {} loaded / {} awake (server), {} client, {} uploads\n\
         net rx: {}/s ({} total), server tx: {}/tick\n\
         players: {}, pixel bodies: {}\n\
         cursor: {},{} [{}]\n\
         selected [1-9]: {}\n\
         keys: AD move, space jump, LMB dig, RMB place, wheel zoom, IJKL pan, O follow, esc pause",
        server.tick,
        server.sim_micros,
        server.loaded_chunks,
        server.awake_chunks,
        view.chunks.len(),
        visuals.uploads,
        human_bytes(net.rx_per_sec),
        human_bytes(net.rx_bytes),
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
