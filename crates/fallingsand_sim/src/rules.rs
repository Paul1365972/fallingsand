use crate::window::{SPEED_OF_LIGHT, SimWindow};
use fallingsand_core::content::{self, MatSpec, material};
use fallingsand_core::{
    Burning, BurningKind, CARDINAL_NEIGHBORS as NEIGHBORS, Cell, CellPos, Dynamics, GasDynamics,
    Ignition, LiquidDynamics, MaterialId, Phase, PowderDynamics, SealedBurn, TICK_DT, Tag,
    VelocityFactor,
};
use fallingsand_math::{Hash, Rng, SUBCELL_BITS, SUBCELL_UNITS_PER_CELL};

const GRID_GRAVITY: f32 = 600.0;
const EFFECT_SALT: Hash = Hash::label("simulation.effect");
const MOVEMENT_SALT: Hash = Hash::label("simulation.movement");
const MAX_STEP: i32 = 31;
const _: () = assert!(MAX_STEP < SPEED_OF_LIGHT);
const MAX_VELOCITY_RAW: i32 = MAX_STEP * SUBCELL_UNITS_PER_CELL;
const SETTLE: i32 = (7.5 * TICK_DT * SUBCELL_UNITS_PER_CELL as f32) as i32;
const GRAVITY_DV: i32 =
    (GRID_GRAVITY * TICK_DT * TICK_DT * SUBCELL_UNITS_PER_CELL as f32 + 0.5) as i32;
const AGITATED: i32 = 2 * GRAVITY_DV;
const FLOW_IMPULSE: i32 = 600;
const IMPACT: i32 = SUBCELL_UNITS_PER_CELL;
const SLOPE_DV: i32 = GRAVITY_DV / 2;

macro_rules! material_dispatch {
    ($(($idx:literal, $name:ident, $spec:ident)),* $(,)?) => {
        pub(crate) fn effect_cell(window: &mut SimWindow, pos: CellPos, tick: u64) {
            let Some(cell) = window.get(pos) else {
                return;
            };
            match cell.material.0 {
                $( $idx => effect_spec::<content::specs::$spec>(window, pos, cell, tick), )*
                _ => unreachable!("invalid material id"),
            }
        }
    };
}
fallingsand_core::for_each_material!(material_dispatch);

fn effect_spec<M: MatSpec>(window: &mut SimWindow, pos: CellPos, cell: Cell, tick: u64) {
    let mut rng = Hash::seed(tick).salt(EFFECT_SALT).pos(pos.x, pos.y).rng();
    if let Some(burning) = const { M::BURNING }
        && burning_step::<M>(window, pos, burning, &mut rng)
    {
        return;
    }
    if let Some(ignition) = const { M::IGNITION }
        && ignite_step(window, pos, ignition, &mut rng)
    {
        return;
    }
    if const { M::IS_REACTIVE } && react::<M>(window, pos, cell, &mut rng) {
        return;
    }
    match const { M::DYNAMICS } {
        Dynamics::None => {}
        Dynamics::Powder(d) => powder_effects::<M>(window, pos, cell, d, &mut rng),
        Dynamics::Liquid(d) => liquid_effects::<M>(window, pos, cell, d, &mut rng),
        Dynamics::Gas(d) => gas_effects(window, pos, cell, d, &mut rng),
    }
}

fn powder_effects<M: MatSpec>(
    window: &mut SimWindow,
    pos: CellPos,
    cell: Cell,
    d: PowderDynamics,
    rng: &mut Rng,
) {
    let (mut vx, mut vy) = cell.vel();
    let grounded = supported(window, cell, pos);
    if grounded {
        vx = d.ground_friction_keep.apply(vx);
    } else {
        vy -= buoyant_gravity::<M>(ambient_density_milli(window, pos));
    }
    let keep = if submerged(window, pos) {
        d.submerged_drag_keep
    } else {
        d.air_drag_keep
    };
    vx = keep.apply(vx);
    vy = keep.apply(vy);
    note_body_below(window, pos);
    if grounded {
        topple(window, pos, cell, d, &mut vx, vy, rng);
    }
    finish_effects(window, pos, cell, vx, vy, grounded);
}

fn topple(
    window: &mut SimWindow,
    pos: CellPos,
    cell: Cell,
    d: PowderDynamics,
    vx: &mut i32,
    vy: i32,
    rng: &mut Rng,
) {
    let kinetic = vx.abs() >= AGITATED || vy.abs() >= AGITATED;
    let loaded = window
        .get(pos.translated(0, 1))
        .is_some_and(|above| content::phase(above.material) == Phase::Powder);
    let threshold = if kinetic {
        d.topple_keep_threshold
    } else if loaded || neighbor_agitated(window, pos) {
        d.topple_start_threshold
    } else {
        return;
    };
    let prefer = prefer_side(*vx, rng);
    let mut pending = false;
    for side in [prefer, -prefer] {
        let open = can_enter(window, cell.material, (side, 0), pos.translated(side, 0))
            && can_enter(window, cell.material, (side, -1), pos.translated(side, -1));
        if !open {
            continue;
        }
        if rng.draw().below(threshold) {
            *vx += side * d.deflect_keep.apply(vy.abs()).max(AGITATED);
            return;
        }
        pending |= loaded;
    }
    if pending {
        window.mark(pos);
    }
}

fn liquid_effects<M: MatSpec>(
    window: &mut SimWindow,
    pos: CellPos,
    cell: Cell,
    d: LiquidDynamics,
    rng: &mut Rng,
) {
    let (cell, converged) = aux_relax(window, pos, cell);
    let (mut vx, mut vy) = cell.vel();
    let grounded = supported(window, cell, pos);
    if grounded {
        vx = d.ground_friction_keep.apply(vx);
        vy += buoyant_rise::<M>(window, pos);
    } else {
        vy -= buoyant_gravity::<M>(ambient_density_milli(window, pos));
    }
    let keep = if submerged(window, pos) {
        d.submerged_drag_keep
    } else {
        d.air_drag_keep
    };
    vx = keep.apply(vx);
    vy = keep.apply(vy);
    note_body_below(window, pos);
    cohesion(window, pos, &mut vx, &mut vy, d.cohesion, Phase::Liquid);
    slope_accelerate(window, pos, cell, &mut vx);
    current_accelerate(window, pos, cell, &mut vx);
    if converged && grounded {
        pressure_accelerate(window, pos, cell, &mut vx, &mut vy);
    }
    if grounded {
        ledge_impulse(window, pos, d, &mut vx, vy, rng);
    }
    if vy <= -IMPACT
        && window
            .get(pos.translated(0, -1))
            .is_some_and(|below| content::phase(below.material) == Phase::Liquid)
    {
        vx += prefer_side(vx, rng) * d.deflect_keep.apply(vy.abs() / 2);
    }
    finish_effects(window, pos, cell, vx, vy, grounded);
}

fn ledge_impulse(
    window: &mut SimWindow,
    pos: CellPos,
    d: LiquidDynamics,
    vx: &mut i32,
    vy: i32,
    rng: &mut Rng,
) {
    if !window
        .get(pos.translated(0, 1))
        .is_some_and(|above| above.is_air())
    {
        return;
    }
    let prefer = prefer_side(*vx, rng);
    let mut pending = false;
    for side in [prefer, -prefer] {
        let drop = window
            .get(pos.translated(side, 0))
            .is_some_and(|beside| beside.is_air())
            && window
                .get(pos.translated(side, -1))
                .is_some_and(|diag| diag.is_air());
        if !drop {
            continue;
        }
        if rng.draw().below(d.flow_threshold) {
            *vx += side * d.deflect_keep.apply(vy.abs().max(FLOW_IMPULSE));
            return;
        }
        pending = true;
    }
    if pending {
        window.mark(pos);
    }
}

fn pressure_accelerate(window: &SimWindow, pos: CellPos, cell: Cell, vx: &mut i32, vy: &mut i32) {
    let calm = |cell: Cell| (cell.vx as i32).abs() < SETTLE && (cell.vy as i32).abs() < SETTLE;
    let still = calm(cell)
        && NEIGHBORS
            .iter()
            .all(|&(dx, dy)| window.get(pos.translated(dx, dy)).is_none_or(calm));
    if !still {
        return;
    }
    let up = up_head(cell.aux) as i32;
    if up >= 2
        && window
            .get(pos.translated(0, 1))
            .is_some_and(|above| above.is_air())
    {
        *vy += SLOPE_DV * up.min(3);
    }
    let down = down_head(cell.aux) as i32;
    if down >= 3 {
        for side in [-1, 1] {
            if window
                .get(pos.translated(side, 0))
                .is_some_and(|beside| beside.is_air())
            {
                *vx += side * SLOPE_DV * down.min(4);
            }
        }
    }
}

fn gas_effects(window: &mut SimWindow, pos: CellPos, cell: Cell, d: GasDynamics, rng: &mut Rng) {
    let (mut vx, mut vy) = cell.vel();
    let capped = !can_enter(window, cell.material, (0, 1), pos.translated(0, 1));
    if !capped {
        vy += GRAVITY_DV;
    }
    vx = d.air_drag_keep.apply(vx);
    vy = d.air_drag_keep.apply(vy);
    if d.turbulence_q16 != 0 {
        let r = rng.draw().bits(16) as i64 - 32768;
        vx += scaled_round(d.turbulence_q16 as i64 * r, 31);
    }
    note_body_below(window, pos);
    cohesion(window, pos, &mut vx, &mut vy, d.cohesion, Phase::Gas);
    if capped {
        let prefer = prefer_side(vx, rng);
        for side in [prefer, -prefer] {
            let open = can_enter(window, cell.material, (side, 0), pos.translated(side, 0))
                && can_enter(window, cell.material, (side, 1), pos.translated(side, 1));
            if open {
                vx += side * d.deflect_keep.apply(vy.abs()).max(AGITATED);
                break;
            }
        }
    }
    finish_effects(window, pos, cell, vx, vy, capped);
}

fn finish_effects(
    window: &mut SimWindow,
    pos: CellPos,
    cell: Cell,
    mut vx: i32,
    mut vy: i32,
    settled: bool,
) {
    if settled {
        if vx.abs() < SETTLE {
            vx = 0;
        }
        if vy.abs() < SETTLE {
            vy = 0;
        }
    }
    vx = vx.clamp(-MAX_VELOCITY_RAW, MAX_VELOCITY_RAW);
    vy = vy.clamp(-MAX_VELOCITY_RAW, MAX_VELOCITY_RAW);
    if cell.vx as i32 != vx || cell.vy as i32 != vy {
        let mut written = cell;
        written.set_vel(vx, vy);
        window.set(pos, written);
    } else if vx != 0 || vy != 0 {
        window.mark(pos);
    }
}

pub(crate) fn move_cell(window: &mut SimWindow, pos: CellPos, tick: u64) {
    let Some(cell) = window.get(pos) else {
        return;
    };
    if cell.flags & Cell::MOVED != 0 {
        window.mark(pos);
        return;
    }
    if matches!(content::phase(cell.material), Phase::Empty | Phase::Solid) {
        return;
    }
    let (mut vx, mut vy) = cell.vel();
    if vx == 0 && vy == 0 {
        return;
    }
    let material = cell.material;
    let mut rng = Hash::seed(tick).salt(MOVEMENT_SALT).pos(pos.x, pos.y).rng();

    let tx = step_cells(vx, &mut rng);
    let ty = step_cells(vy, &mut rng);
    let (ix, iy) = (tx.abs(), ty.abs());
    let (sx, sy) = (tx.signum(), ty.signum());
    let mut cur = pos;
    let mut moved = false;
    let mut done_x = 0;
    let mut done_y = 0;
    while done_x < ix || done_y < iy {
        let step_x = if done_x == ix {
            false
        } else if done_y == iy {
            true
        } else {
            done_x * iy <= done_y * ix
        };
        let dir = if step_x { (sx, 0) } else { (0, sy) };
        let next = cur.translated(dir.0, dir.1);
        match entry(window, material, dir, next) {
            Entry::Open => {
                window.swap(cur, next);
                cur = next;
                moved = true;
                if step_x {
                    done_x += 1;
                } else {
                    done_y += 1;
                }
            }
            Entry::Busy => {
                window.mark(cur);
                if step_x {
                    done_x = ix;
                } else {
                    done_y = iy;
                }
            }
            Entry::Blocked => {
                if step_x {
                    done_x = ix;
                } else {
                    done_y = iy;
                }
            }
        }
    }
    if moved {
        note_undermined(window, pos);
    }

    let restitution = VelocityFactor::from_raw(content::restitution_q16(material));
    let phase = content::phase(material);
    for (dx, dy) in NEIGHBORS {
        let into = if dx != 0 { vx * dx > 0 } else { vy * dy > 0 };
        let target = cur.translated(dx, dy);
        if into && !can_enter(window, material, (dx, dy), target) {
            let crowds = window
                .get(target)
                .is_some_and(|blocker| content::phase(blocker.material) == phase);
            if dx != 0 {
                vx = if crowds {
                    vx / 2
                } else {
                    reflect(vx, restitution)
                };
            } else {
                vy = if crowds {
                    vy / 2
                } else {
                    reflect(vy, restitution)
                };
            }
        }
    }
    let gdir = if content::phase(material) == Phase::Gas {
        1
    } else {
        -1
    };
    let settled = !can_enter(window, material, (0, gdir), cur.translated(0, gdir));
    if settled {
        if vx.abs() < SETTLE {
            vx = 0;
        }
        if vy.abs() < SETTLE {
            vy = 0;
        }
    }
    vx = vx.clamp(-MAX_VELOCITY_RAW, MAX_VELOCITY_RAW);
    vy = vy.clamp(-MAX_VELOCITY_RAW, MAX_VELOCITY_RAW);
    let Some(current) = window.get(cur) else {
        return;
    };
    if current.vx as i32 != vx || current.vy as i32 != vy {
        let mut written = current;
        written.set_vel(vx, vy);
        window.set(cur, written);
    } else if vx != 0 || vy != 0 {
        window.mark(cur);
    }
}

pub(crate) fn random_tick(_window: &mut SimWindow, _pos: CellPos, _tick: u64) {}

fn react<M: MatSpec>(window: &mut SimWindow, pos: CellPos, cell: Cell, rng: &mut Rng) -> bool {
    let mut keep = false;
    for (dx, dy) in NEIGHBORS {
        let neighbor_pos = pos.translated(dx, dy);
        let Some(neighbor) = window.get(neighbor_pos) else {
            continue;
        };
        let reaction = M::REACTIONS[neighbor.material.0 as usize];
        if reaction.threshold != 0 {
            keep = true;
            if rng.draw().below(reaction.threshold) {
                note_structural(window, pos, cell.material);
                note_structural(window, neighbor_pos, neighbor.material);
                set_product(window, pos, reaction.becomes, rng);
                set_product(window, neighbor_pos, reaction.other_becomes, rng);
                return true;
            }
        }
    }
    if let Some((threshold, product)) = const { M::DECAY } {
        if rng.draw().below(threshold) {
            set_product(window, pos, product, rng);
            return true;
        }
        keep = true;
    }
    if keep {
        window.mark(pos);
    }
    false
}

fn note_structural(window: &mut SimWindow, pos: CellPos, material: MaterialId) {
    if content::phase(material) != Phase::Solid {
        return;
    }
    for (dx, dy) in NEIGHBORS {
        window.note_structural(pos.translated(dx, dy));
    }
}

fn ignite_step(window: &mut SimWindow, pos: CellPos, ignition: Ignition, rng: &mut Rng) -> bool {
    if ignition.open == 0 && ignition.sealed == 0 {
        return false;
    }
    let hot_neighbors = NEIGHBORS
        .iter()
        .filter(|&&(dx, dy)| {
            window
                .get(pos.translated(dx, dy))
                .is_some_and(|neighbor| content::tags(neighbor.material).contains(Tag::Hot))
        })
        .count();
    if hot_neighbors == 0 {
        return false;
    }
    let threshold = if oxygen_exposed(window, pos) {
        ignition.open
    } else {
        ignition.sealed
    };
    if threshold == 0 {
        return false;
    }
    window.mark(pos);
    for _ in 0..hot_neighbors {
        if rng.draw().below(threshold) {
            let Some(mut lit) = window.get(pos) else {
                return true;
            };
            lit.material = ignition.into;
            lit.set_body(false);
            window.set(pos, lit);
            return true;
        }
    }
    false
}

fn burning_step<M: MatSpec>(
    window: &mut SimWindow,
    pos: CellPos,
    burning: Burning,
    rng: &mut Rng,
) -> bool {
    if let Some(water) = adjacent_water(window, pos) {
        if burning.kind == BurningKind::Flame {
            set_product(window, pos, material::STEAM, rng);
        } else {
            burn_out::<M>(window, pos, burning, rng);
            set_product(window, water, material::STEAM, rng);
        }
        return true;
    }
    if rng.draw().below(burning.emit) {
        emit_into_air(window, pos, material::FIRE, rng);
    }
    let burn = if oxygen_exposed(window, pos) {
        burning.burn
    } else {
        match burning.sealed {
            SealedBurn::Becomes(next) => {
                let Some(mut cell) = window.get(pos) else {
                    return true;
                };
                cell.material = next;
                window.set(pos, cell);
                return true;
            }
            SealedBurn::Smoulder(threshold) => threshold,
        }
    };
    if rng.draw().below(burn) {
        burn_out::<M>(window, pos, burning, rng);
        return true;
    }
    window.mark(pos);
    false
}

fn burn_out<M: MatSpec>(window: &mut SimWindow, pos: CellPos, burning: Burning, rng: &mut Rng) {
    if const { matches!(M::PHASE, Phase::Solid) } {
        for (dx, dy) in NEIGHBORS {
            window.note_structural(pos.translated(dx, dy));
        }
    }
    let out = match burning.residue {
        Some((threshold, id)) if rng.draw().below(threshold) => id,
        _ => burning.burnout,
    };
    set_product(window, pos, out, rng);
}

fn emit_into_air(window: &mut SimWindow, pos: CellPos, material: MaterialId, rng: &mut Rng) {
    let (dx, dy) = rng.draw().choose(&NEIGHBORS);
    let target = pos.translated(dx, dy);
    if window
        .get(target)
        .is_some_and(|neighbor| neighbor.material == MaterialId::AIR)
    {
        set_product(window, target, material, rng);
    }
}

fn adjacent_water(window: &SimWindow, pos: CellPos) -> Option<CellPos> {
    NEIGHBORS.iter().find_map(|&(dx, dy)| {
        let neighbor_pos = pos.translated(dx, dy);
        window
            .get(neighbor_pos)
            .is_some_and(|neighbor| neighbor.material == material::WATER)
            .then_some(neighbor_pos)
    })
}

fn oxygen_exposed(window: &SimWindow, pos: CellPos) -> bool {
    NEIGHBORS.iter().any(|&(dx, dy)| {
        window.get(pos.translated(dx, dy)).is_some_and(|neighbor| {
            matches!(content::phase(neighbor.material), Phase::Empty | Phase::Gas)
        })
    })
}

fn set_product(window: &mut SimWindow, pos: CellPos, material: MaterialId, rng: &mut Rng) {
    window.set(pos, Cell::new(material, rng.draw().bits(4) as u8));
}

fn note_undermined(window: &mut SimWindow, vacated: CellPos) {
    let above = vacated.translated(0, 1);
    let rigid = window.get(above).is_some_and(|cell| {
        content::phase(cell.material) == Phase::Solid && content::is_rigid_capable(cell.material)
    });
    if rigid {
        window.note_structural(above);
    }
}

fn note_body_below(window: &mut SimWindow, pos: CellPos) {
    let below = pos.translated(0, -1);
    if window.get(below).is_some_and(|cell| cell.is_body()) {
        window.note_structural(below);
    }
}

fn ambient_density_milli(window: &SimWindow, pos: CellPos) -> i32 {
    if let Some(below) = window.get(pos.translated(0, -1))
        && matches!(content::phase(below.material), Phase::Liquid | Phase::Gas)
    {
        return content::density_milli(below.material);
    }
    const { content::density_milli(MaterialId::AIR) }
}

fn buoyant_gravity<M: MatSpec>(ambient: i32) -> i32 {
    let density = const { M::DENSITY_MILLI } as i64;
    let submerged = (density - ambient as i64).clamp(0, density);
    ((GRAVITY_DV as i64 * submerged + density / 2) / density) as i32
}

fn buoyant_rise<M: MatSpec>(window: &SimWindow, pos: CellPos) -> i32 {
    let Some(above) = window.get(pos.translated(0, 1)) else {
        return 0;
    };
    if content::phase(above.material) != Phase::Liquid {
        return 0;
    }
    let ambient = content::density_milli(above.material) as i64;
    let density = (const { M::DENSITY_MILLI } as i64).max(1);
    if ambient <= density {
        return 0;
    }
    (GRAVITY_DV as i64 * (ambient - density) / density).min(4 * GRAVITY_DV as i64) as i32
}

fn neighbor_mean_vel(window: &SimWindow, pos: CellPos, phase: Phase) -> Option<(i32, i32)> {
    let mut sum_x = 0;
    let mut sum_y = 0;
    let mut count = 0;
    for (dx, dy) in NEIGHBORS {
        if let Some(cell) = window.get(pos.translated(dx, dy))
            && content::phase(cell.material) == phase
        {
            sum_x += cell.vx as i32;
            sum_y += cell.vy as i32;
            count += 1;
        }
    }
    (count > 0).then(|| (sum_x / count, sum_y / count))
}

fn cohesion(
    window: &SimWindow,
    pos: CellPos,
    vx: &mut i32,
    vy: &mut i32,
    factor: VelocityFactor,
    phase: Phase,
) {
    if !factor.is_zero()
        && let Some((mean_x, mean_y)) = neighbor_mean_vel(window, pos, phase)
    {
        *vx += factor.apply(mean_x - *vx);
        *vy += factor.apply(mean_y - *vy);
    }
}

fn slope_accelerate(window: &SimWindow, pos: CellPos, cell: Cell, vx: &mut i32) {
    if !window
        .get(pos.translated(0, 1))
        .is_some_and(|top| top.is_air())
    {
        return;
    }
    let same = |dx: i32, dy: i32| {
        window
            .get(pos.translated(dx, dy))
            .is_some_and(|neighbor| neighbor.material == cell.material)
    };
    for side in [-1, 1] {
        let higher_uphill = same(-side, 0) && same(-side, 1);
        let open_downhill = window
            .get(pos.translated(side, 0))
            .is_some_and(|beside| beside.is_air());
        if higher_uphill && open_downhill {
            *vx += side * SLOPE_DV;
        }
    }
}

fn current_accelerate(window: &SimWindow, pos: CellPos, cell: Cell, vx: &mut i32) {
    let head = |dx: i32| {
        window
            .get(pos.translated(dx, 0))
            .filter(|neighbor| neighbor.material == cell.material)
            .map(|neighbor| down_head(neighbor.aux) as i32)
    };
    let (Some(left), Some(right)) = (head(-1), head(1)) else {
        return;
    };
    let tilt = left - right;
    if tilt.abs() >= 3 {
        *vx += tilt.signum() * SLOPE_DV;
    }
}

const fn down_head(aux: u8) -> u8 {
    aux & 0x0F
}

const fn up_head(aux: u8) -> u8 {
    aux >> 4
}

fn aux_relax(window: &mut SimWindow, pos: CellPos, cell: Cell) -> (Cell, bool) {
    let same = |dx: i32, dy: i32| {
        window
            .get(pos.translated(dx, dy))
            .filter(|neighbor| neighbor.material == cell.material)
    };
    let (above, below) = (same(0, 1), same(0, -1));
    let (left, right) = (same(-1, 0), same(1, 0));
    let down = if let Some(top) = above {
        (down_head(top.aux) + 1).min(0x0F)
    } else if window
        .get(pos.translated(0, 1))
        .is_some_and(|top| top.is_air())
    {
        0
    } else {
        left.map_or(0, |n| down_head(n.aux).saturating_sub(1))
            .max(right.map_or(0, |n| down_head(n.aux).saturating_sub(1)))
    };
    let up = below
        .map_or(0, |n| {
            down_head(n.aux)
                .saturating_sub(down + 1)
                .max(up_head(n.aux))
        })
        .max(left.map_or(0, |n| {
            down_head(n.aux)
                .saturating_sub(down + 1)
                .max(up_head(n.aux).saturating_sub(1))
        }))
        .max(right.map_or(0, |n| {
            down_head(n.aux)
                .saturating_sub(down + 1)
                .max(up_head(n.aux).saturating_sub(1))
        }))
        .min(0x0F);
    let target = down | (up << 4);
    if target == cell.aux {
        return (cell, true);
    }
    let mut relaxed = cell;
    relaxed.aux = target;
    window.set(pos, relaxed);
    (relaxed, false)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Entry {
    Open,
    Busy,
    Blocked,
}

fn admits(mover: MaterialId, dy: i32, target: Cell) -> bool {
    if !matches!(
        content::phase(target.material),
        Phase::Empty | Phase::Liquid | Phase::Gas
    ) {
        return false;
    }
    let pushing = content::density_milli(mover);
    let pushed = content::density_milli(target.material);
    pushed < pushing || (dy > 0 && pushed > pushing) || (dy == 0 && target.is_air())
}

fn can_enter(window: &SimWindow, mover: MaterialId, dir: (i32, i32), target: CellPos) -> bool {
    window
        .get(target)
        .is_some_and(|cell| admits(mover, dir.1, cell))
}

fn entry(window: &SimWindow, mover: MaterialId, dir: (i32, i32), target: CellPos) -> Entry {
    let Some(cell) = window.get(target) else {
        return Entry::Blocked;
    };
    if !admits(mover, dir.1, cell) {
        return Entry::Blocked;
    }
    if !cell.is_air() && cell.flags & Cell::MOVED != 0 {
        return Entry::Busy;
    }
    Entry::Open
}

fn supported(window: &SimWindow, cell: Cell, pos: CellPos) -> bool {
    !can_enter(window, cell.material, (0, -1), pos.translated(0, -1))
}

fn submerged(window: &SimWindow, pos: CellPos) -> bool {
    window
        .get(pos.translated(0, 1))
        .is_some_and(|above| content::phase(above.material) == Phase::Liquid)
}

fn neighbor_agitated(window: &SimWindow, pos: CellPos) -> bool {
    NEIGHBORS.iter().any(|&(dx, dy)| {
        window.get(pos.translated(dx, dy)).is_some_and(|cell| {
            matches!(content::phase(cell.material), Phase::Powder | Phase::Liquid)
                && ((cell.vx as i32).abs() >= AGITATED || (cell.vy as i32).abs() >= AGITATED)
        })
    })
}

fn prefer_side(vx: i32, rng: &mut Rng) -> i32 {
    match vx.cmp(&0) {
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => {
            if rng.draw().bit() {
                1
            } else {
                -1
            }
        }
    }
}

fn step_cells(v: i32, rng: &mut Rng) -> i32 {
    let mag = v.abs();
    let fractional = (rng.draw().bits(SUBCELL_BITS) as i32) < mag % SUBCELL_UNITS_PER_CELL;
    let cells = (mag / SUBCELL_UNITS_PER_CELL + fractional as i32).min(MAX_STEP);
    cells * v.signum()
}

fn scaled_round(product: i64, shift: u32) -> i32 {
    let half = 1i64 << (shift - 1);
    let magnitude = (product.abs() + half) >> shift;
    (if product < 0 { -magnitude } else { magnitude }) as i32
}

fn reflect(v: i32, restitution: VelocityFactor) -> i32 {
    -restitution.apply(v)
}
