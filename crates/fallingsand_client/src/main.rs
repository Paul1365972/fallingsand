mod camera;
mod chat;
mod connscreen;
mod debug;
mod hud;
#[cfg(not(target_family = "wasm"))]
mod icon;
mod identity;
mod interpolation;
mod inventory;
mod inventory_ui;
mod menu;
mod net;
mod particles;
mod pause;
mod player;
mod render;
mod settings;
mod sky;
mod worldview;

use bevy::prelude::*;
use fallingsand_core::{ItemRegistry, MaterialRegistry, RecipeRegistry};
use std::sync::Arc;

pub const MATERIALS_RON: &str = include_str!("../../../data/materials.ron");
pub const REACTIONS_RON: &str = include_str!("../../../data/reactions.ron");
pub const ITEMS_RON: &str = include_str!("../../../data/items.ron");
pub const RECIPES_RON: &str = include_str!("../../../data/recipes.ron");

#[derive(States, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    MainMenu,
    InGame,
}

#[derive(SubStates, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[source(AppState = AppState::InGame)]
pub enum GameState {
    #[default]
    Connecting,
    Playing,
}

#[derive(SubStates, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[source(GameState = GameState::Playing)]
pub enum PauseState {
    #[default]
    Running,
    Paused,
}

#[derive(Resource, Clone)]
pub struct ClientRegistry(pub Arc<MaterialRegistry>);

#[derive(Resource, Clone)]
pub struct ClientItemRegistry(pub Arc<ItemRegistry>);

#[derive(Resource, Clone)]
pub struct ClientRecipes(pub Arc<RecipeRegistry>);

fn main() {
    let registry = Arc::new(
        MaterialRegistry::from_ron(MATERIALS_RON, REACTIONS_RON)
            .expect("data/materials.ron must be valid"),
    );
    let item_registry = Arc::new(
        ItemRegistry::from_ron(ITEMS_RON, &registry).expect("data/items.ron must be valid"),
    );
    let recipes = Arc::new(
        RecipeRegistry::from_ron(RECIPES_RON, &item_registry)
            .expect("data/recipes.ron must be valid"),
    );
    let world_name = net::cli_world_name();
    let initial_state = if world_name.is_some() {
        AppState::InGame
    } else {
        AppState::MainMenu
    };

    let mut app = App::new();
    if let Some(name) = world_name {
        app.insert_resource(menu::SelectedWorld(name));
    }
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
    .insert_resource(ClearColor(Color::srgb(0.08, 0.09, 0.13)))
    .insert_resource(ClientRegistry(registry))
    .insert_resource(ClientItemRegistry(item_registry))
    .insert_resource(ClientRecipes(recipes))
    .insert_state(initial_state)
    .add_sub_state::<GameState>()
    .add_sub_state::<PauseState>()
    .add_plugins((
        net::NetPlugin,
        render::ChunkRenderPlugin,
        worldview::WorldViewPlugin,
        interpolation::InterpolationPlugin,
        player::PlayerPlugin,
        camera::CameraPlugin,
        debug::DebugOverlayPlugin,
    ))
    .add_plugins((
        menu::MenuPlugin,
        pause::PausePlugin,
        hud::HudPlugin,
        chat::ChatPlugin,
        particles::ParticlesPlugin,
        sky::SkyPlugin,
        connscreen::ConnScreenPlugin,
        settings::SettingsPlugin,
        inventory::InventoryPlugin,
        inventory_ui::InventoryUiPlugin,
    ));
    #[cfg(not(target_family = "wasm"))]
    app.add_plugins(icon::IconPlugin);
    app.run();
}
