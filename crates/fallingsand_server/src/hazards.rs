use crate::mobs::Mobs;
use crate::player::{PlayerLife, Players};
use crate::{MAX_AIR_SECONDS, MAX_HEALTH};
use fallingsand_core::content;
use fallingsand_core::{CellPos, CellRect, Phase, TICK_DT, Tag};
use fallingsand_protocol::GameMode;
use fallingsand_sim::CellWorld;
use fallingsand_sim::body::Bodies;

pub const BURN_SECS: f32 = 4.0;
pub const BURN_DPS: f32 = 6.0;
pub const DROWN_DPS: f32 = 10.0;
pub const AIR_REFILL_MULT: f32 = 4.0;
pub const REGEN_DELAY_SECS: f32 = 8.0;
pub const REGEN_RATE: f32 = 2.0;
const REGEN_DELAY_TICKS: u64 = fallingsand_core::ticks_from_secs(REGEN_DELAY_SECS);

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct HazardSample {
    pub contact_dps: f32,
    pub hot: bool,
    pub extinguish: bool,
    pub head_submerged: bool,
}

pub fn sample_hazards(world: &CellWorld, rect: CellRect) -> HazardSample {
    let mut sample = HazardSample::default();
    let mut probe = |pos: CellPos| {
        let Some(cell) = world.get_cell(pos) else {
            return;
        };
        let hot = content::tags(cell.material).contains(Tag::Hot);
        sample.contact_dps = sample
            .contact_dps
            .max(content::material(cell.material).contact_damage);
        sample.hot |= hot;
        sample.extinguish |= content::phase(cell.material) == Phase::Liquid && !hot;
    };
    for y in rect.grown(1).rows() {
        probe(CellPos::new(rect.min.x - 1, y));
        probe(CellPos::new(rect.max.x + 1, y));
    }
    for x in rect.columns() {
        probe(CellPos::new(x, rect.min.y - 1));
        probe(CellPos::new(x, rect.max.y + 1));
    }
    let head = CellPos::new((rect.min.x + rect.max.x) / 2, rect.max.y + 1);
    sample.head_submerged = matches!(
        world.get_cell(head),
        Some(cell) if content::phase(cell.material) == Phase::Liquid
            || content::tags(cell.material).contains(Tag::Suffocating)
    );
    sample
}

pub fn apply_hazards(sim: &CellWorld, bodies: &Bodies, players: &mut Players) {
    for (_, player) in players.iter_mut() {
        let survival = player.profile.mode == GameMode::Survival;
        let PlayerLife::Alive(avatar) = &mut player.life else {
            continue;
        };
        if !survival {
            avatar.air = MAX_AIR_SECONDS;
            avatar.burning_secs = 0.0;
            continue;
        }
        let bounds = bodies
            .bounds(avatar.body_id)
            .expect("an alive avatar owns its body");
        let sample = sample_hazards(sim, bounds);
        let mut damage = sample.contact_dps * TICK_DT;
        if sample.hot {
            avatar.burning_secs = BURN_SECS;
        }
        if sample.extinguish {
            avatar.burning_secs = 0.0;
        }
        if avatar.burning_secs > 0.0 {
            damage += BURN_DPS * TICK_DT;
            avatar.burning_secs = (avatar.burning_secs - TICK_DT).max(0.0);
        }
        if sample.head_submerged {
            avatar.air = (avatar.air - TICK_DT).max(0.0);
            if avatar.air <= 0.0 {
                damage += DROWN_DPS * TICK_DT;
            }
        } else {
            avatar.air = (avatar.air + AIR_REFILL_MULT * TICK_DT).min(MAX_AIR_SECONDS);
        }
        if damage > 0.0 {
            avatar.health.hp -= damage;
            avatar.health.regen_delay_ticks = REGEN_DELAY_TICKS;
        } else if avatar.health.regen_delay_ticks > 0 {
            avatar.health.regen_delay_ticks -= 1;
        } else if avatar.health.hp < MAX_HEALTH {
            avatar.health.hp = (avatar.health.hp + REGEN_RATE * TICK_DT).min(MAX_HEALTH);
        }
    }
}

pub fn apply_mob_hazards(sim: &CellWorld, bodies: &Bodies, mobs: &mut Mobs) -> Vec<u32> {
    let mut dead = Vec::new();
    for (&body_id, mob) in mobs.iter_mut() {
        let Some(bounds) = bodies.bounds(body_id) else {
            continue;
        };
        let sample = sample_hazards(sim, bounds);
        let mut damage = sample.contact_dps * TICK_DT;
        if sample.hot {
            mob.burning_secs = BURN_SECS;
        }
        if sample.extinguish {
            mob.burning_secs = 0.0;
        }
        if mob.burning_secs > 0.0 {
            damage += BURN_DPS * TICK_DT;
            mob.burning_secs = (mob.burning_secs - TICK_DT).max(0.0);
        }
        mob.hp -= damage;
        if mob.hp <= 0.0 {
            dead.push(body_id);
        }
    }
    dead
}
