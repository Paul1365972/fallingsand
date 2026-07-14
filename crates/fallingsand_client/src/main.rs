mod game;
mod view;

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy::render::error_handler::{ErrorType, RenderError, RenderErrorHandler, RenderErrorPolicy};
use bevy::sprite_render::Material2dPlugin;
use game::ClientGame;
use view::Game;

fn render_error_policy(
    error: &RenderError,
    main_world: &mut World,
    _render_world: &mut World,
) -> RenderErrorPolicy {
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
    if let Some(world) = game::net::cli_world_name() {
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
        Material2dPlugin::<view::chunks::ChunkMaterial>::default(),
        Material2dPlugin::<view::sky::LightingMaterial>::default(),
        Material2dPlugin::<view::camera::UpscaleMaterial>::default(),
        Material2dPlugin::<view::sky::SunMaterial>::default(),
        Material2dPlugin::<view::sky::MoonMaterial>::default(),
        Material2dPlugin::<view::sky::StarfieldMaterial>::default(),
        Material2dPlugin::<view::sky::AtmosphereMaterial>::default(),
        Material2dPlugin::<view::parallax::CaveWallMaterial>::default(),
        Material2dPlugin::<view::parallax::SilhouetteMaterial>::default(),
        FrameTimeDiagnosticsPlugin::default(),
    ))
    .insert_resource(ClearColor(Color::srgb(0.08, 0.09, 0.13)))
    .insert_resource(RenderErrorHandler(render_error_policy))
    .insert_resource(Game(client))
    .init_resource::<view::io::UiInbox>()
    .init_resource::<view::chunks::ChunkVisuals>()
    .init_resource::<view::chunks::ChunkUploadQueue>()
    .init_resource::<view::players::NametagVisuals>()
    .init_resource::<view::sky::Sky>()
    .init_resource::<view::sky::ActiveLights>()
    .init_resource::<view::sky::EmissiveLights>()
    .init_resource::<view::ui::debug::StatWindows>()
    .add_systems(
        Startup,
        (
            (view::chunks::setup_shared, view::camera::setup_camera).chain(),
            view::sky::load_shared_shaders,
            view::ui::debug::setup_overlay,
            view::ui::icons::load_item_icons,
        ),
    )
    .add_systems(
        PostStartup,
        (view::sky::setup_sky, view::parallax::setup_parallax),
    )
    .add_systems(
        Update,
        (view::io::collect_ui_events, view::io::drive_game).chain(),
    )
    .add_systems(
        Update,
        (
            view::camera::sync_camera,
            view::camera::resize_targets,
            view::camera::rebind_targets,
            view::sky::sync_sky,
            view::sky::scan_emissive,
            view::sky::apply_lighting,
            view::parallax::sync_parallax,
            view::players::sync_nametags,
            view::ui::debug::draw_debug_borders,
        )
            .chain()
            .after(view::io::drive_game),
    )
    .add_systems(
        Update,
        (
            view::chunks::sync_chunks,
            view::particles::drain_particles,
            view::particles::sync_target,
            view::particles::update_particles,
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
            view::ui::debug::update_overlay,
            view::ui::button_hover,
        )
            .after(view::io::drive_game),
    );
    #[cfg(not(target_family = "wasm"))]
    app.add_systems(Update, view::icon::set_window_icons);
    view::chunks::setup_render_app(&mut app);
    app.run();
}
