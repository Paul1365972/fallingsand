mod game;
mod view;

use bevy::prelude::*;
use bevy::render::error_handler::{ErrorType, RenderError, RenderErrorHandler, RenderErrorPolicy};
use game::ClientGame;
use view::Game;

fn render_error_policy(
    error: &RenderError,
    main_world: &mut World,
    _render_world: &mut World,
) -> RenderErrorPolicy {
    error!("render error: {error:?}");
    match error.ty {
        ErrorType::DeviceLost => RenderErrorPolicy::Recover(default()),
        _ => {
            main_world.write_message(AppExit::error());
            RenderErrorPolicy::StopRendering
        }
    }
}

fn main() {
    let mut client = ClientGame::new();
    if let Some(world) = game::platform::cli_world_name() {
        client.start_game_local(world);
    }

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "fallingsand".into(),
                    fit_canvas_to_parent: true,
                    ..default()
                }),
                ..default()
            })
            .set(ImagePlugin::default_nearest()),
    )
    .add_plugins((
        view::render::GameplayRendererPlugin,
        view::ui::debug::DiagnosticsPlugin,
    ))
    .insert_resource(ClearColor(Color::srgb(0.08, 0.09, 0.13)))
    .insert_resource(RenderErrorHandler(render_error_policy))
    .insert_resource(Game(client))
    .init_resource::<view::io::UiInbox>()
    .init_resource::<view::players::NametagVisuals>()
    .add_systems(
        Startup,
        (
            view::ui::debug::setup_overlay,
            view::ui::icons::load_item_icons,
        ),
    )
    .add_systems(
        Update,
        (view::io::collect_ui_events, view::io::drive_game).chain(),
    )
    .add_systems(
        Update,
        view::players::sync_nametags.after(view::render::GameplayRenderSet::Camera),
    )
    .add_systems(
        Update,
        (
            view::ui::menu::sync_menu,
            view::ui::game_menu::sync_game_menu,
            view::ui::settings::sync_settings,
            view::ui::connscreen::sync_connscreen,
            (
                view::ui::hud::sync_hud,
                view::ui::hud::patch_hud_slots,
                view::ui::hud::hud_status,
                view::ui::hud::sync_cursor_hud,
                view::ui::hud::sync_death_screen,
            ),
            view::ui::inventory::sync_overlay,
            view::ui::inventory::patch_overlay_slots,
            view::ui::inventory::sync_craftable,
            view::ui::inventory::update_cursor_follow,
            view::ui::inventory::update_tooltip,
            view::ui::chat::sync_chat,
            view::ui::chat::fade_chat,
            view::ui::button_hover,
        )
            .after(view::io::drive_game),
    );
    app.add_systems(
        Update,
        view::ui::debug::update_overlay.after(view::render::GameplayRenderSet::Prepared),
    );
    #[cfg(not(target_family = "wasm"))]
    app.add_systems(Update, view::icon::set_window_icons);
    app.run();
}
