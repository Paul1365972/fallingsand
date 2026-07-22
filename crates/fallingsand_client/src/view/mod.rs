pub mod camera;
#[cfg(not(target_family = "wasm"))]
pub mod icon;
pub mod io;
pub mod players;
pub mod render;
pub mod ui;

use crate::game::ClientGame;
use bevy::prelude::*;

#[derive(Resource)]
pub struct Game(pub ClientGame);

pub struct ViewPlugin;

impl Plugin for ViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((render::GameplayRendererPlugin, ui::debug::DiagnosticsPlugin))
            .insert_resource(ClearColor(Color::srgb(0.08, 0.09, 0.13)))
            .init_resource::<io::UiInbox>()
            .init_resource::<players::NametagVisuals>()
            .add_systems(
                Startup,
                (ui::debug::setup_overlay, ui::icons::load_item_icons),
            )
            .add_systems(Update, (io::collect_ui_events, io::drive_game).chain())
            .add_systems(
                Update,
                players::sync_nametags.after(render::GameplayRenderSet::Camera),
            )
            .add_systems(
                Update,
                (
                    ui::menu::sync_menu,
                    ui::game_menu::sync_game_menu,
                    ui::settings::sync_settings,
                    ui::connscreen::sync_connscreen,
                    (
                        ui::hud::sync_hud,
                        ui::hud::patch_hud_slots,
                        ui::hud::hud_status,
                        ui::hud::sync_cursor_hud,
                        ui::hud::sync_death_screen,
                    ),
                    ui::inventory::sync_overlay,
                    ui::inventory::patch_overlay_slots,
                    ui::inventory::sync_craftable,
                    ui::inventory::update_cursor_follow,
                    ui::inventory::update_tooltip,
                    ui::chat::sync_chat,
                    ui::chat::fade_chat,
                    ui::button_hover,
                )
                    .after(io::drive_game),
            )
            .add_systems(
                Update,
                ui::debug::update_overlay.after(render::GameplayRenderSet::Prepared),
            );
        #[cfg(not(target_family = "wasm"))]
        app.add_systems(Update, icon::set_window_icons);
    }
}
