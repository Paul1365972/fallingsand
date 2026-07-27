use crate::player::{Avatar, AvatarSnapshot, BodyIds, Health, PlayerLife, Players};
use fallingsand_core::content::{self, material};
use fallingsand_core::{CellPos, Subcell};
use fallingsand_protocol::{GameMode, PlayerId};
use fallingsand_sim::CellWorld;
use fallingsand_sim::creature::{Creature, PlayerParams, StepInput, grounded, step_player};
use fallingsand_sim::debris::{CreaturePeer, DebrisWorld};
use fallingsand_sim::player::{DUCK_ROWS, STAND_ROWS, player_shape, stamp_player, unstamp_player};
use fallingsand_sim::shape::Footprint;
use std::collections::BTreeMap;

pub fn creature_mass(creature: &Creature) -> i64 {
    let cells = i64::from(creature.shape.w()) * i64::from(creature.shape.h());
    i64::from(content::density_milli(material::FLESH).max(1)) * cells
}

pub fn creature_peer(players: &Players, id: u32) -> Option<CreaturePeer> {
    let player = players.player_for_body(id)?;
    let avatar = players.get(player)?.avatar()?;
    Some(CreaturePeer {
        mass_milli: creature_mass(&avatar.creature),
        vx: avatar.creature.vx,
        vy: avatar.creature.vy,
        grounded: avatar.creature.on_ground,
    })
}

type BlockedGroups = BTreeMap<(u32, bool), (Subcell, Vec<(CellPos, i64)>)>;

struct CreatureShove {
    body_id: u32,
    pusher: PlayerId,
    horizontal: bool,
    removed: Subcell,
    pusher_mass: i64,
}

pub fn step_physics(sim: &mut CellWorld, players: &mut Players, debris: &mut DebrisWorld) {
    let params = PlayerParams::default();
    let mut shoves: Vec<CreatureShove> = Vec::new();

    for (&id, player) in players.iter_mut() {
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
        let mass = creature_mass(&avatar.creature);
        let mut total_removed = (Subcell::ZERO, Subcell::ZERO);
        for blocked in &result.blocked {
            total_removed.0 += blocked.velocity_delta_x;
            total_removed.1 += blocked.velocity_delta_y;
        }
        let before = (
            avatar.creature.vx + total_removed.0,
            avatar.creature.vy + total_removed.1,
        );
        let mut groups: BlockedGroups = BTreeMap::new();
        for blocked in &result.blocked {
            let Some(body_id) = sim.get_cell(blocked.pos).and_then(|cell| cell.body_id()) else {
                continue;
            };
            for (horizontal, share) in [
                (true, blocked.velocity_delta_x),
                (false, blocked.velocity_delta_y),
            ] {
                if share == Subcell::ZERO {
                    continue;
                }
                let entry = groups
                    .entry((body_id, horizontal))
                    .or_insert((Subcell::ZERO, Vec::new()));
                entry.0 += share;
                entry.1.push((blocked.pos, share.raw().abs().max(1)));
            }
        }
        for ((body_id, horizontal), (removed, cells)) in groups {
            let axis_before = if horizontal { before.0 } else { before.1 };
            match debris.creature_collide(body_id, &cells, horizontal, removed, axis_before, mass) {
                Some(returned) => {
                    if horizontal {
                        avatar.creature.vx += returned;
                    } else {
                        avatar.creature.vy += returned;
                    }
                }
                None => shoves.push(CreatureShove {
                    body_id,
                    pusher: id,
                    horizontal,
                    removed,
                    pusher_mass: mass,
                }),
            }
        }
        let facing_left = match input.move_x() {
            x if x < 0 => true,
            x if x > 0 => false,
            _ => avatar.stamp.facing_left(),
        };
        commit_pose(sim, avatar, snapshot, facing_left);
    }

    for shove in shoves {
        apply_creature_shove(players, shove);
    }
}

fn apply_creature_shove(players: &mut Players, shove: CreatureShove) {
    let Some(target) = players.player_for_body(shove.body_id) else {
        return;
    };
    let Some(avatar) = players
        .get_mut(target)
        .and_then(|player| player.avatar_mut())
    else {
        return;
    };
    let target_mass = creature_mass(&avatar.creature);
    let removed = shove.removed.raw();
    if !shove.horizontal && removed < 0 && avatar.creature.on_ground {
        return;
    }
    let transferred = removed * shove.pusher_mass / (shove.pusher_mass + target_mass);
    if shove.horizontal {
        avatar.creature.vx += Subcell::from_raw(transferred);
    } else {
        avatar.creature.vy += Subcell::from_raw(transferred);
    }
    if let Some(pusher) = players
        .get_mut(shove.pusher)
        .and_then(|player| player.avatar_mut())
    {
        if shove.horizontal {
            pusher.creature.vx += Subcell::from_raw(transferred);
        } else {
            pusher.creature.vy += Subcell::from_raw(transferred);
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
