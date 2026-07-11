use crate::player::{
    Air, Burning, Control, Health, Mode, PLAYER_HALF_H, PLAYER_HALF_W, PLAYER_MASS, Player,
    PlayerActor, PlayerRaster,
};
use crate::{MAX_AIR_SECS, MAX_HP, Registry, SimWorld, SpawnPoint};
use bevy_ecs::prelude::*;
use fallingsand_core::{CellPos, Fixed};
use fallingsand_protocol::GameMode;
use fallingsand_sim::CellWorld;
use fallingsand_sim::bodies::wake_covering;
use fallingsand_sim::physics::{
    Actor, Controller, Footprint, PlayerParams, StepInput, footprint_at, grounded, step_player,
};
use fallingsand_sim::player::{force_stamp_player, stamp_player, unstamp_player};

const SPAWN_SEARCH_UP: i32 = 64;

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub fn step_physics(
    mut sim: ResMut<SimWorld>,
    registry: Res<Registry>,
    spawn_point: Res<SpawnPoint>,
    mut bodies: ResMut<crate::bodies::PixelBodies>,
    mut impulses: ResMut<crate::PlayerImpulses>,
    mut crushes: ResMut<crate::hazards::CrushEvents>,
    mut query: Query<(
        Entity,
        &mut Player,
        &Mode,
        &mut PlayerActor,
        &mut PlayerRaster,
        &mut Control,
        &mut Health,
        &mut Air,
        &mut Burning,
    )>,
) {
    let params = PlayerParams::default();
    let mut order: Vec<(u32, Entity)> = query
        .iter()
        .map(|(entity, player, ..)| (player.id.0, entity))
        .collect();
    order.sort_unstable();
    let mut shoves: Vec<(CellPos, f32, f32)> = Vec::new();

    for (_, entity) in order {
        let Ok((
            entity,
            mut player,
            mode,
            mut body,
            mut raster,
            mut control,
            mut health,
            mut air,
            mut burning,
        )) = query.get_mut(entity)
        else {
            continue;
        };

        if !raster.0.is_stamped() {
            spawn_stamp(
                &mut sim.0,
                &registry.0,
                &mut raster.0,
                &mut body.0,
                &mut control.0,
            );
            if !raster.0.is_stamped() {
                continue;
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
        let was_ducking = control.0.ducking();
        let result = {
            let own = raster.0.own_cells();
            step_player(
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
            &registry.0,
            &mut bodies,
            &mut raster.0,
            &mut body.0,
            &mut control.0,
            snapshot,
            was_ducking,
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

        if health.hp <= 0.0 {
            health.hp = MAX_HP;
            air.secs = MAX_AIR_SECS;
            burning.secs = 0.0;
            unstamp_player(&mut sim.0, &mut raster.0);
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
    for (pos, jx, jy) in shoves {
        let target = query
            .iter()
            .find_map(|(entity, _, _, _, raster, ..)| raster.0.covers(pos).then_some(entity));
        if let Some(target) = target {
            let entry = impulses.0.entry(target).or_insert((0.0, 0.0));
            entry.0 += jx;
            entry.1 += jy;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn commit_pose(
    sim: &mut CellWorld,
    registry: &fallingsand_core::MaterialRegistry,
    bodies: &mut crate::bodies::PixelBodies,
    stamp: &mut fallingsand_sim::PlayerStamp,
    body: &mut Actor,
    control: &mut Controller,
    snapshot: Actor,
    was_ducking: bool,
    facing_left: bool,
) {
    let ducking = control.ducking();
    let candidates = [
        (body.x, body.y, body.half_h, ducking),
        (body.x, snapshot.y, snapshot.half_h, was_ducking),
        (snapshot.x, body.y, body.half_h, ducking),
    ];
    for (attempt, &(x, y, half_h, duck)) in candidates.iter().enumerate() {
        let fp = footprint_at(x, y, body.half_w, half_h);
        let Some(vacated) = stamp_player(sim, registry, stamp, fp, duck, facing_left) else {
            continue;
        };
        match attempt {
            1 => {
                body.y = snapshot.y;
                body.half_h = snapshot.half_h;
                body.vy = Fixed::ZERO;
                control.set_ducking(was_ducking);
            }
            2 => {
                body.x = snapshot.x;
                body.vx = Fixed::ZERO;
            }
            _ => {}
        }
        if attempt != 0 {
            body.on_ground = grounded(sim, registry, body, stamp.own_cells());
        }
        wake_neighbours(sim, bodies, stamp, &vacated);
        return;
    }
    *body = snapshot;
    body.vx = Fixed::ZERO;
    body.vy = Fixed::ZERO;
    control.set_ducking(was_ducking);
    body.on_ground = grounded(sim, registry, body, stamp.own_cells());
}

fn wake_neighbours(
    sim: &CellWorld,
    bodies: &mut crate::bodies::PixelBodies,
    stamp: &fallingsand_sim::PlayerStamp,
    vacated: &[CellPos],
) {
    const NEIGHBORS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];
    for &pos in vacated {
        for (dx, dy) in NEIGHBORS {
            let neighbor = pos.translated(dx, dy);
            if stamp.covers(neighbor) {
                continue;
            }
            if sim.get_cell(neighbor).is_some_and(|cell| cell.is_body()) {
                wake_covering(&mut bodies.bodies, neighbor);
            }
        }
    }
}

fn spawn_stamp(
    sim: &mut CellWorld,
    registry: &fallingsand_core::MaterialRegistry,
    stamp: &mut fallingsand_sim::PlayerStamp,
    body: &mut Actor,
    control: &mut Controller,
) {
    let base = body.footprint();
    if !footprint_loaded(sim, base) {
        return;
    }
    control.set_ducking(false);
    body.half_h = PLAYER_HALF_H;
    for up in 0..=SPAWN_SEARCH_UP {
        let fp = Footprint {
            x0: base.x0,
            y0: base.y0 + up,
            x1: base.x1,
            y1: base.y1 + up,
        };
        if stamp_player(sim, registry, stamp, fp, false, false).is_some() {
            body.y += Fixed::from_int(up);
            return;
        }
    }
    tracing::warn!("spawn stamp forced at {:?}", (base.x0, base.y0));
    force_stamp_player(sim, stamp, base, false, false);
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
