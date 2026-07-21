use super::Sky;
use crate::view::Game;
use crate::view::camera::{CameraState, light_field_margin};
use bevy::log::warn_once;
use bevy::prelude::*;

pub const MAX_PLAYER_LIGHTS: usize = 256;
const PLAYER_LIGHT_RADIUS: f32 = 40.0;
const BURNING_LIGHT_RADIUS: f32 = 64.0;

#[derive(Resource, Default, Clone)]
pub struct ActiveLights {
    pub lights: Vec<Vec4>,
    pub params: super::materials::LightingParams,
}

pub fn apply_lighting(
    game: Res<Game>,
    sky: Res<Sky>,
    state: Res<CameraState>,
    mut active: ResMut<ActiveLights>,
) {
    active.lights.clear();
    let darkness = if sky.synced { sky.darkness() } else { 0.0 };
    if darkness > 0.001
        && let Some(ingame) = game.0.ingame()
    {
        if let Some(local) = ingame.local_avatar() {
            let radius = if local.burning {
                BURNING_LIGHT_RADIUS
            } else {
                PLAYER_LIGHT_RADIUS
            };
            active
                .lights
                .push(Vec4::new(local.pos.x, local.pos.y, radius, 1.0));
        }
        let local = ingame
            .net
            .session
            .as_ref()
            .and_then(|session| session.player());
        for (&player, remote) in &ingame.players.avatars {
            if Some(player) != local && remote.burning {
                active.lights.push(Vec4::new(
                    remote.pos.x,
                    remote.pos.y,
                    BURNING_LIGHT_RADIUS,
                    1.0,
                ));
            }
        }
    }
    if active.lights.len() > MAX_PLAYER_LIGHTS {
        warn_once!(
            "dropping {} point lights over capacity {MAX_PLAYER_LIGHTS}",
            active.lights.len() - MAX_PLAYER_LIGHTS
        );
    }
    let mut params = super::materials::LightingParams {
        darkness,
        light_count: active.lights.len().min(MAX_PLAYER_LIGHTS) as u32,
        ..default()
    };
    for (slot, light) in params.lights.iter_mut().zip(active.lights.iter()) {
        *slot = *light;
    }
    let (snapped, _) = state.layer(Vec2::ZERO);
    params.snapped_cam = snapped.as_vec2();
    params.margin = light_field_margin(state.native);
    active.params = params;
}
