mod game;
mod view;

use bevy::prelude::*;
use bevy::render::error_handler::{ErrorType, RenderError, RenderErrorHandler, RenderErrorPolicy};
use game::ClientGame;
use view::{Game, ViewPlugin};

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

    App::new()
        .add_plugins(
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
        .add_plugins(ViewPlugin)
        .insert_resource(RenderErrorHandler(render_error_policy))
        .insert_resource(Game(client))
        .run();
}
