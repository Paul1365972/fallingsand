use crate::SimWorld;
use crate::player::{
    Control, Health, Life, Mode, PLAYER_HALF_H, PLAYER_MASS, Player, PlayerActor, PlayerRaster,
};
use bevy_ecs::prelude::*;
use fallingsand_core::{CellPos, Fixed};
use fallingsand_protocol::{GameMode, LifeState};
use fallingsand_sim::CellWorld;
use fallingsand_sim::bodies::{vacated_wake_targets, wake_covering};
use fallingsand_sim::physics::{
    Actor, Footprint, PlayerParams, StepInput, footprint_at, grounded, step_player,
};
use fallingsand_sim::player::{DUCK_ROWS, STAND_ROWS, stamp_player, unstamp_player};
use rustc_hash::FxHashSet;

const SPAWN_SEARCH_UP: i32 = 64;

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub fn step_physics(
    mut sim: ResMut<SimWorld>,
    mut bodies: ResMut<crate::bodies::PixelBodies>,
    mut impulses: ResMut<crate::PlayerImpulses>,
    mut crushes: ResMut<crate::hazards::CrushEvents>,
    mut query: Query<(
        Entity,
        &mut Player,
        &Life,
        &mut Health,
        &Mode,
        &mut PlayerActor,
        &mut PlayerRaster,
        &mut Control,
    )>,
) {
    let params = PlayerParams::default();
    let mut order: Vec<(u32, Entity)> = query
        .iter()
        .map(|(entity, player, ..)| (player.id.0, entity))
        .collect();
    order.sort_unstable();
    let mut shoves: Vec<(CellPos, f32, f32)> = Vec::new();
    let prior: Vec<(Entity, FxHashSet<CellPos>)> = query
        .iter()
        .filter_map(|(entity, _, _, _, _, _, raster, ..)| {
            raster.0.own_cells().map(|set| (entity, set.clone()))
        })
        .collect();

    for (_, entity) in order {
        let Ok((entity, mut player, life, mut health, mode, mut body, mut raster, mut control)) =
            query.get_mut(entity)
        else {
            continue;
        };

        if life.0 != LifeState::Alive {
            continue;
        }

        if !raster.0.is_stamped() {
            match spawn_stamp(&mut sim.0, &mut raster.0, &mut body.0) {
                StampResult::Stamped => {}
                StampResult::Deferred => continue,
                StampResult::Blocked => {
                    health.hp = 0.0;
                    continue;
                }
            }
        }

        if let Some((jx, jy)) = impulses.0.remove(&entity) {
            let dvx = jx / PLAYER_MASS;
            let dvy = jy / PLAYER_MASS;
            body.0.vx = body.0.vx.add_vel_f32(dvx);
            body.0.vy = body.0.vy.add_vel_f32(dvy);
            crushes.0.push((entity, dvx.hypot(dvy)));
        }

        let snapshot = body.0;
        let result = {
            let own = raster.0.own_cells();
            step_player(
                &sim.0,
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
                own,
            )
        };
        let facing_left = match player.input.move_x {
            x if x < 0 => true,
            x if x > 0 => false,
            _ => raster.0.facing_left(),
        };
        commit_pose(
            &mut sim.0,
            &mut bodies,
            &mut raster.0,
            &mut body.0,
            snapshot,
            facing_left,
        );

        for blocked in &result.blocked {
            let Some(cell) = sim.0.get_cell(blocked.pos) else {
                continue;
            };
            if !cell.is_body() {
                continue;
            }
            let jx = PLAYER_MASS * blocked.dvx;
            let jy = PLAYER_MASS * blocked.dvy;
            match bodies.body_at_mut(blocked.pos) {
                Some(pixel_body) => {
                    if pixel_body.frozen {
                        continue;
                    }
                    let rx = (Fixed::cell_center(blocked.pos.x) - pixel_body.x).to_f32();
                    let ry = (Fixed::cell_center(blocked.pos.y) - pixel_body.y).to_f32();
                    pixel_body.vx = pixel_body.vx.add_vel_f32(jx * pixel_body.inv_mass());
                    pixel_body.vy = pixel_body.vy.add_vel_f32(jy * pixel_body.inv_mass());
                    pixel_body.spin += (rx * jy - ry * jx) * pixel_body.inv_inertia();
                    pixel_body.rest_secs = 0.0;
                    pixel_body.asleep = false;
                }
                None => shoves.push((blocked.pos, jx, jy)),
            }
        }
    }

    impulses.0.clear();
    for (pos, jx, jy) in shoves {
        let target = query
            .iter()
            .find_map(|(entity, _, _, _, _, _, raster, ..)| raster.0.covers(pos).then_some(entity))
            .or_else(|| {
                prior
                    .iter()
                    .find_map(|(entity, cells)| cells.contains(&pos).then_some(*entity))
            });
        if let Some(target) = target {
            let entry = impulses.0.entry(target).or_insert((0.0, 0.0));
            entry.0 += jx;
            entry.1 += jy;
        }
    }
}

fn commit_pose(
    sim: &mut CellWorld,
    bodies: &mut crate::bodies::PixelBodies,
    stamp: &mut fallingsand_sim::PlayerStamp,
    body: &mut Actor,
    snapshot: Actor,
    facing_left: bool,
) {
    let candidates = [
        (body.x, body.y, body.half_h),
        (body.x, snapshot.y, snapshot.half_h),
        (snapshot.x, body.y, body.half_h),
    ];
    for (attempt, &(x, y, half_h)) in candidates.iter().enumerate() {
        let fp = footprint_at(x, y, body.half_w, half_h);
        let Some(vacated) = stamp_player(sim, stamp, fp, facing_left) else {
            continue;
        };
        match attempt {
            1 => {
                body.y = snapshot.y;
                body.half_h = snapshot.half_h;
                body.vy = Fixed::ZERO;
            }
            2 => {
                body.x = snapshot.x;
                body.vx = Fixed::ZERO;
            }
            _ => {}
        }
        if attempt != 0 {
            body.on_ground = grounded(sim, body, stamp.own_cells());
        }
        wake_neighbours(sim, bodies, stamp, &vacated);
        return;
    }
    *body = snapshot;
    body.vx = Fixed::ZERO;
    body.vy = Fixed::ZERO;
    body.on_ground = grounded(sim, body, stamp.own_cells());
}

fn wake_neighbours(
    sim: &CellWorld,
    bodies: &mut crate::bodies::PixelBodies,
    stamp: &fallingsand_sim::PlayerStamp,
    vacated: &[CellPos],
) {
    for pos in vacated_wake_targets(sim, &|pos| stamp.covers(pos), vacated) {
        wake_covering(&mut bodies.bodies, pos);
    }
}

pub(crate) fn unstamp_and_wake(
    sim: &mut CellWorld,
    bodies: &mut crate::bodies::PixelBodies,
    stamp: &mut fallingsand_sim::PlayerStamp,
) {
    let vacated: Vec<CellPos> = stamp
        .own_cells()
        .map(|set| set.iter().copied().collect())
        .unwrap_or_default();
    unstamp_player(sim, stamp);
    for pos in vacated_wake_targets(sim, &|_| false, &vacated) {
        wake_covering(&mut bodies.bodies, pos);
    }
}

pub(crate) enum StampResult {
    Stamped,
    Deferred,
    Blocked,
}

pub(crate) fn spawn_stamp(
    sim: &mut CellWorld,
    stamp: &mut fallingsand_sim::PlayerStamp,
    body: &mut Actor,
) -> StampResult {
    body.half_h = PLAYER_HALF_H;
    let base = body.footprint();
    if !footprint_loaded(sim, base) {
        return StampResult::Deferred;
    }
    for rows in (DUCK_ROWS as i32..=STAND_ROWS as i32).rev() {
        let fp = Footprint {
            x0: base.x0,
            y0: base.y0,
            x1: base.x1,
            y1: base.y0 + rows - 1,
        };
        if stamp_player(sim, stamp, fp, false).is_some() {
            body.y += Fixed::from_int(rows / 2 - STAND_ROWS as i32 / 2);
            body.half_h = Fixed::from_int(rows).mul(Fixed::HALF);
            return StampResult::Stamped;
        }
    }
    for up in 1..=SPAWN_SEARCH_UP {
        let fp = Footprint {
            x0: base.x0,
            y0: base.y0 + up,
            x1: base.x1,
            y1: base.y1 + up,
        };
        if stamp_player(sim, stamp, fp, false).is_some() {
            body.y += Fixed::from_int(up);
            return StampResult::Stamped;
        }
    }
    StampResult::Blocked
}

fn footprint_loaded(sim: &CellWorld, fp: Footprint) -> bool {
    for y in fp.y0..=fp.y1 {
        for x in fp.x0..=fp.x1 {
            if sim.get_cell(CellPos::new(x, y)).is_none() {
                return false;
            }
        }
    }
    true
}
