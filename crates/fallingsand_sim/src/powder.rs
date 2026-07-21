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
    }
    write_velocity(window, pos, cell, vx, vy, grounded);
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
    let threshold = if kinetic {
        dynamics.topple_keep_threshold
    } else {
        dynamics.topple_start_threshold
    };
    let preferred = prefer_side(*vx, rng);
    for side in [preferred, -preferred] {
        let open = can_enter(window, cell.material, 0, pos.translated(side, 0))
            && can_enter(window, cell.material, -1, pos.translated(side, -1));
        if open && rng.draw().below(threshold) {
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
