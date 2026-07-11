use crate::player::{Air, Burning, Health, Mode, PlayerActor};
use crate::{MAX_AIR_SECS, MAX_HP, Registry, SimWorld};
use bevy_ecs::prelude::*;
use fallingsand_core::{CellPos, MaterialRegistry, Phase, TICK_DT, Tag};
use fallingsand_protocol::GameMode;
use fallingsand_sim::physics::{Actor, CellSource};
use rustc_hash::FxHashMap;

pub const BURN_SECS: f32 = 4.0;
pub const BURN_DPS: f32 = 6.0;
pub const DROWN_DPS: f32 = 10.0;
pub const AIR_REFILL_MULT: f32 = 4.0;
pub const CRUSH_THRESHOLD_DV: f32 = 120.0;
pub const CRUSH_DAMAGE_PER_DV: f32 = 0.3;
pub const REGEN_DELAY_SECS: f32 = 8.0;
pub const REGEN_RATE: f32 = 2.0;
const REGEN_DELAY_TICKS: u64 = fallingsand_core::ticks_from_secs(REGEN_DELAY_SECS);

#[derive(Resource, Default)]
pub struct CrushEvents(pub Vec<(Entity, f32)>);

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct HazardSample {
    pub contact_dps: f32,
    pub hot: bool,
    pub extinguish: bool,
    pub head_submerged: bool,
}

pub fn sample_hazards<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &Actor,
) -> HazardSample {
    let mut sample = HazardSample::default();
    let fp = body.footprint();
    let mut probe = |pos: CellPos| {
        let Some(cell) = world.cell_at(pos) else {
            return;
        };
        let material = registry.get(cell.material);
        let hot = material.tags.contains(Tag::Hot);
        sample.contact_dps = sample.contact_dps.max(material.contact_damage);
        sample.hot |= hot;
        sample.extinguish |= material.phase == Phase::Liquid && !hot;
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
        Some(cell) if registry.get(cell.material).phase == Phase::Liquid
    );
    sample
}

pub fn crush_damage(dv: f32) -> f32 {
    ((dv - CRUSH_THRESHOLD_DV) * CRUSH_DAMAGE_PER_DV).max(0.0)
}

#[allow(clippy::type_complexity)]
pub fn apply_hazards(
    sim: Res<SimWorld>,
    registry: Res<Registry>,
    mut crushes: ResMut<CrushEvents>,
    mut query: Query<(
        Entity,
        &Mode,
        &PlayerActor,
        &mut Health,
        &mut Air,
        &mut Burning,
    )>,
) {
    let tick = sim.0.tick();
    let mut crush: FxHashMap<Entity, f32> = FxHashMap::default();
    for (entity, dv) in crushes.0.drain(..) {
        let entry = crush.entry(entity).or_insert(0.0);
        *entry = entry.max(dv);
    }

    for (entity, mode, body, mut health, mut air, mut burning) in &mut query {
        if mode.0 != GameMode::Survival {
            air.secs = MAX_AIR_SECS;
            burning.secs = 0.0;
            continue;
        }
        let sample = sample_hazards(&sim.0, &registry.0, &body.0);
        let mut damage = sample.contact_dps * TICK_DT;
        if sample.hot {
            burning.secs = BURN_SECS;
        }
        if sample.extinguish {
            burning.secs = 0.0;
        }
        if burning.active() {
            damage += BURN_DPS * TICK_DT;
            burning.secs = (burning.secs - TICK_DT).max(0.0);
        }
        if sample.head_submerged {
            air.secs = (air.secs - TICK_DT).max(0.0);
            if air.secs <= 0.0 {
                damage += DROWN_DPS * TICK_DT;
            }
        } else {
            air.secs = (air.secs + AIR_REFILL_MULT * TICK_DT).min(MAX_AIR_SECS);
        }
        if let Some(&dv) = crush.get(&entity) {
            damage += crush_damage(dv);
        }
        if damage > 0.0 {
            health.hp -= damage;
            health.last_damage_tick = tick;
        } else if health.hp < MAX_HP
            && tick.saturating_sub(health.last_damage_tick) >= REGEN_DELAY_TICKS
        {
            health.hp = (health.hp + REGEN_RATE * TICK_DT).min(MAX_HP);
        }
    }
}
