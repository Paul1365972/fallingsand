pub mod camera;
#[cfg(not(target_family = "wasm"))]
pub mod icon;
pub mod io;
pub mod players;
pub mod render;
pub mod ui;

use crate::game::ClientGame;
use bevy::prelude::Resource;

#[derive(Resource)]
pub struct Game(pub ClientGame);
