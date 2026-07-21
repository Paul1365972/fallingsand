use super::RawMaterial;
use crate::{EmissionDef, GasDef, LiquidDef, PhaseDef, PowderDef};
use fallingsand_material::{
    Dynamics, GasDynamics, LiquidDynamics, Phase, PowderDynamics, VelocityFactor,
};
use fallingsand_math::{SUBCELL_UNITS_PER_CELL, TICK_DT, chance_threshold};

pub(super) fn per_tick_chance(rate: f32) -> f32 {
    1.0 - (-rate * TICK_DT).exp()
}

fn per_tick_keep(rate: f32) -> f32 {
    (-rate * TICK_DT).exp()
}

fn quantize_q16(value: f32) -> u32 {
    (f64::from(value) * 65536.0).round() as u32
}

fn velocity_factor(value: f32) -> VelocityFactor {
    VelocityFactor::from_raw(quantize_q16(value))
}

pub(super) fn milli(value: f32) -> i32 {
    (f64::from(value) * 1000.0).round() as i32
}

pub(super) fn phase_tag(phase: PhaseDef) -> Phase {
    match phase {
        PhaseDef::Empty => Phase::Empty,
        PhaseDef::Solid(_) => Phase::Solid,
        PhaseDef::Powder(_) => Phase::Powder,
        PhaseDef::Liquid(_) => Phase::Liquid,
        PhaseDef::Gas(_) => Phase::Gas,
    }
}

fn srgb_to_linear(channel: u8) -> f32 {
    let s = channel as f32 / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

pub(super) fn bake_emission(def: Option<EmissionDef>) -> ([f32; 3], f32) {
    match def {
        Some(emission) => (
            [
                srgb_to_linear(emission.color[0]) * emission.intensity,
                srgb_to_linear(emission.color[1]) * emission.intensity,
                srgb_to_linear(emission.color[2]) * emission.intensity,
            ],
            emission.flicker,
        ),
        None => ([0.0; 3], 0.0),
    }
}

fn drag_keeps(air_drag: f32) -> (VelocityFactor, VelocityFactor) {
    let drag_loss = 1.0 - per_tick_keep(air_drag);
    (
        velocity_factor(1.0 - drag_loss.min(0.9)),
        velocity_factor(1.0 - (drag_loss * 6.0).min(0.9)),
    )
}

pub(super) fn quantize_dynamics(raw: &RawMaterial) -> Dynamics {
    match raw.phase {
        PhaseDef::Empty | PhaseDef::Solid(_) => Dynamics::None,
        PhaseDef::Powder(PowderDef {
            air_drag,
            ground_friction,
            topple_start,
            topple_keep,
            deflect,
        }) => {
            let (air_drag_keep, submerged_drag_keep) = drag_keeps(air_drag);
            Dynamics::Powder(PowderDynamics {
                air_drag_keep,
                submerged_drag_keep,
                ground_friction_keep: velocity_factor(per_tick_keep(ground_friction)),
                deflect_keep: velocity_factor(deflect.clamp(0.0, 1.0)),
                topple_start_threshold: chance_threshold(per_tick_chance(topple_start)),
                topple_keep_threshold: chance_threshold(per_tick_chance(topple_keep)),
            })
        }
        PhaseDef::Liquid(LiquidDef {
            drag,
            impact,
            flow_rate: _,
        }) => Dynamics::Liquid(LiquidDynamics {
            drag_keep: velocity_factor(per_tick_keep(drag)),
            impact_keep: velocity_factor(impact.clamp(0.0, 1.0)),
        }),
        PhaseDef::Gas(GasDef {
            air_drag,
            turbulence,
            flow_rate: _,
        }) => Dynamics::Gas(GasDynamics {
            air_drag_keep: drag_keeps(air_drag).0,
            turbulence_q16: quantize_q16(
                turbulence * TICK_DT.sqrt() * TICK_DT * SUBCELL_UNITS_PER_CELL as f32,
            ),
        }),
    }
}

pub(super) fn flow_threshold(raw: &RawMaterial) -> u64 {
    let flow_rate = match raw.phase {
        PhaseDef::Liquid(LiquidDef { flow_rate, .. }) | PhaseDef::Gas(GasDef { flow_rate, .. }) => {
            flow_rate
        }
        _ => return 0,
    };
    flow_rate.map_or(u64::MAX, |rate| chance_threshold(per_tick_chance(rate)))
}

pub(super) fn compile_liquid_exchange_thresholds(raws: &[RawMaterial]) -> Vec<u64> {
    let mut thresholds = Vec::with_capacity(raws.len() * raws.len());
    for a in raws {
        for b in raws {
            thresholds.push(liquid_exchange_threshold(a, b));
        }
    }
    thresholds
}

fn liquid_exchange_threshold(a: &RawMaterial, b: &RawMaterial) -> u64 {
    let (PhaseDef::Liquid(a_liquid), PhaseDef::Liquid(b_liquid)) = (a.phase, b.phase) else {
        return 0;
    };
    let a_density = milli(a.density);
    let b_density = milli(b.density);
    if a_density == b_density {
        return 0;
    }
    let rate = match (a_liquid.flow_rate, b_liquid.flow_rate) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(rate), None) | (None, Some(rate)) => Some(rate),
        (None, None) => None,
    };
    rate.map_or(u64::MAX, |rate| {
        let density_delta = (a_density - b_density).abs() as f32;
        let density_max = a_density.max(b_density) as f32;
        let drive = (density_delta / density_max).sqrt();
        chance_threshold(per_tick_chance(rate * drive))
    })
}
