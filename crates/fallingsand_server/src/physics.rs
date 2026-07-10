use crate::player::{
    Air, Burning, Control, Health, Mode, PLAYER_HALF_H, PLAYER_HALF_W, PLAYER_MASS, Player,
    PlayerActor,
};
use crate::{MAX_AIR_SECS, MAX_HP, Registry, SimObstacles, SimWorld, SpawnPoint};
use bevy_ecs::prelude::*;
use fallingsand_core::Fixed;
use fallingsand_protocol::GameMode;
use fallingsand_sim::physics::{
    Actor, BOUNCE_MIN_SPEED, Controller, PlayerParams, StepInput, scatter_powder, step_player,
};

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub fn step_physics(
    mut sim: ResMut<SimWorld>,
    registry: Res<Registry>,
    obstacles: Res<SimObstacles>,
    spawn_point: Res<SpawnPoint>,
    mut bodies: ResMut<crate::bodies::PixelBodies>,
    mut impulses: ResMut<crate::PlayerImpulses>,
    mut crushes: ResMut<crate::hazards::CrushEvents>,
    mut query: Query<(
        Entity,
        &mut Player,
        &Mode,
        &mut PlayerActor,
        &mut Control,
        &mut Health,
        &mut Air,
        &mut Burning,
    )>,
) {
    let params = PlayerParams::default();
    for (entity, mut player, mode, mut body, mut control, mut health, mut air, mut burning) in
        &mut query
    {
        if let Some((jx, jy)) = impulses.0.remove(&entity) {
            let dvx = jx / PLAYER_MASS;
            let dvy = jy / PLAYER_MASS;
            body.0.vx = body.0.vx.add_f32(dvx);
            body.0.vy = body.0.vy.add_f32(dvy);
            crushes.0.push((entity, dvx.hypot(dvy)));
        }
        let result = step_player(
            &sim.0,
            &registry.0,
            &params,
            &mut body.0,
            &mut control.0,
            StepInput {
                move_x: player.input.move_x,
                jump: player.input.jump,
                jump_pressed: std::mem::take(&mut player.jump_pressed),
                down: player.input.down,
                fly: player.flying && mode.0 == GameMode::Creative,
            },
        );
        if !result.displaced.is_empty() {
            scatter_powder(
                &mut sim.0,
                &registry.0,
                &obstacles.0,
                &body.0,
                &result.displaced,
            );
        }
        for blocked in &result.blocked {
            if !sim
                .0
                .get_cell(blocked.pos)
                .is_some_and(|cell| cell.is_body())
            {
                continue;
            }
            let Some(pixel_body) = bodies.body_at_mut(blocked.pos) else {
                continue;
            };
            if pixel_body.frozen {
                continue;
            }
            let jx = PLAYER_MASS * blocked.dvx;
            let jy = PLAYER_MASS * blocked.dvy;
            let rx = (Fixed::cell_center(blocked.pos.x) - pixel_body.x).to_f32();
            let ry = (Fixed::cell_center(blocked.pos.y) - pixel_body.y).to_f32();
            pixel_body.vx = pixel_body.vx.add_f32(jx * pixel_body.inv_mass);
            pixel_body.vy = pixel_body.vy.add_f32(jy * pixel_body.inv_mass);
            pixel_body.spin += (rx * jy - ry * jx) * pixel_body.inv_inertia;
            pixel_body.rest_secs = 0.0;
            pixel_body.asleep = false;
        }
        if health.hp <= 0.0 {
            health.hp = MAX_HP;
            air.secs = MAX_AIR_SECS;
            burning.secs = 0.0;
            body.0 = Actor::new(
                Fixed::from_cell(spawn_point.0.x),
                Fixed::from_cell(spawn_point.0.y),
                PLAYER_HALF_W,
                PLAYER_HALF_H,
            );
            control.0 = Controller::default();
        }
    }
    impulses.0.clear();
}

const PUSH_RESTITUTION: f32 = 0.2;

pub fn push_players(mut query: Query<&mut PlayerActor>) {
    let mut combos = query.iter_combinations_mut::<2>();
    while let Some([mut a, mut b]) = combos.fetch_next() {
        let dx = b.0.x - a.0.x;
        let dy = b.0.y - a.0.y;
        let ox = (a.0.half_w + b.0.half_w) - dx.abs();
        let oy = (a.0.half_h + b.0.half_h) - dy.abs();
        if ox <= Fixed::ZERO || oy <= Fixed::ZERO {
            continue;
        }
        if ox < oy {
            let push = ox.mul(Fixed::HALF);
            let n = if dx >= Fixed::ZERO { 1.0 } else { -1.0 };
            if dx >= Fixed::ZERO {
                a.0.x -= push;
                b.0.x += push;
            } else {
                a.0.x += push;
                b.0.x -= push;
            }
            let rel = (b.0.vx - a.0.vx).to_f32();
            if rel * n < 0.0 {
                let e = if rel.abs() > BOUNCE_MIN_SPEED {
                    PUSH_RESTITUTION
                } else {
                    0.0
                };
                let delta = (1.0 + e) * rel * 0.5;
                b.0.vx = b.0.vx.add_f32(-delta);
                a.0.vx = a.0.vx.add_f32(delta);
            }
        } else {
            let push = oy.mul(Fixed::HALF);
            let n = if dy >= Fixed::ZERO { 1.0 } else { -1.0 };
            if dy >= Fixed::ZERO {
                a.0.y -= push;
                b.0.y += push;
            } else {
                a.0.y += push;
                b.0.y -= push;
            }
            let rel = (b.0.vy - a.0.vy).to_f32();
            if rel * n < 0.0 {
                let e = if rel.abs() > BOUNCE_MIN_SPEED {
                    PUSH_RESTITUTION
                } else {
                    0.0
                };
                let delta = (1.0 + e) * rel * 0.5;
                b.0.vy = b.0.vy.add_f32(-delta);
                a.0.vy = a.0.vy.add_f32(delta);
            }
        }
    }
}
