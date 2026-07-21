use crate::{
    motion::{Entry, GRAVITY_DV, prefer_side, traverse, vector_length, write_velocity},
    window::SimWindow,
};
use fallingsand_core::{Cell, CellPos, LiquidDynamics, Phase, VelocityFactor, content};
use fallingsand_math::{Hash, Rng};

const MOVEMENT_SALT: Hash = Hash::label("simulation.movement");
const SURFACE_DIRECTION_SALT: Hash = Hash::label("simulation.liquid_surface_direction");
const QUADRATIC_DRAG_Q16: i64 = 918;

pub(crate) fn apply_effects(
    window: &mut SimWindow,
    pos: CellPos,
    cell: Cell,
    dynamics: LiquidDynamics,
) {
    let (mut vx, mut vy) = cell.vel();
    let falling = can_fall_into(window, cell, pos.translated(0, -1));
    if falling {
        vy -= GRAVITY_DV;
    }
    (vx, vy) = apply_drag(dynamics, vx, vy);
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
        relax(window, pos, cell, &mut rng);
        return;
    }

    let travel = traverse(
        window,
        pos,
        vx,
        vy,
        &mut rng,
        |window, _, target| entry(window, target),
        |window, from, to| window.swap(from, to),
    );
    let Some(current) = window.get(travel.pos) else {
        return;
    };
    (vx, vy) = current.vel();
    let impact = VelocityFactor::from_raw(content::liquid_impact_q16(cell.material));
    if travel.blocked[1] < 0 {
        (vx, vy) = redirect_impact(window, travel.pos, vx, vy, impact, &mut rng);
    } else {
        if travel.blocked[1] > 0 {
            vy = 0;
        }
        if travel.blocked[0] != 0 {
            vx = -impact.apply(vx);
        }
    }
    let settled = !can_fall_into(window, current, travel.pos.translated(0, -1));
    write_velocity(window, travel.pos, current, vx, vy, settled);
}

fn relax(window: &mut SimWindow, pos: CellPos, cell: Cell, rng: &mut Rng) {
    let side = prefer_side(0, rng);
    let mut target = [(0, -1), (side, -1), (-side, -1)]
        .into_iter()
        .find_map(|(dx, dy)| {
            let target = pos.translated(dx, dy);
            can_fall_into(window, cell, target).then_some(target)
        });
    if target.is_none() && exposed(window, pos, cell) {
        let row_side = if Hash::seed(u64::from(cell.material.0))
            .salt(SURFACE_DIRECTION_SALT)
            .pos(0, pos.y)
            .bit()
        {
            -1
        } else {
            1
        };
        target = [row_side, -row_side]
            .into_iter()
            .map(|dx| pos.translated(dx, 0))
            .find(|&target| supported_interface(window, cell, target));
    }
    let Some(target) = target else {
        return;
    };
    match entry(window, target) {
        Entry::Open if rng.draw().below(content::flow_threshold(cell.material)) => {
            window.swap(pos, target);
        }
        Entry::Open | Entry::Busy => window.mark(pos),
        Entry::Blocked => {}
    }
}

fn redirect_impact(
    window: &SimWindow,
    pos: CellPos,
    vx: i32,
    vy: i32,
    keep: VelocityFactor,
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
    if !cell.is_air() && cell.flags & Cell::MOVED != 0 {
        return Entry::Busy;
    }
    Entry::Open
}

fn dynamic(cell: Cell) -> bool {
    !cell.is_body()
        && matches!(
            content::phase(cell.material),
            Phase::Empty | Phase::Liquid | Phase::Gas
        )
}

fn can_fall_into(window: &SimWindow, mover: Cell, target: CellPos) -> bool {
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
