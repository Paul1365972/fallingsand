use crate::bodies::PixelBodies;
use crate::player::{
    Avatar, AvatarSnapshot, Health, PLAYER_HALF_H, PLAYER_HALF_W, PLAYER_MASS, PlayerLife, Players,
};
use fallingsand_core::{CellPos, Fixed};
use fallingsand_protocol::GameMode;
use fallingsand_sim::CellWorld;
use fallingsand_sim::bodies::{vacated_wake_targets, wake_covering};
use fallingsand_sim::physics::{
    Footprint, PlayerParams, StepInput, footprint_at, grounded, step_player,
};
use fallingsand_sim::player::{DUCK_ROWS, STAND_ROWS, stamp_player, unstamp_player};

pub fn step_physics(sim: &mut CellWorld, bodies: &mut PixelBodies, players: &mut Players) {
    bodies.refresh_owners();
    let params = PlayerParams::default();
    let prior: Vec<_> = players
        .iter()
        .filter_map(|(&id, player)| {
            player
                .avatar()
                .and_then(|avatar| avatar.stamp.own_cells().map(|set| (id, set.clone())))
        })
        .collect();
    let mut shoves: Vec<(CellPos, f32, f32)> = Vec::new();

    for (_, player) in players.iter_mut() {
        let input = player.control.input;
        let jump_pressed = std::mem::take(&mut player.control.jump_pressed);
        let creative = player.profile.mode == GameMode::Creative;
        let PlayerLife::Alive(avatar) = &mut player.life else {
            continue;
        };
        debug_assert!(avatar.stamp.is_stamped());

        let (jx, jy) = std::mem::take(&mut avatar.pending_impulse);
        if jx != 0.0 || jy != 0.0 {
            let dvx = jx / PLAYER_MASS;
            let dvy = jy / PLAYER_MASS;
            avatar.actor.vx = avatar.actor.vx.add_vel_f32(dvx);
            avatar.actor.vy = avatar.actor.vy.add_vel_f32(dvy);
            avatar.pending_crush_dv = avatar.pending_crush_dv.max(dvx.hypot(dvy));
        }

        let snapshot = avatar.actor;
        let result = step_player(
            sim,
            &params,
            &mut avatar.actor,
            &mut avatar.controller,
            StepInput {
                move_x: input.move_x,
                jump: input.jump,
                jump_pressed,
                down: input.down,
                fly: avatar.flying && creative,
            },
            avatar.stamp.own_cells(),
        );
        let facing_left = match input.move_x {
            x if x < 0 => true,
            x if x > 0 => false,
            _ => avatar.stamp.facing_left(),
        };
        commit_pose(sim, bodies, avatar, snapshot, facing_left);

        let (mut restore_x, mut restore_y) = (0.0f32, 0.0f32);
        for blocked in &result.blocked {
            let Some(cell) = sim.get_cell(blocked.pos) else {
                continue;
            };
            if !cell.is_body() {
                continue;
            }
            match bodies.body_at_mut(blocked.pos) {
                Some(pixel_body) => {
                    if pixel_body.frozen {
                        continue;
                    }
                    restore_x += blocked.dvx;
                    restore_y += blocked.dvy;
                    pixel_body.rest_secs = 0.0;
                    pixel_body.asleep = false;
                }
                None => {
                    let jx = PLAYER_MASS * blocked.dvx;
                    let jy = PLAYER_MASS * blocked.dvy;
                    shoves.push((blocked.pos, jx, jy));
                }
            }
        }
        avatar.actor.vx = avatar.actor.vx.add_vel_f32(restore_x);
        avatar.actor.vy = avatar.actor.vy.add_vel_f32(restore_y);
    }

    for (pos, jx, jy) in shoves {
        let target = players
            .iter()
            .find_map(|(&id, player)| {
                player
                    .avatar()
                    .filter(|avatar| avatar.stamp.covers(pos))
                    .map(|_| id)
            })
            .or_else(|| {
                prior
                    .iter()
                    .find_map(|(id, cells)| cells.contains(&pos).then_some(*id))
            });
        if let Some(target) = target
            && let Some(avatar) = players
                .get_mut(target)
                .and_then(|player| player.avatar_mut())
        {
            avatar.pending_impulse.0 += jx;
            avatar.pending_impulse.1 += jy;
        }
    }
}

fn commit_pose(
    sim: &mut CellWorld,
    bodies: &mut PixelBodies,
    avatar: &mut Avatar,
    snapshot: fallingsand_sim::physics::Actor,
    facing_left: bool,
) {
    let candidates = [
        (avatar.actor.x, avatar.actor.y, avatar.actor.half_h),
        (avatar.actor.x, snapshot.y, snapshot.half_h),
        (snapshot.x, avatar.actor.y, avatar.actor.half_h),
    ];
    for (attempt, &(x, y, half_h)) in candidates.iter().enumerate() {
        let fp = footprint_at(x, y, avatar.actor.half_w, half_h);
        let Some(vacated) = stamp_player(sim, &mut avatar.stamp, fp, facing_left) else {
            continue;
        };
        match attempt {
            1 => {
                avatar.actor.y = snapshot.y;
                avatar.actor.half_h = snapshot.half_h;
                avatar.actor.vy = Fixed::ZERO;
            }
            2 => {
                avatar.actor.x = snapshot.x;
                avatar.actor.vx = Fixed::ZERO;
            }
            _ => {}
        }
        if attempt != 0 {
            avatar.actor.on_ground = grounded(sim, &avatar.actor, avatar.stamp.own_cells());
        }
        wake_neighbours(sim, bodies, &avatar.stamp, &vacated);
        return;
    }
    avatar.actor = snapshot;
    avatar.actor.vx = Fixed::ZERO;
    avatar.actor.vy = Fixed::ZERO;
    avatar.actor.on_ground = grounded(sim, &avatar.actor, avatar.stamp.own_cells());
}

fn wake_neighbours(
    sim: &CellWorld,
    bodies: &mut PixelBodies,
    stamp: &fallingsand_sim::PlayerStamp,
    vacated: &[CellPos],
) {
    for pos in vacated_wake_targets(sim, &|pos| stamp.covers(pos), vacated) {
        wake_covering(&mut bodies.bodies, &bodies.owners, pos);
    }
}

pub fn unstamp_and_wake(
    sim: &mut CellWorld,
    bodies: &mut PixelBodies,
    stamp: &mut fallingsand_sim::PlayerStamp,
) {
    let vacated: Vec<CellPos> = stamp
        .own_cells()
        .map(|set| set.iter().copied().collect())
        .unwrap_or_default();
    unstamp_player(sim, stamp);
    bodies.refresh_owners();
    for pos in vacated_wake_targets(sim, &|_| false, &vacated) {
        wake_covering(&mut bodies.bodies, &bodies.owners, pos);
    }
}

pub fn footprint_loaded(sim: &CellWorld, fp: Footprint) -> bool {
    for y in fp.y0..=fp.y1 {
        for x in fp.x0..=fp.x1 {
            if sim.get_cell(CellPos::new(x, y)).is_none() {
                return false;
            }
        }
    }
    true
}

pub fn try_materialize(
    sim: &mut CellWorld,
    bodies: &mut PixelBodies,
    template: &AvatarSnapshot,
    candidate: CellPos,
) -> Option<Avatar> {
    let saved = template.cell();
    let (x, y) = if candidate == saved {
        (template.x, template.y)
    } else {
        (Fixed::from_cell(candidate.x), Fixed::from_cell(candidate.y))
    };
    let mut actor = fallingsand_sim::physics::Actor::new(x, y, PLAYER_HALF_W, PLAYER_HALF_H);
    actor.vx = template.vx;
    actor.vy = template.vy;
    let base = actor.footprint();
    if !footprint_loaded(sim, base) {
        return None;
    }

    let mut stamp = fallingsand_sim::PlayerStamp::default();
    for rows in (DUCK_ROWS as i32..=STAND_ROWS as i32).rev() {
        let fp = Footprint {
            x0: base.x0,
            y0: base.y0,
            x1: base.x1,
            y1: base.y0 + rows - 1,
        };
        let Some(vacated) = stamp_player(sim, &mut stamp, fp, false) else {
            continue;
        };
        actor.y += Fixed::from_int(rows / 2 - STAND_ROWS as i32 / 2);
        actor.half_h = Fixed::from_int(rows).mul(Fixed::HALF);
        wake_neighbours(sim, bodies, &stamp, &vacated);
        return Some(Avatar {
            actor,
            stamp,
            controller: Default::default(),
            health: Health {
                hp: template.hp.clamp(0.0, crate::MAX_HP),
                regen_delay_ticks: template.regen_delay_ticks,
            },
            air: template.air.clamp(0.0, crate::MAX_AIR_SECS),
            burning_secs: template.burning.max(0.0),
            flying: template.flying,
            dig: Default::default(),
            pending_impulse: (0.0, 0.0),
            pending_crush_dv: 0.0,
        });
    }
    None
}
