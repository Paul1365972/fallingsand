pub mod camera;
pub mod chunks;
#[cfg(not(target_family = "wasm"))]
pub mod icon;
pub mod io;
pub mod parallax;
pub mod particles;
pub mod players;
pub mod sky;
pub mod ui;

use crate::game::ClientGame;
use bevy::math::Vec2;
use bevy::prelude::Resource;

pub const PLAYER_SIZE: Vec2 = Vec2::new(3.0, 9.0);
pub const PLAYER_DUCK_SIZE: Vec2 = Vec2::new(3.0, 5.0);

#[derive(Resource)]
pub struct Game(pub ClientGame);
