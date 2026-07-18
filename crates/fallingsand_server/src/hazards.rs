use crate::player::{PlayerLife, Players};
use crate::{MAX_AIR_SECONDS, MAX_HEALTH};
use fallingsand_core::content;
use fallingsand_core::{CellPos, Phase, TICK_DT, Tag};
use fallingsand_protocol::GameMode;
use fallingsand_sim::CellWorld;
use fallingsand_sim::physics::{Actor, CellSource};

pub const BURN_SECS: f32 = 4.0;
pub const BURN_DPS: f32 = 6.0;
pub const DROWN_DPS: f32 = 10.0;
pub const AIR_REFILL_MULT: f32 = 4.0;
pub const CRUSH_THRESHOLD_DV: f32 = 120.0;
pub const CRUSH_DAMAGE_PER_DV: f32 = 0.3;
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

pub fn sample_hazards<W: CellSource>(world: &W, body: &Actor) -> HazardSample {
    let mut sample = HazardSample::default();
    let fp = body.footprint();
    let mut probe = |pos: CellPos| {
        let Some(cell) = world.cell_at(pos) else {
            return;
        };
        let hot = content::tags(cell.material).contains(Tag::Hot);
        sample.contact_dps = sample
            .contact_dps
            .max(content::material(cell.material).contact_damage);
        sample.hot |= hot;
        sample.extinguish |= content::phase(cell.material) == Phase::Liquid && !hot;
    };
    for y in fp.y0 - 1..=fp.y1 + 1 {
        probe(CellPos::new(fp.x0 - 1, y));
        probe(CellPos::new(fp.x1 + 1, y));
    }
    for x in fp.x0..=fp.x1 {
        probe(CellPos::new(x, fp.y0 - 1));
        probe(CellPos::new(x, fp.y1 + 1));
    }
    let head = CellPos::new(body.x.floor_cell(), fp.y1 + 1);
    sample.head_submerged = matches!(
        world.cell_at(head),
        Some(cell) if content::phase(cell.material) == Phase::Liquid
    );
    sample
}

pub fn crush_damage(dv: f32) -> f32 {
    ((dv - CRUSH_THRESHOLD_DV) * CRUSH_DAMAGE_PER_DV).max(0.0)
}

pub fn apply_hazards(sim: &CellWorld, players: &mut Players) {
    for (_, player) in players.iter_mut() {
        let survival = player.profile.mode == GameMode::Survival;
        let PlayerLife::Alive(avatar) = &mut player.life else {
            continue;
        };
        if !survival {
            avatar.air = MAX_AIR_SECONDS;
            avatar.burning_secs = 0.0;
            avatar.pending_crush_dv = 0.0;
            continue;
        }
        let sample = sample_hazards(sim, &avatar.actor);
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
        damage += crush_damage(std::mem::take(&mut avatar.pending_crush_dv));
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
