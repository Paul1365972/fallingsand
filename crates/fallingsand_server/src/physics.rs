use crate::player::{Avatar, AvatarSnapshot, BodyIds, Health, PlayerLife, Players};
use fallingsand_core::{CellPos, Subcell};
use fallingsand_protocol::GameMode;
use fallingsand_sim::CellWorld;
use fallingsand_sim::creature::{Creature, PlayerParams, StepInput, grounded, step_player};
use fallingsand_sim::player::{DUCK_ROWS, STAND_ROWS, player_shape, stamp_player, unstamp_player};
use fallingsand_sim::shape::Footprint;

pub fn step_physics(sim: &mut CellWorld, players: &mut Players) {
    let params = PlayerParams::default();
    let mut shoves: Vec<(u32, Subcell, Subcell)> = Vec::new();

    for (_, player) in players.iter_mut() {
        let input = player.control.input;
        let jump_pressed = std::mem::take(&mut player.control.jump_pressed);
        let creative = player.profile.mode == GameMode::Creative;
        let PlayerLife::Alive(avatar) = &mut player.life else {
            continue;
        };
        debug_assert!(avatar.stamp.is_stamped());

        let snapshot = avatar.creature.clone();
        let result = step_player(
            sim,
            &params,
            &mut avatar.creature,
            &mut avatar.controller,
            StepInput {
                move_x: input.move_x(),
                jump: input.jump,
                jump_pressed,
                down: input.down,
                fly: avatar.flying && creative,
            },
            avatar.stamp.own_cells(),
            avatar.stamp.submersion(),
        );
        for blocked in &result.blocked {
            let Some(body_id) = sim.get_cell(blocked.pos).and_then(|cell| cell.body_id()) else {
                continue;
            };
            shoves.push((body_id, blocked.velocity_delta_x, blocked.velocity_delta_y));
        }
        let facing_left = match input.move_x() {
            x if x < 0 => true,
            x if x > 0 => false,
            _ => avatar.stamp.facing_left(),
        };
        commit_pose(sim, avatar, snapshot, facing_left);
    }

    for (body_id, dvx, dvy) in shoves {
        if let Some(target) = players.player_for_body(body_id)
            && let Some(avatar) = players
                .get_mut(target)
                .and_then(|player| player.avatar_mut())
        {
            avatar.creature.vx += dvx;
            avatar.creature.vy += dvy;
        }
    }
}

fn commit_pose(sim: &mut CellWorld, avatar: &mut Avatar, snapshot: Creature, facing_left: bool) {
    let full = avatar.creature.footprint();
    if stamp_player(sim, &mut avatar.stamp, full, facing_left, avatar.body_id).is_some() {
        return;
    }

    let d_step = Subcell::from_cells((avatar.creature.rows() / 2 - snapshot.rows() / 2) as f32);
    avatar.creature.y -= d_step;
    avatar.creature.shape = snapshot.shape;
    let held = avatar.creature.footprint();
    stamp_player(sim, &mut avatar.stamp, held, facing_left, avatar.body_id)
        .expect("a same-height translation always stamps");
    avatar.creature.on_ground = grounded(sim, &avatar.creature, avatar.stamp.own_cells());
}

pub fn unstamp(sim: &mut CellWorld, stamp: &mut fallingsand_sim::PlayerStamp, body_id: u32) {
    unstamp_player(sim, stamp, body_id);
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
    body_ids: &mut BodyIds,
    template: &AvatarSnapshot,
    candidate: CellPos,
) -> Option<Avatar> {
    let saved = template.cell();
    let (x, y) = if candidate == saved {
        (template.x, template.y)
    } else {
        (
            Subcell::from_cell(candidate.x),
            Subcell::from_cell(candidate.y),
        )
    };
    let mut creature = Creature::new(x, y, player_shape(STAND_ROWS));
    creature.vx = template.vx;
    creature.vy = template.vy;
    let base = creature.footprint();
    if !footprint_loaded(sim, base) {
        return None;
    }

    let body_id = body_ids.allocate();
    let mut stamp = fallingsand_sim::PlayerStamp::default();
    for rows in (DUCK_ROWS as i32..=STAND_ROWS as i32).rev() {
        let fp = Footprint {
            x0: base.x0,
            y0: base.y0,
            x1: base.x1,
            y1: base.y0 + rows - 1,
        };
        let Some(()) = stamp_player(sim, &mut stamp, fp, false, body_id) else {
            continue;
        };
        creature.y += Subcell::from_cells((rows / 2 - STAND_ROWS as i32 / 2) as f32);
        creature.shape = player_shape(rows as usize);
        return Some(Avatar {
            creature,
            stamp,
            body_id,
            controller: Default::default(),
            health: Health {
                hp: template.hp.clamp(0.0, crate::MAX_HEALTH),
                regen_delay_ticks: template.regen_delay_ticks,
            },
            air: template.air.clamp(0.0, crate::MAX_AIR_SECONDS),
            burning_secs: template.burning.max(0.0),
            flying: template.flying,
            dig: Default::default(),
        });
    }
    None
}
