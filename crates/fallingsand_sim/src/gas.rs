use crate::{
    motion::{Entry, GRAVITY_DV, can_enter, entry, movement_rng, prefer_side, write_velocity},
    window::SimWindow,
};
use fallingsand_core::{Cell, CellPos, GasDynamics, content};
use fallingsand_math::{Hash, Rng};

const TURBULENCE_SALT: Hash = Hash::label("simulation.turbulence");

pub(crate) fn apply_effects(
    window: &mut SimWindow,
    pos: CellPos,
    cell: Cell,
    dynamics: GasDynamics,
) {
    let (mut vx, mut vy) = cell.vel();
    let capped = !can_enter(window, cell.material, 1, pos.translated(0, 1));
    if !capped {
        vy += GRAVITY_DV;
    }
    vx = dynamics.air_drag_keep.apply(vx);
    vy = dynamics.air_drag_keep.apply(vy);
    if dynamics.turbulence_q16 != 0 {
        let noise = Hash::seed(u64::from(cell.material.0) << 8 | u64::from(cell.shade))
            .salt(TURBULENCE_SALT)
            .pos(pos.x, pos.y)
            .bits(16) as i64
            - 32768;
        vx += scaled_round(i64::from(dynamics.turbulence_q16) * noise, 31);
    }
    write_velocity(window, pos, cell, vx, vy, capped);
}

pub(crate) fn move_cell(window: &mut SimWindow, pos: CellPos, cell: Cell, tick: u64) {
    if cell.vel() != (0, 0) {
        crate::motion::move_cell(window, pos, cell, tick);
        return;
    }
    flow(window, pos, cell, &mut movement_rng(tick, pos));
}

fn flow(window: &mut SimWindow, pos: CellPos, cell: Cell, rng: &mut Rng) {
    let above = pos.translated(0, 1);
    match entry(window, cell.material, 1, above) {
        Entry::Open => {
            window.swap(pos, above);
            return;
        }
        Entry::Busy => {
            window.mark(pos);
            return;
        }
        Entry::Blocked => {}
    }
    let preferred = prefer_side(0, rng);
    for side in [preferred, -preferred] {
        let beside = pos.translated(side, 0);
        match entry(window, cell.material, 0, beside) {
            Entry::Open => {}
            Entry::Busy => {
                window.mark(pos);
                continue;
            }
            Entry::Blocked => continue,
        }
        let diagonal = pos.translated(side, 1);
        let target = match entry(window, cell.material, 1, diagonal) {
            Entry::Open => diagonal,
            Entry::Busy => {
                window.mark(pos);
                continue;
            }
            Entry::Blocked => beside,
        };
        if rng.draw().below(content::flow_threshold(cell.material)) {
            window.swap(pos, target);
        } else {
            window.mark(pos);
        }
        return;
    }
}

fn scaled_round(product: i64, shift: u32) -> i32 {
    let magnitude = (product.abs() + (1i64 << (shift - 1))) >> shift;
    (if product < 0 { -magnitude } else { magnitude }) as i32
}
