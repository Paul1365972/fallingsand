use crate::{
    motion::{
        Entry, GRAVITY_DV, TraverseControl, prefer_side, strike_body, swap_through_liquid,
        traverse, vector_length, write_velocity,
    },
    window::SimWindow,
};
use fallingsand_core::{Cell, CellPos, LiquidDynamics, Phase, Q16, content};
use fallingsand_math::{Hash, Rng, SUBCELL_UNITS_PER_CELL};

const MOVEMENT_SALT: Hash = Hash::label("simulation.movement");
const QUADRATIC_DRAG_Q16: i64 = 918;
const DOWNHILL_LAUNCH: i32 = (2 * GRAVITY_DV * SUBCELL_UNITS_PER_CELL).isqrt();

pub(crate) fn apply_effects(
    window: &mut SimWindow,
    pos: CellPos,
    cell: Cell,
    dynamics: LiquidDynamics,
) {
    let (mut vx, mut vy) = cell.vel();
    let below = pos.translated(0, -1);
    let falling = can_fall_freely_into(window, cell, below);
    if falling {
        vy -= GRAVITY_DV;
    }
    let gliding = !falling
        && vy == 0
        && window
            .get(below)
            .is_some_and(|support| content::phase(support.material) == Phase::Liquid);
    if gliding {
        let ahead = pos.translated(vx.signum(), 0);
        if vx != 0 && !window.get(ahead).is_some_and(dynamic) {
            vx = 0;
        } else {
            vx = dynamics.glide_keep.apply(vx);
        }
    } else {
        (vx, vy) = apply_drag(dynamics, vx, vy);
    }
    write_velocity(window, pos, cell, vx, vy, !falling);
}

fn apply_drag(dynamics: LiquidDynamics, vx: i32, vy: i32) -> (i32, i32) {
    let vx = dynamics.drag_keep.apply(vx);
    let vy = dynamics.drag_keep.apply(vy);
    let speed = i64::from(vector_length(vx, vy));
    let denominator = 65_536 + (QUADRATIC_DRAG_Q16 * speed + 128) / 256;
    (
        divide_signed(i64::from(vx) * 65_536, denominator) as i32,
        divide_signed(i64::from(vy) * 65_536, denominator) as i32,
    )
}

fn divide_signed(numerator: i64, denominator: i64) -> i64 {
    let result = (numerator.abs() + denominator / 2) / denominator;
    if numerator < 0 { -result } else { result }
}

pub(crate) fn move_cell(window: &mut SimWindow, pos: CellPos, cell: Cell, tick: u64) {
    let mut rng = Hash::seed(tick).salt(MOVEMENT_SALT).pos(pos.x, pos.y).rng();
    let (mut vx, mut vy) = cell.vel();
    if vx == 0 && vy == 0 {
        relax(window, pos, cell, tick, &mut rng);
        return;
    }

    let travel = traverse(
        window,
        pos,
        vx,
        vy,
        &mut rng,
        |window, _, target| entry(window, target),
        |window, from, to| swap_moving(window, from, to, tick),
    );
    let Some(current) = window.get(travel.pos) else {
        return;
    };
    (vx, vy) = current.vel();
    let impact = content::liquid_impact(cell.material);
    if travel.blocked[1] < 0 {
        strike_body(window, cell.material, travel.pos.translated(0, -1), 0, vy);
        (vx, vy) = redirect_impact(window, travel.pos, vx, vy, impact, &mut rng);
    } else {
        if travel.blocked[1] > 0 {
            strike_body(window, cell.material, travel.pos.translated(0, 1), 0, vy);
            vy = 0;
        }
        if travel.blocked[0] != 0 {
            let after = -impact.apply(vx);
            strike_body(
                window,
                cell.material,
                travel.pos.translated(travel.blocked[0], 0),
                vx - after,
                0,
            );
            vx = after;
        }
    }
    let settled = !can_fall_freely_into(window, current, travel.pos.translated(0, -1));
    write_velocity(window, travel.pos, current, vx, vy, settled);
}

fn relax(window: &mut SimWindow, pos: CellPos, cell: Cell, tick: u64, rng: &mut Rng) {
    let side = prefer_side(0, rng);
    let target = [(0, -1), (side, -1), (-side, -1)]
        .into_iter()
        .find_map(|(dx, dy)| {
            let target = pos.translated(dx, dy);
            can_exchange_downhill_into(window, cell, target).then_some(target)
        });
    let Some(target) = target else {
        relax_interface(window, pos, cell, rng);
        return;
    };
    match entry(window, target) {
        Entry::Open if rng.draw().below(passive_threshold(window, cell, target)) => {
            if window.get(target).is_some_and(|displaced| {
                content::phase(displaced.material) == Phase::Liquid
                    && displaced.material != cell.material
            }) {
                swap_through_liquid(window, pos, target, tick);
            } else {
                window.swap(pos, target);
                if target.x != pos.x {
                    let mut moved = cell;
                    moved.set_vel((target.x - pos.x) * DOWNHILL_LAUNCH, 0);
                    window.set(target, moved);
                }
            }
        }
        Entry::Open | Entry::Busy => window.mark(pos),
        Entry::Blocked => {}
    }
}

fn relax_interface(window: &mut SimWindow, pos: CellPos, cell: Cell, rng: &mut Rng) {
    if !exposed(window, pos, cell) {
        return;
    }
    let side = interface_direction(cell, pos);
    let target = pos.translated(side, 0);
    if !supported_interface(window, cell, target) {
        return;
    }
    match entry(window, target) {
        Entry::Open if rng.draw().below(passive_threshold(window, cell, target)) => {
            window.swap(pos, target);
        }
        Entry::Open | Entry::Busy => window.mark(pos),
        Entry::Blocked => {}
    }
}

fn interface_direction(cell: Cell, pos: CellPos) -> i32 {
    if (pos.y ^ i32::from(cell.material.0)) & 1 == 0 {
        1
    } else {
        -1
    }
}

fn swap_moving(window: &mut SimWindow, from: CellPos, to: CellPos, tick: u64) -> TraverseControl {
    let (Some(mover), Some(displaced)) = (window.get(from), window.get(to)) else {
        return TraverseControl::Continue;
    };
    if content::phase(displaced.material) == Phase::Liquid && displaced.material != mover.material {
        swap_through_liquid(window, from, to, tick).map_or(TraverseControl::Continue, |(vx, vy)| {
            TraverseControl::Revector(vx, vy)
        })
    } else {
        window.swap(from, to);
        TraverseControl::Continue
    }
}

fn passive_threshold(window: &SimWindow, mover: Cell, target: CellPos) -> u64 {
    window.get(target).map_or(0, |displaced| {
        if content::phase(displaced.material) == Phase::Liquid
            && displaced.material != mover.material
        {
            content::liquid_exchange_threshold(mover.material, displaced.material)
        } else {
            content::flow_threshold(mover.material)
        }
    })
}

fn redirect_impact(
    window: &SimWindow,
    pos: CellPos,
    vx: i32,
    vy: i32,
    keep: Q16,
    rng: &mut Rng,
) -> (i32, i32) {
    let speed = keep.apply(vector_length(vx, vy));
    let preferred = prefer_side(vx, rng);
    [preferred, -preferred]
        .into_iter()
        .find(|&side| entry(window, pos.translated(side, 0)) == Entry::Open)
        .map_or((0, 0), |side| (side * speed, 0))
}

fn entry(window: &SimWindow, target: CellPos) -> Entry {
    let Some(cell) = window.get(target) else {
        return Entry::Blocked;
    };
    if !dynamic(cell) {
        return Entry::Blocked;
    }
    if !cell.is_air() && cell.is_moved() {
        return Entry::Busy;
    }
    Entry::Open
}

fn dynamic(cell: Cell) -> bool {
    cell.body_id().is_none()
        && matches!(
            content::phase(cell.material),
            Phase::Empty | Phase::Liquid | Phase::Gas
        )
}

fn can_fall_freely_into(window: &SimWindow, mover: Cell, target: CellPos) -> bool {
    window.get(target).is_some_and(|cell| {
        dynamic(cell)
            && content::phase(cell.material) != Phase::Liquid
            && content::density_milli(mover.material) > content::density_milli(cell.material)
    })
}

fn can_exchange_downhill_into(window: &SimWindow, mover: Cell, target: CellPos) -> bool {
    window.get(target).is_some_and(|cell| {
        dynamic(cell)
            && content::density_milli(mover.material) > content::density_milli(cell.material)
    })
}

fn exposed(window: &SimWindow, pos: CellPos, mover: Cell) -> bool {
    window.get(pos.translated(0, 1)).is_some_and(|above| {
        dynamic(above)
            && content::density_milli(above.material) < content::density_milli(mover.material)
    })
}

fn supported_interface(window: &SimWindow, mover: Cell, target: CellPos) -> bool {
    let Some(displaced) = window.get(target) else {
        return false;
    };
    dynamic(displaced)
        && content::density_milli(displaced.material) < content::density_milli(mover.material)
        && window.get(target.translated(0, -1)).is_some_and(|below| {
            !dynamic(below)
                || content::density_milli(below.material) >= content::density_milli(mover.material)
        })
}
