use super::Sky;
use super::materials::{LightingMaterial, LightingParams};
use crate::view::Game;
use crate::view::camera::{CameraState, LayerAssets, light_field_margin};
use bevy::log::warn_once;
use bevy::prelude::*;

pub const MAX_PLAYER_LIGHTS: usize = 256;
const PLAYER_LIGHT_RADIUS: f32 = 40.0;
const BURNING_LIGHT_RADIUS: f32 = 64.0;

#[derive(Resource, Default)]
pub struct ActiveLights {
    pub lights: Vec<Vec4>,
    pub darkness: f32,
}

impl ActiveLights {
    pub fn write(&self, params: &mut LightingParams) {
        params.darkness = self.darkness;
        if self.lights.len() > MAX_PLAYER_LIGHTS {
            warn_once!(
                "dropping {} point lights over capacity {MAX_PLAYER_LIGHTS}",
                self.lights.len() - MAX_PLAYER_LIGHTS
            );
        }
        params.light_count = self.lights.len().min(MAX_PLAYER_LIGHTS) as u32;
        params.lights = [Vec4::ZERO; MAX_PLAYER_LIGHTS];
        for (slot, light) in params.lights.iter_mut().zip(self.lights.iter()) {
            *slot = *light;
        }
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

    let Some(material) = materials.get(&assets.lighting) else {
        return;
    };
    let mut params = material.params.clone();
    active.write(&mut params);
    let (snapped, _) = state.layer(Vec2::ZERO);
    params.snapped_cam = snapped.as_vec2();
    params.native_size = state.native.as_vec2();
    params.margin = light_field_margin(state.native);
    let changed = material.params != params;
    if changed && let Some(mut material) = materials.get_mut(&assets.lighting) {
        material.params = params;
    }
}
