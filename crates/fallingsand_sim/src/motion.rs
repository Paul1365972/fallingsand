use crate::window::{SPEED_OF_LIGHT, SimWindow};
use fallingsand_core::{
    CARDINAL_NEIGHBORS as NEIGHBORS, Cell, CellPos, MaterialId, Phase, TICK_DT, Tag,
    VelocityFactor, content,
};
use fallingsand_math::{Hash, Rng, SUBCELL_BITS, SUBCELL_UNITS_PER_CELL};

const GRID_GRAVITY: f32 = 600.0;
const MOVEMENT_SALT: Hash = Hash::label("simulation.movement");
const LIQUID_WAKE_SALT: Hash = Hash::label("simulation.liquid_wake");
const MAX_COMPONENT_CELLS: i32 = 31;
const _: () = assert!(MAX_COMPONENT_CELLS < SPEED_OF_LIGHT);
const MAX_COMPONENT_RAW: i32 = MAX_COMPONENT_CELLS * SUBCELL_UNITS_PER_CELL;
const SETTLE: i32 = (7.5 * TICK_DT * SUBCELL_UNITS_PER_CELL as f32) as i32;
pub(crate) const GRAVITY_DV: i32 =
    (GRID_GRAVITY * TICK_DT * TICK_DT * SUBCELL_UNITS_PER_CELL as f32 + 0.5) as i32;
pub(crate) const AGITATED: i32 = 2 * GRAVITY_DV;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Entry {
    Open,
    Busy,
    Blocked,
}

pub(crate) enum TraverseControl {
    Continue,
    Revector(i32, i32),
}

pub(crate) struct Travel {
    pub pos: CellPos,
    pub blocked: [i32; 2],
}

pub(crate) fn traverse(
    window: &mut SimWindow,
    pos: CellPos,
    vx: i32,
    vy: i32,
    rng: &mut Rng,
    mut entry: impl FnMut(&SimWindow, (i32, i32), CellPos) -> Entry,
    mut swap: impl FnMut(&mut SimWindow, CellPos, CellPos) -> TraverseControl,
) -> Travel {
    let mut steps = [step_cells(vx, rng), step_cells(vy, rng)];
    let mut distance = [steps[0].abs(), steps[1].abs()];
    let mut done = [0, 0];
    let mut remaining = distance[0] + distance[1];
    let mut travel = Travel {
        pos,
        blocked: [0, 0],
    };
    while done != distance && remaining > 0 {
        let axis = if done[0] == distance[0] {
            1
        } else if done[1] == distance[1] || done[0] * distance[1] <= done[1] * distance[0] {
            0
        } else {
            1
        };
        let sign = steps[axis].signum();
        let dir = if axis == 0 { (sign, 0) } else { (0, sign) };
        let next = travel.pos.translated(dir.0, dir.1);
        let stop = match entry(window, dir, next) {
            Entry::Open => {
                let control = swap(window, travel.pos, next);
                travel.pos = next;
                done[axis] += 1;
                remaining -= 1;
                if let TraverseControl::Revector(vx, vy) = control {
                    steps = [step_cells(vx, rng), step_cells(vy, rng)];
                    distance = [steps[0].abs(), steps[1].abs()];
                    done = [0, 0];
                    travel.blocked = [0, 0];
                }
                false
            }
            Entry::Busy => {
                window.mark(travel.pos);
                true
            }
            Entry::Blocked => {
                travel.blocked[axis] = sign;
                true
            }
        };
        if stop {
            done[axis] = distance[axis];
        }
    }
    travel
}

pub(crate) fn move_cell(window: &mut SimWindow, pos: CellPos, cell: Cell, tick: u64) {
    let (mut vx, mut vy) = cell.vel();
    let material = cell.material;
    let phase = content::phase(material);
    let mut rng = movement_rng(tick, pos);
    let travel = traverse(
        window,
        pos,
        vx,
        vy,
        &mut rng,
        |window, dir, target| entry(window, material, dir.1, target),
        |window, from, to| swap_dynamic(window, from, to, tick),
    );
    if let Some(current) = window.get(travel.pos) {
        (vx, vy) = current.vel();
    }
    let restitution = VelocityFactor::from_raw(content::restitution_q16(material));
    for (dx, dy) in NEIGHBORS {
        let incoming = if dx != 0 { vx * dx > 0 } else { vy * dy > 0 };
        let target = travel.pos.translated(dx, dy);
        if !incoming || can_enter(window, material, dy, target) {
            continue;
        }
        let Some(blocker) = window.get(target) else {
            continue;
        };
        let blocker_phase = content::phase(blocker.material);
        let dynamic = matches!(blocker_phase, Phase::Powder | Phase::Liquid | Phase::Gas);
        let velocity = if dx != 0 { &mut vx } else { &mut vy };
        let pixel_body =
            blocker.is_body() && !content::tags(blocker.material).contains(Tag::Player);
        *velocity = if pixel_body || dynamic {
            transfer_momentum(window, material, target, (dx, dy), *velocity, restitution)
        } else if blocker_phase == phase {
            *velocity / 2
        } else {
            -restitution.apply(*velocity)
        };
    }
    let gravity_direction = if phase == Phase::Gas { 1 } else { -1 };
    let settled = !can_enter(
        window,
        material,
        gravity_direction,
        travel.pos.translated(0, gravity_direction),
    );
    let Some(current) = window.get(travel.pos) else {
        return;
    };
    write_velocity(window, travel.pos, current, vx, vy, settled);
}

fn swap_dynamic(window: &mut SimWindow, from: CellPos, to: CellPos, tick: u64) -> TraverseControl {
    let (Some(mover), Some(displaced)) = (window.get(from), window.get(to)) else {
        return TraverseControl::Continue;
    };
    if content::phase(mover.material) == Phase::Powder
        && content::phase(displaced.material) == Phase::Liquid
    {
        swap_through_liquid(window, from, to, tick).map_or(TraverseControl::Continue, |(vx, vy)| {
            TraverseControl::Revector(vx, vy)
        })
    } else {
        window.swap(from, to);
        TraverseControl::Continue
    }
}

pub(crate) fn swap_through_liquid(
    window: &mut SimWindow,
    from: CellPos,
    to: CellPos,
    tick: u64,
) -> Option<(i32, i32)> {
    let (Some(mut mover), Some(mut displaced)) = (window.get(from), window.get(to)) else {
        return None;
    };
    let mover_mass = i64::from(content::density_milli(mover.material).max(1));
    let displaced_mass = i64::from(content::density_milli(displaced.material).max(1));
    let mass = mover_mass + displaced_mass;
    let center_vx = divide_signed(
        mover_mass * i64::from(mover.vx) + displaced_mass * i64::from(displaced.vx),
        mass,
    ) as i32;
    let center_vy = divide_signed(
        mover_mass * i64::from(mover.vy) + displaced_mass * i64::from(displaced.vy),
        mass,
    ) as i32;
    let retention = liquid_wake_keep(mover.material, displaced.material);
    let (forward, transverse) = density_scatter(retention, mover_mass, displaced_mass);
    let side = if Hash::seed(tick)
        .salt(LIQUID_WAKE_SALT)
        .pos(to.x, to.y)
        .bit()
    {
        -1
    } else {
        1
    };
    let relative_vx = i32::from(mover.vx) - i32::from(displaced.vx);
    let relative_vy = i32::from(mover.vy) - i32::from(displaced.vy);
    let wake_vx = forward.apply(relative_vx) - side * transverse.apply(relative_vy);
    let wake_vy = forward.apply(relative_vy) + side * transverse.apply(relative_vx);
    let density_vx = side
        * density_exchange_speed(
            mover.material,
            displaced.material,
            mover_mass,
            displaced_mass,
            from,
            to,
        );
    let mover_vx =
        center_vx + divide_signed(displaced_mass * i64::from(wake_vx), mass) as i32 + density_vx;
    let mover_vy = center_vy + divide_signed(displaced_mass * i64::from(wake_vy), mass) as i32;
    mover.set_vel(mover_vx, mover_vy);
    displaced.set_vel(
        center_vx
            - divide_signed(mover_mass * i64::from(wake_vx), mass) as i32
            - divide_signed(mover_mass * i64::from(density_vx), displaced_mass) as i32,
        center_vy - divide_signed(mover_mass * i64::from(wake_vy), mass) as i32,
    );
    window.set(from, mover);
    window.set(to, displaced);
    window.swap(from, to);
    Some((mover_vx, mover_vy))
}

fn density_exchange_speed(
    mover: MaterialId,
    displaced: MaterialId,
    mover_mass: i64,
    displaced_mass: i64,
    from: CellPos,
    to: CellPos,
) -> i32 {
    if content::phase(mover) != Phase::Liquid
        || content::phase(displaced) != Phase::Liquid
        || to.y >= from.y
        || mover_mass <= displaced_mass
    {
        return 0;
    }
    let potential = 2
        * i128::from(GRAVITY_DV)
        * i128::from(SUBCELL_UNITS_PER_CELL)
        * i128::from(mover_mass - displaced_mass)
        * i128::from(displaced_mass);
    let inertia = i128::from(mover_mass) * i128::from(mover_mass + displaced_mass);
    (potential / inertia).isqrt() as i32
}

fn liquid_wake_keep(mover: MaterialId, displaced: MaterialId) -> VelocityFactor {
    let displaced_retention = content::liquid_impact_q16(displaced);
    let retained = if content::phase(mover) == Phase::Liquid {
        displaced_retention.min(content::liquid_impact_q16(mover))
    } else {
        displaced_retention
    };
    VelocityFactor::from_raw(retained)
}

fn density_scatter(
    retention: VelocityFactor,
    mover_mass: i64,
    displaced_mass: i64,
) -> (VelocityFactor, VelocityFactor) {
    let density_delta = mover_mass.abs_diff(displaced_mass);
    let density_max = mover_mass.max(displaced_mass) as u64;
    let contrast_q32 = (u128::from(density_delta) << 32) / u128::from(density_max);
    let forward_q16 = contrast_q32.isqrt() as u32;
    let transverse_q16 = ((1u128 << 32) - contrast_q32).isqrt() as u32;
    (
        combine_factors(retention, forward_q16),
        combine_factors(retention, transverse_q16),
    )
}

fn combine_factors(a: VelocityFactor, b_q16: u32) -> VelocityFactor {
    let product = u64::from(a.raw()) * u64::from(b_q16);
    VelocityFactor::from_raw(((product + (1 << 15)) >> 16) as u32)
}

pub(crate) fn movement_rng(tick: u64, pos: CellPos) -> Rng {
    Hash::seed(tick).salt(MOVEMENT_SALT).pos(pos.x, pos.y).rng()
}

pub(crate) fn write_velocity(
    window: &mut SimWindow,
    pos: CellPos,
    cell: Cell,
    mut vx: i32,
    mut vy: i32,
    settled: bool,
) {
    if settled && vector_length(vx, vy) < SETTLE {
        vx = 0;
        vy = 0;
    }
    vx = vx.clamp(-MAX_COMPONENT_RAW, MAX_COMPONENT_RAW);
    vy = vy.clamp(-MAX_COMPONENT_RAW, MAX_COMPONENT_RAW);
    if cell.vx as i32 != vx || cell.vy as i32 != vy {
        let mut written = cell;
        written.set_vel(vx, vy);
        window.set(pos, written);
    } else if vx != 0 || vy != 0 {
        window.mark(pos);
    }
}

pub(crate) fn prefer_side(vx: i32, rng: &mut Rng) -> i32 {
    match vx.signum() {
        0 => {
            if rng.draw().bit() {
                1
            } else {
                -1
            }
        }
        side => side,
    }
}

pub(crate) fn vector_length(vx: i32, vy: i32) -> i32 {
    (i64::from(vx) * i64::from(vx) + i64::from(vy) * i64::from(vy)).isqrt() as i32
}

fn step_cells(velocity: i32, rng: &mut Rng) -> i32 {
    let magnitude = velocity.abs();
    let fraction = magnitude % SUBCELL_UNITS_PER_CELL;
    let cells = magnitude / SUBCELL_UNITS_PER_CELL
        + i32::from((rng.draw().bits(SUBCELL_BITS) as i32) < fraction);
    cells.min(MAX_COMPONENT_CELLS) * velocity.signum()
}

pub(crate) fn can_enter(window: &SimWindow, mover: MaterialId, dy: i32, target: CellPos) -> bool {
    window
        .get(target)
        .is_some_and(|cell| admits(mover, dy, cell))
}

pub(crate) fn entry(window: &SimWindow, mover: MaterialId, dy: i32, target: CellPos) -> Entry {
    let Some(cell) = window.get(target) else {
        return Entry::Blocked;
    };
    if !admits(mover, dy, cell) {
        return Entry::Blocked;
    }
    if !cell.is_air() && cell.flags & Cell::MOVED != 0 {
        return Entry::Busy;
    }
    Entry::Open
}

fn admits(mover: MaterialId, dy: i32, target: Cell) -> bool {
    if !matches!(
        content::phase(target.material),
        Phase::Empty | Phase::Liquid | Phase::Gas
    ) {
        return false;
    }
    let mover_density = content::density_milli(mover);
    let target_density = content::density_milli(target.material);
    target_density < mover_density
        || (dy > 0 && target_density > mover_density)
        || (dy == 0 && target.is_air())
}

pub(crate) fn transfer_momentum(
    window: &mut SimWindow,
    mover: MaterialId,
    target: CellPos,
    direction: (i32, i32),
    velocity: i32,
    restitution: VelocityFactor,
) -> i32 {
    let Some(mut blocker) = window.get(target) else {
        return velocity;
    };
    let horizontal = direction.0 != 0;
    let sign = if horizontal { direction.0 } else { direction.1 };
    let blocker_velocity = if horizontal {
        i32::from(blocker.vx)
    } else {
        i32::from(blocker.vy)
    };
    let closing = (velocity - blocker_velocity) * sign;
    if closing <= 0 {
        return velocity;
    }
    let mover_mass = i64::from(content::density_milli(mover).max(1));
    let blocker_mass = i64::from(content::density_milli(blocker.material).max(1));
    let restitution = if content::phase(mover) == content::phase(blocker.material) {
        0
    } else {
        restitution
            .raw()
            .min(content::restitution_q16(blocker.material))
    };
    let impulse = i64::from(1u32 << 16) + i64::from(restitution);
    let denominator = (mover_mass + blocker_mass) * i64::from(1u32 << 16);
    let mover_delta =
        divide_signed(i64::from(closing) * blocker_mass * impulse, denominator) as i32;
    let blocker_delta =
        divide_signed(i64::from(closing) * mover_mass * impulse, denominator) as i32;
    let received = blocker_velocity + sign * blocker_delta;
    if horizontal {
        blocker.set_vel(received, i32::from(blocker.vy));
    } else {
        blocker.set_vel(i32::from(blocker.vx), received);
    }
    blocker.flags |= Cell::MOVED;
    window.set(target, blocker);
    velocity - sign * mover_delta
}

fn divide_signed(numerator: i64, denominator: i64) -> i64 {
    let magnitude = (numerator.abs() + denominator / 2) / denominator;
    if numerator < 0 { -magnitude } else { magnitude }
}
