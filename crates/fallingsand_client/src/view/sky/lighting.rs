use super::Sky;
use super::materials::{LightingMaterial, LightingParams};
use crate::view::Game;
use crate::view::camera::{CameraState, LayerAssets};
use bevy::prelude::*;

pub const MAX_PLAYER_LIGHTS: usize = 32;
const PLAYER_LIGHT_RADIUS: f32 = 70.0;
const BURNING_LIGHT_RADIUS: f32 = 40.0;

#[derive(Resource, Default)]
pub struct ActiveLights {
    pub lights: Vec<Vec4>,
    pub darkness: f32,
}

impl ActiveLights {
    pub fn write(&self, params: &mut LightingParams) {
        params.darkness = self.darkness;
        params.light_count = self.lights.len().min(MAX_PLAYER_LIGHTS) as u32;
        let mut array = [Vec4::ZERO; MAX_PLAYER_LIGHTS];
        for (slot, light) in array.iter_mut().zip(self.lights.iter()) {
            *slot = *light;
        }
        params.lights = array;
    }
}

pub fn apply_lighting(
    game: Res<Game>,
    sky: Res<Sky>,
    state: Res<CameraState>,
    assets: Res<LayerAssets>,
    mut active: ResMut<ActiveLights>,
    mut materials: ResMut<Assets<LightingMaterial>>,
) {
    active.darkness = if sky.synced { sky.darkness() } else { 0.0 };
    active.lights.clear();
    if active.darkness > 0.001
        && let Some(ingame) = game.0.ingame()
    {
        if let Some(local) = ingame.local_avatar() {
            active.lights.push(Vec4::new(
                local.pos.x,
                local.pos.y,
                PLAYER_LIGHT_RADIUS,
                1.0,
            ));
            if local.burning {
                active.lights.push(Vec4::new(
                    local.pos.x,
                    local.pos.y,
                    BURNING_LIGHT_RADIUS,
                    1.0,
                ));
            }
        }
        let local = ingame
            .net
            .session
            .as_ref()
            .and_then(|session| session.player());
        for (&player, remote) in &ingame.players.avatars {
            if Some(player) == local {
                continue;
            }
            if remote.burning {
                active.lights.push(Vec4::new(
                    remote.pos.x,
                    remote.pos.y,
                    BURNING_LIGHT_RADIUS,
                    1.0,
                ));
            }
        }
    }

    let Some(mut material) = materials.get_mut(&assets.lighting) else {
        return;
    };
    active.write(&mut material.params);
    let (snapped, _) = state.layer(Vec2::ZERO);
    material.params.snapped_cam = snapped.as_vec2();
    material.params.native_size = state.native.as_vec2();
}
