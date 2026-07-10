use super::materials::{LightingMaterial, LightingParams};
use super::{Sky, SkyAssets};
use crate::ClientRegistry;
use crate::camera::CameraState;
use crate::net::Session;
use crate::player::{PlayerVisual, PlayerVisuals};
use crate::worldview::WorldView;
use bevy::prelude::*;
use fallingsand_core::CellPos;

pub(super) const MAX_LIGHTS: usize = 32;
const PLAYER_LIGHT_RADIUS: f32 = 70.0;
const BURNING_LIGHT_RADIUS: f32 = 40.0;
const EMISSIVE_LIGHT_RADIUS: f32 = 28.0;
const EMISSIVE_MERGE_DIST: f32 = 24.0;
const EMISSIVE_MAX_RADIUS: f32 = 60.0;
const EMISSIVE_SCAN_STRIDE: i32 = 8;
const LIGHT_SCAN_INTERVAL: f32 = 0.1;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LightSet;

#[derive(Resource, Default)]
pub struct ActiveLights {
    pub lights: Vec<Vec4>,
    pub darkness: f32,
}

impl ActiveLights {
    pub fn write(&self, params: &mut LightingParams) {
        params.darkness = self.darkness;
        params.light_count = self.lights.len().min(MAX_LIGHTS) as u32;
        let mut array = [Vec4::ZERO; MAX_LIGHTS];
        for (slot, light) in array.iter_mut().zip(self.lights.iter()) {
            *slot = *light;
        }
        params.lights = array;
    }
}

#[derive(Resource, Default)]
pub(super) struct EmissiveLights(pub(super) Vec<Vec4>);

pub(super) fn scan_emissive(
    sky: Res<Sky>,
    real: Res<Time>,
    registry: Res<ClientRegistry>,
    view: Res<WorldView>,
    state: Res<CameraState>,
    mut emissive_lights: ResMut<EmissiveLights>,
    mut cooldown: Local<f32>,
) {
    *cooldown -= real.delta_secs();
    if *cooldown > 0.0 {
        return;
    }
    *cooldown = LIGHT_SCAN_INTERVAL;
    if !sky.synced || sky.darkness() <= 0.001 {
        if !emissive_lights.0.is_empty() {
            emissive_lights.0.clear();
        }
        return;
    }

    let mut lights: Vec<Vec4> = Vec::new();
    let center = state.pos;
    let half = state.view_cells() / 2.0 + 32.0;
    let emissive = registry.0.tag_mask("emissive");
    let min_x =
        ((center.x - half.x) as i32).div_euclid(EMISSIVE_SCAN_STRIDE) * EMISSIVE_SCAN_STRIDE;
    let min_y =
        ((center.y - half.y) as i32).div_euclid(EMISSIVE_SCAN_STRIDE) * EMISSIVE_SCAN_STRIDE;
    let mut y = min_y;
    while y as f32 <= center.y + half.y {
        let mut x = min_x;
        while x as f32 <= center.x + half.x {
            let pos = CellPos::new(x, y);
            if let Some(cell) = view.get_cell(pos)
                && registry.0.has_tag(cell.material, emissive)
            {
                let point = Vec2::new(x as f32, y as f32);
                let mut merged = false;
                for light in lights.iter_mut() {
                    let existing = Vec2::new(light.x, light.y);
                    if existing.distance(point) < EMISSIVE_MERGE_DIST + light.z * 0.5 {
                        let mid = (existing + point) / 2.0;
                        let radius =
                            (light.z + existing.distance(point) * 0.5).min(EMISSIVE_MAX_RADIUS);
                        *light = Vec4::new(mid.x, mid.y, radius, light.w);
                        merged = true;
                        break;
                    }
                }
                if !merged && lights.len() < MAX_LIGHTS - 1 {
                    lights.push(Vec4::new(point.x, point.y, EMISSIVE_LIGHT_RADIUS, 0.9));
                }
            }
            x += EMISSIVE_SCAN_STRIDE;
        }
        y += EMISSIVE_SCAN_STRIDE;
    }
    emissive_lights.0 = lights;
}

pub(super) fn collect_lights(
    sky: Res<Sky>,
    emissive_lights: Res<EmissiveLights>,
    session: Option<Res<Session>>,
    visuals: Res<PlayerVisuals>,
    players: Query<(&Transform, &PlayerVisual)>,
    mut active: ResMut<ActiveLights>,
) {
    active.darkness = if sky.synced { sky.darkness() } else { 0.0 };
    active.lights.clear();
    if active.darkness <= 0.001 {
        return;
    }

    let local = session.and_then(|session| session.player);
    if let Some(id) = local
        && let Some(&entity) = visuals.0.get(&id)
        && let Ok((transform, _)) = players.get(entity)
    {
        active.lights.push(Vec4::new(
            transform.translation.x,
            transform.translation.y,
            PLAYER_LIGHT_RADIUS,
            1.0,
        ));
    }
    for (transform, visual) in &players {
        if visual.burning && active.lights.len() < MAX_LIGHTS {
            active.lights.push(Vec4::new(
                transform.translation.x,
                transform.translation.y,
                BURNING_LIGHT_RADIUS,
                1.0,
            ));
        }
    }
    for light in &emissive_lights.0 {
        if active.lights.len() >= MAX_LIGHTS {
            break;
        }
        active.lights.push(*light);
    }
}

pub(super) fn apply_lighting(
    state: Res<CameraState>,
    active: Res<ActiveLights>,
    assets: Res<SkyAssets>,
    mut materials: ResMut<Assets<LightingMaterial>>,
) {
    let Some(mut material) = materials.get_mut(&assets.lighting) else {
        return;
    };
    active.write(&mut material.params);
    let (snapped, _) = state.layer(Vec2::ZERO);
    material.params.snapped_cam = snapped.as_vec2();
    material.params.native_size = state.native.as_vec2();
}
