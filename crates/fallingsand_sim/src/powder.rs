use crate::{
    motion::{AGITATED, GRAVITY_DV, can_enter, prefer_side, write_velocity},
    window::SimWindow,
};
use fallingsand_core::content::MatSpec;
use fallingsand_core::{Cell, CellPos, MaterialId, Phase, PowderDynamics, content};
use fallingsand_math::{Hash, Rng};

const TOPPLE_RESISTANCE_SALT: Hash = Hash::label("simulation.topple_resistance");

pub(crate) fn apply_effects<M: MatSpec>(
    window: &mut SimWindow,
    pos: CellPos,
    cell: Cell,
    dynamics: PowderDynamics,
    rng: &mut Rng,
) {
    let (mut vx, mut vy) = cell.vel();
    let grounded = !can_enter(window, cell.material, -1, pos.translated(0, -1));
    if grounded {
        vx = dynamics.ground_friction_keep.apply(vx);
    } else {
        vy -= buoyant_gravity::<M>(ambient_density(window, pos));
    }
    let drag = if window
        .get(pos.translated(0, 1))
        .is_some_and(|above| content::phase(above.material) == Phase::Liquid)
    {
        dynamics.submerged_drag_keep
    } else {
        dynamics.air_drag_keep
    };
    vx = drag.apply(vx);
    vy = drag.apply(vy);
    if grounded {
        topple(window, pos, cell, dynamics, &mut vx, vy, rng);
        if let Some(load) = transfer_load(window, pos, cell.material, vy) {
            let mut written = cell;
            written.set_vel(vx, -load.kept);
            written.set_stressed();
            if !load.saturated {
                written.set_moved();
            }
            window.set(pos, written);
            return;
        }
    }
    if cell.is_stressed() {
        let mut released = cell;
        released.clear_stressed();
        released.set_vel(vx, vy);
        window.set(pos, released);
        return;
    }
    write_velocity(window, pos, cell, vx, vy, grounded);
}

fn flux_cap(material: MaterialId) -> i32 {
    content::repose_layers(material) as i32 * GRAVITY_DV
}

struct Load {
    kept: i32,
    saturated: bool,
}

fn retain(carried: i32, sent: i32, cap: i32) -> Load {
    let kept = carried - sent;
    if kept > cap {
        Load {
            kept: cap,
            saturated: true,
        }
    } else {
        Load {
            kept: kept.max(0),
            saturated: false,
        }
    }
}

fn transfer_load(
    window: &mut SimWindow,
    pos: CellPos,
    material: MaterialId,
    vy: i32,
) -> Option<Load> {
    let below = pos.translated(0, -1);
    let support = window.get(below)?;
    let cap = flux_cap(material);
    if cap == 0 {
        return None;
    }
    let carried = GRAVITY_DV + (-vy).max(0);
    let mass = i64::from(content::density_milli(material).max(1));
    if let Some(id) = support.body_id() {
        let send = carried.min(cap);
        window.body_impulse(id, below, 0, -i64::from(send) * mass);
        return Some(retain(carried, send, cap));
    }
    if content::phase(support.material) != Phase::Powder || !support.is_stressed() {
        return None;
    }
    let (support_vx, support_vy) = support.vel();
    let support_mass = i64::from(content::density_milli(support.material).max(1));
    let room = (i64::from(flux_cap(support.material)) + i64::from(support_vy.min(0))).max(0);
    let send = carried.min(cap);
    let deposited = (i64::from(send) * mass / support_mass).min(room) as i32;
    if deposited > 0 {
        let mut written = support;
        written.set_vel(support_vx, support_vy - deposited);
        window.set(below, written);
    }
    let sent = (i64::from(deposited) * support_mass / mass) as i32;
    Some(retain(carried, sent, cap))
}

fn topple(
    window: &mut SimWindow,
    pos: CellPos,
    cell: Cell,
    dynamics: PowderDynamics,
    vx: &mut i32,
    vy: i32,
    rng: &mut Rng,
) {
    let kinetic = vx.abs() >= AGITATED || vy.abs() >= AGITATED;
    let loaded = window
        .get(pos.translated(0, 1))
        .is_some_and(|above| content::phase(above.material) == Phase::Powder);
    let mut resistance = Hash::seed(u64::from(cell.material.0) << 8 | u64::from(cell.shade))
        .salt(TOPPLE_RESISTANCE_SALT)
        .pos(pos.x, pos.y)
        .rng();
    let rng = if kinetic {
        rng
    } else if loaded {
        &mut resistance
    } else {
        return;
    };
    let unsheltered = can_enter(window, cell.material, -1, pos.translated(-1, -1))
        && can_enter(window, cell.material, -1, pos.translated(1, -1));
    let threshold = if kinetic {
        dynamics.topple_keep_threshold
    } else {
        dynamics.topple_start_threshold
    };
    let preferred = prefer_side(*vx, rng);
    for side in [preferred, -preferred] {
        let open = can_enter(window, cell.material, 0, pos.translated(side, 0))
            && can_enter(window, cell.material, -1, pos.translated(side, -1));
        if open && (unsheltered || rng.draw().below(threshold)) {
            *vx += side * dynamics.deflect_keep.apply(vy.abs()).max(AGITATED);
            return;
        }
    }
}

fn ambient_density(window: &SimWindow, pos: CellPos) -> i32 {
    window
        .get(pos.translated(0, -1))
        .filter(|cell| matches!(content::phase(cell.material), Phase::Liquid | Phase::Gas))
        .map_or(const { content::density_milli(MaterialId::AIR) }, |cell| {
            content::density_milli(cell.material)
        })
}

fn buoyant_gravity<M: MatSpec>(ambient: i32) -> i32 {
    let density = const { M::DENSITY_MILLI } as i64;
    let submerged = (density - i64::from(ambient)).clamp(0, density);
    ((i64::from(GRAVITY_DV) * submerged + density / 2) / density) as i32
}
