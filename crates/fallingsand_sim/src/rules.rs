use crate::window::SimWindow;
use fallingsand_core::content::{self, MatSpec, material};
use fallingsand_core::{
    Burning, BurningKind, CARDINAL_NEIGHBORS as NEIGHBORS, Cell, CellPos, Dynamics, GRID_GRAVITY,
    GasDynamics, LiquidDynamics, MaterialId, Phase, PowderDynamics, SealedBurn, TICK_DT, VEL_ONE,
};
use fallingsand_rng::{Hash, Rng};

const VEL_MAX: i32 = 31 * VEL_ONE;
const MAX_STEP: i32 = 31;
const VEL_BITS: u32 = VEL_ONE.trailing_zeros();
const _: () = assert!(1 << VEL_BITS == VEL_ONE);
const SETTLE: i32 = (7.5 * TICK_DT * VEL_ONE as f32) as i32;
const SUBMERGED_DENSITY_MILLI: i32 = 100_000;
const GRAVITY_DV: i32 = (GRID_GRAVITY * TICK_DT * TICK_DT * VEL_ONE as f32 + 0.5) as i32;
const AGITATED: i32 = 2 * GRAVITY_DV;

macro_rules! material_dispatch {
    ($(($idx:literal, $name:ident, $spec:ident)),* $(,)?) => {
        pub(crate) fn update_cell(
            window: &mut SimWindow,
            pos: CellPos,
            tick: u64,
            tick_byte: u8,
        ) {
            let Some(cell) = window.get(pos) else {
                return;
            };
            if cell.updated == tick_byte {
                window.mark(pos);
                return;
            }
            match cell.material.0 {
                $( $idx => update_cell_spec::<content::specs::$spec>(
                    window, pos, cell, tick, tick_byte,
                ), )*
                _ => unreachable!("invalid material id"),
            }
        }

        pub(crate) fn random_tick(
            window: &mut SimWindow,
            pos: CellPos,
            tick: u64,
            tick_byte: u8,
        ) {
            let Some(cell) = window.get(pos) else {
                return;
            };
            match cell.material.0 {
                $( $idx => random_tick_spec::<content::specs::$spec>(
                    window, pos, tick, tick_byte,
                ), )*
                _ => unreachable!("invalid material id"),
            }
        }
    };
}
fallingsand_core::for_each_material!(material_dispatch);

fn update_cell_spec<M: MatSpec>(
    window: &mut SimWindow,
    pos: CellPos,
    cell: Cell,
    tick: u64,
    tick_byte: u8,
) {
    let mut rng = Hash::seed(tick).pos(pos.x, pos.y).rng();

    if const { M::IS_HOT } {
        ignite_neighbors(window, pos, &mut rng, tick_byte);
    }
    if let Some(burning) = const { M::BURNING }
        && burning_step::<M>(window, pos, burning, &mut rng, tick_byte)
    {
        return;
    }
    if const { M::IS_REACTIVE } && react::<M>(window, pos, cell, &mut rng, tick_byte) {
        return;
    }
    match const { M::DYNAMICS } {
        Dynamics::None => {}
        Dynamics::Powder(d) => update_powder::<M>(window, pos, cell, d, &mut rng, tick_byte),
        Dynamics::Liquid(d) => update_liquid::<M>(window, pos, cell, d, &mut rng, tick_byte),
        Dynamics::Gas(d) => update_gas::<M>(window, pos, cell, d, &mut rng, tick_byte),
    }
}

fn random_tick_spec<M: MatSpec>(
    _window: &mut SimWindow,
    _pos: CellPos,
    _tick: u64,
    _tick_byte: u8,
) {
}

fn react<M: MatSpec>(
    window: &mut SimWindow,
    pos: CellPos,
    cell: Cell,
    rng: &mut Rng,
    tick_byte: u8,
) -> bool {
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
                set_product(window, pos, reaction.becomes, rng, tick_byte);
                set_product(window, neighbor_pos, reaction.other_becomes, rng, tick_byte);
                return true;
            }
        }
    }
    if let Some((threshold, product)) = const { M::DECAY } {
        if rng.draw().below(threshold) {
            set_product(window, pos, product, rng, tick_byte);
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

fn ignite_neighbors(window: &mut SimWindow, pos: CellPos, rng: &mut Rng, tick_byte: u8) {
    for (dx, dy) in NEIGHBORS {
        let neighbor_pos = pos.translated(dx, dy);
        let Some(neighbor) = window.get(neighbor_pos) else {
            continue;
        };
        if neighbor.updated == tick_byte {
            continue;
        }
        let Some(ignition) = content::ignition(neighbor.material) else {
            continue;
        };
        let threshold = if oxygen_exposed(window, neighbor_pos) {
            ignition.open
        } else {
            ignition.sealed
        };
        if threshold == 0 {
            continue;
        }
        window.mark(pos);
        if rng.draw().below(threshold) {
            let mut lit = neighbor;
            lit.material = ignition.into;
            lit.set_body(false);
            lit.updated = tick_byte;
            window.set(neighbor_pos, lit);
        }
    }
}

fn burning_step<M: MatSpec>(
    window: &mut SimWindow,
    pos: CellPos,
    burning: Burning,
    rng: &mut Rng,
    tick_byte: u8,
) -> bool {
    if let Some(water) = adjacent_water(window, pos) {
        if burning.kind == BurningKind::Flame {
            set_product(window, pos, material::STEAM, rng, tick_byte);
        } else {
            burn_out::<M>(window, pos, burning, rng, tick_byte);
            set_product(window, water, material::STEAM, rng, tick_byte);
        }
        return true;
    }
    if rng.draw().below(burning.emit) {
        emit_into_air(window, pos, material::FIRE, rng, tick_byte);
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
                cell.updated = tick_byte;
                window.set(pos, cell);
                return true;
            }
            SealedBurn::Smoulder(threshold) => threshold,
        }
    };
    if rng.draw().below(burn) {
        burn_out::<M>(window, pos, burning, rng, tick_byte);
        return true;
    }
    window.mark(pos);
    false
}

fn burn_out<M: MatSpec>(
    window: &mut SimWindow,
    pos: CellPos,
    burning: Burning,
    rng: &mut Rng,
    tick_byte: u8,
) {
    if const { matches!(M::PHASE, Phase::Solid) } {
        for (dx, dy) in NEIGHBORS {
            window.note_structural(pos.translated(dx, dy));
        }
    }
    let out = match burning.residue {
        Some((threshold, id)) if rng.draw().below(threshold) => id,
        _ => burning.burnout,
    };
    set_product(window, pos, out, rng, tick_byte);
}

fn emit_into_air(
    window: &mut SimWindow,
    pos: CellPos,
    material: MaterialId,
    rng: &mut Rng,
    tick_byte: u8,
) {
    let (dx, dy) = NEIGHBORS[rng.draw().bits(2) as usize];
    let target = pos.translated(dx, dy);
    if window
        .get(target)
        .is_some_and(|neighbor| neighbor.material == MaterialId::AIR)
    {
        set_product(window, target, material, rng, tick_byte);
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

fn set_product(
    window: &mut SimWindow,
    pos: CellPos,
    material: MaterialId,
    rng: &mut Rng,
    tick_byte: u8,
) {
    let mut cell = Cell::new(material, rng.draw().bits(4) as u8);
    cell.updated = tick_byte;
    window.set(pos, cell);
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

fn ambient_density_milli(window: &SimWindow, pos: CellPos) -> i32 {
    if let Some(below) = window.get(pos.translated(0, -1))
        && matches!(content::phase(below.material), Phase::Liquid | Phase::Gas)
    {
        return content::density_milli(below.material);
    }
    const { content::density_milli(MaterialId::AIR) }
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

fn can_enter<M: MatSpec>(window: &SimWindow, dir: (i32, i32), target: CellPos) -> bool {
    let Some(cell) = window.get(target) else {
        return false;
    };
    if !matches!(
        content::phase(cell.material),
        Phase::Empty | Phase::Liquid | Phase::Gas
    ) {
        return false;
    }
    let density = const { M::DENSITY_MILLI };
    let target_density = content::density_milli(cell.material);
    match dir.1 {
        dy if dy < 0 => density > target_density,
        dy if dy > 0 => density < target_density,
        _ => density > target_density || cell.is_air(),
    }
}

fn step_cells(v: i32, rng: &mut Rng) -> i32 {
    let mag = v.abs();
    let fractional = (rng.draw().bits(VEL_BITS) as i32) < mag % VEL_ONE;
    let cells = (mag / VEL_ONE + fractional as i32).min(MAX_STEP);
    cells * v.signum()
}

fn mul_q16(v: i32, keep_q16: u32) -> i32 {
    scaled_round(v as i64 * keep_q16 as i64, 16)
}

fn scaled_round(product: i64, shift: u32) -> i32 {
    let half = 1i64 << (shift - 1);
    let magnitude = (product.abs() + half) >> shift;
    (if product < 0 { -magnitude } else { magnitude }) as i32
}

fn reflect(v: i32, restitution_q16: u32) -> i32 {
    -mul_q16(v, restitution_q16)
}

fn update_powder<M: MatSpec>(
    window: &mut SimWindow,
    pos: CellPos,
    cell: Cell,
    d: PowderDynamics,
    rng: &mut Rng,
    tick_byte: u8,
) {
    update_mover::<M>(
        window,
        pos,
        cell,
        d.restitution_q16,
        -1,
        rng,
        tick_byte,
        |window, pos, vx, vy, _rng| {
            settling_accelerate::<M>(
                window,
                pos,
                vx,
                vy,
                d.air_drag_keep_q16,
                d.submerged_drag_q16,
                d.ground_friction_keep_q16,
                d.cohesion_q16,
            )
        },
        |window, cur, vx, vy, rng| topple_slide::<M>(window, cur, vx, vy, d, rng),
    );
}

fn update_liquid<M: MatSpec>(
    window: &mut SimWindow,
    pos: CellPos,
    cell: Cell,
    d: LiquidDynamics,
    rng: &mut Rng,
    tick_byte: u8,
) {
    let above = pos.translated(0, 1);
    if let Some(top) = window.get(above)
        && content::phase(top.material) == Phase::Liquid
        && content::density_milli(top.material) > const { M::DENSITY_MILLI }
    {
        window.swap(pos, above);
        return;
    }

    update_mover::<M>(
        window,
        pos,
        cell,
        d.restitution_q16,
        -1,
        rng,
        tick_byte,
        |window, pos, vx, vy, _rng| {
            settling_accelerate::<M>(
                window,
                pos,
                vx,
                vy,
                d.air_drag_keep_q16,
                d.submerged_drag_q16,
                d.ground_friction_keep_q16,
                d.cohesion_q16,
            )
        },
        |window, cur, vx, vy, rng| ledge_flow::<M>(window, cur, vx, vy, d, rng),
    );
}

fn update_gas<M: MatSpec>(
    window: &mut SimWindow,
    pos: CellPos,
    cell: Cell,
    d: GasDynamics,
    rng: &mut Rng,
    tick_byte: u8,
) {
    update_mover::<M>(
        window,
        pos,
        cell,
        d.restitution_q16,
        1,
        rng,
        tick_byte,
        |window, pos, vx, vy, rng| {
            *vy += GRAVITY_DV;
            *vx = mul_q16(*vx, d.air_drag_keep_q16);
            *vy = mul_q16(*vy, d.air_drag_keep_q16);
            if d.turbulence_q16 > 0 {
                let r = rng.draw().bits(16) as i64 - 32768;
                *vx += scaled_round(d.turbulence_q16 as i64 * r, 31);
            }
            note_body_below(window, pos);
            cohesion::<M>(window, pos, vx, vy, d.cohesion_q16);
        },
        |window, cur, vx, vy, rng| ceiling_spread::<M>(window, cur, vx, vy, d, rng),
    );
}

#[allow(clippy::too_many_arguments)]
fn settling_accelerate<M: MatSpec>(
    window: &mut SimWindow,
    pos: CellPos,
    vx: &mut i32,
    vy: &mut i32,
    air_drag_keep_q16: u32,
    submerged_drag_q16: u32,
    ground_friction_keep_q16: u32,
    cohesion_q16: u32,
) {
    let ambient = ambient_density_milli(window, pos);
    *vy -= buoyant_gravity::<M>(ambient);
    apply_drag(vx, vy, ambient, air_drag_keep_q16, submerged_drag_q16);

    if supported_below::<M>(window, pos) {
        *vx = mul_q16(*vx, ground_friction_keep_q16);
    }
    note_body_below(window, pos);
    cohesion::<M>(window, pos, vx, vy, cohesion_q16);
}

#[allow(clippy::too_many_arguments)]
fn update_mover<M: MatSpec>(
    window: &mut SimWindow,
    pos: CellPos,
    cell: Cell,
    restitution_q16: u32,
    gdir: i32,
    rng: &mut Rng,
    tick_byte: u8,
    accelerate: impl FnOnce(&mut SimWindow, CellPos, &mut i32, &mut i32, &mut Rng),
    redirect: impl FnOnce(&mut SimWindow, &mut CellPos, &mut i32, i32, &mut Rng) -> bool,
) {
    let (mut vx, mut vy) = cell.vel();

    accelerate(window, pos, &mut vx, &mut vy, rng);

    let (mut cur, mut moved) = traverse::<M>(window, pos, &mut vx, &mut vy, restitution_q16, rng);
    if !can_enter::<M>(window, (0, gdir), cur.translated(0, gdir)) {
        moved |= redirect(window, &mut cur, &mut vx, vy, rng);
    }
    if moved {
        note_undermined(window, pos);
    }
    finish::<M>(window, cur, vx, vy, restitution_q16, gdir, tick_byte);
}

fn buoyant_gravity<M: MatSpec>(ambient: i32) -> i32 {
    let density = const { M::DENSITY_MILLI } as i64;
    let submerged = (density - ambient as i64).clamp(0, density);
    ((GRAVITY_DV as i64 * submerged + density / 2) / density) as i32
}

fn apply_drag(vx: &mut i32, vy: &mut i32, ambient: i32, keep_q16: u32, keep_submerged_q16: u32) {
    let keep = if ambient > SUBMERGED_DENSITY_MILLI {
        keep_submerged_q16
    } else {
        keep_q16
    };
    *vx = mul_q16(*vx, keep);
    *vy = mul_q16(*vy, keep);
}

fn supported_below<M: MatSpec>(window: &SimWindow, pos: CellPos) -> bool {
    !can_enter::<M>(window, (0, -1), pos.translated(0, -1))
}

fn note_body_below(window: &mut SimWindow, pos: CellPos) {
    let below = pos.translated(0, -1);
    if window.get(below).is_some_and(|cell| cell.is_body()) {
        window.note_structural(below);
    }
}

fn cohesion<M: MatSpec>(
    window: &SimWindow,
    pos: CellPos,
    vx: &mut i32,
    vy: &mut i32,
    cohesion_q16: u32,
) {
    if cohesion_q16 > 0
        && let Some((mean_x, mean_y)) = neighbor_mean_vel(window, pos, const { M::PHASE })
    {
        *vx += mul_q16(mean_x - *vx, cohesion_q16);
        *vy += mul_q16(mean_y - *vy, cohesion_q16);
    }
}

fn traverse<M: MatSpec>(
    window: &mut SimWindow,
    pos: CellPos,
    vx: &mut i32,
    vy: &mut i32,
    restitution_q16: u32,
    rng: &mut Rng,
) -> (CellPos, bool) {
    *vx = (*vx).clamp(-VEL_MAX, VEL_MAX);
    *vy = (*vy).clamp(-VEL_MAX, VEL_MAX);

    let tx = step_cells(*vx, rng);
    let ty = step_cells(*vy, rng);
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
        if step_x {
            let next = cur.translated(sx, 0);
            if can_enter::<M>(window, (sx, 0), next) {
                window.swap(cur, next);
                cur = next;
                moved = true;
                done_x += 1;
            } else {
                *vx = reflect(*vx, restitution_q16);
                done_x = ix;
            }
        } else {
            let next = cur.translated(0, sy);
            if can_enter::<M>(window, (0, sy), next) {
                window.swap(cur, next);
                cur = next;
                moved = true;
                done_y += 1;
            } else {
                done_y = iy;
            }
        }
    }
    (cur, moved)
}

fn finish<M: MatSpec>(
    window: &mut SimWindow,
    cur: CellPos,
    mut vx: i32,
    mut vy: i32,
    restitution_q16: u32,
    gdir: i32,
    tick_byte: u8,
) {
    for (dx, dy) in NEIGHBORS {
        let into = if dx != 0 { vx * dx > 0 } else { vy * dy > 0 };
        let target = cur.translated(dx, dy);
        if into && !can_enter::<M>(window, (dx, dy), target) {
            if dx != 0 {
                vx = reflect(vx, restitution_q16);
            } else {
                vy = reflect(vy, restitution_q16);
            }
        }
    }
    let settled = !can_enter::<M>(window, (0, gdir), cur.translated(0, gdir));
    if settled {
        if vx.abs() < SETTLE {
            vx = 0;
        }
        if vy.abs() < SETTLE {
            vy = 0;
        }
    }
    vx = vx.clamp(-VEL_MAX, VEL_MAX);
    vy = vy.clamp(-VEL_MAX, VEL_MAX);

    let Some(current) = window.get(cur) else {
        return;
    };
    if current.vx as i32 != vx || current.vy as i32 != vy {
        let mut written = current;
        written.set_vel(vx, vy);
        written.updated = tick_byte;
        window.set(cur, written);
    } else if vx != 0 || vy != 0 {
        window.mark(cur);
        if vx.abs() >= AGITATED || vy.abs() >= AGITATED {
            for (dx, dy) in NEIGHBORS {
                window.mark(cur.translated(dx, dy));
            }
        }
    }
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

fn topple_slide<M: MatSpec>(
    window: &mut SimWindow,
    cur: &mut CellPos,
    vx: &mut i32,
    vy: i32,
    d: PowderDynamics,
    rng: &mut Rng,
) -> bool {
    let kinetic = vx.abs() >= AGITATED || vy.abs() >= AGITATED;
    let loaded = window
        .get(cur.translated(0, 1))
        .is_some_and(|cell| content::phase(cell.material) == Phase::Powder);
    let threshold = if kinetic {
        d.topple_keep_threshold
    } else if loaded || neighbor_agitated(window, *cur) {
        d.topple_start_threshold
    } else {
        return false;
    };
    let gain = mul_q16(vy.abs(), d.deflect_keep_q16);
    let prefer = prefer_side(*vx, rng);
    let mut pending = false;
    for side in [prefer, -prefer] {
        if !can_enter::<M>(window, (side, 0), cur.translated(side, 0)) {
            continue;
        }
        let diag = cur.translated(side, -1);
        if !can_enter::<M>(window, (side, -1), diag) {
            continue;
        }
        if rng.draw().below(threshold) {
            *vx += side * gain.max(AGITATED);
            window.swap(*cur, diag);
            *cur = diag;
            return true;
        }
        pending |= loaded;
    }
    if pending {
        window.mark(*cur);
    }
    false
}

fn neighbor_agitated(window: &SimWindow, pos: CellPos) -> bool {
    NEIGHBORS.iter().any(|&(dx, dy)| {
        window.get(pos.translated(dx, dy)).is_some_and(|cell| {
            matches!(content::phase(cell.material), Phase::Powder | Phase::Liquid)
                && ((cell.vx as i32).abs() >= AGITATED || (cell.vy as i32).abs() >= AGITATED)
        })
    })
}

fn ledge_flow<M: MatSpec>(
    window: &mut SimWindow,
    cur: &mut CellPos,
    vx: &mut i32,
    vy: i32,
    d: LiquidDynamics,
    rng: &mut Rng,
) -> bool {
    let gain = mul_q16(vy.abs(), d.deflect_keep_q16);
    let prefer = prefer_side(*vx, rng);
    let can_flow = rng.draw().below(d.flow_threshold);
    for side in [prefer, -prefer] {
        let beside = cur.translated(side, 0);
        if !can_enter::<M>(window, (side, 0), beside) {
            continue;
        }
        if !can_flow {
            window.mark(*cur);
            return false;
        }
        let diag = cur.translated(side, -1);
        if can_enter::<M>(window, (side, -1), diag) {
            *vx += side * gain;
            window.swap(*cur, diag);
            *cur = diag;
            return true;
        }
        window.swap(*cur, beside);
        *cur = beside;
        return true;
    }
    false
}

fn ceiling_spread<M: MatSpec>(
    window: &mut SimWindow,
    cur: &mut CellPos,
    vx: &mut i32,
    vy: i32,
    d: GasDynamics,
    rng: &mut Rng,
) -> bool {
    let gain = mul_q16(vy.abs(), d.deflect_keep_q16);
    let prefer = prefer_side(*vx, rng);
    for side in [prefer, -prefer] {
        let beside = cur.translated(side, 0);
        if !can_enter::<M>(window, (side, 0), beside) {
            continue;
        }
        let diag = cur.translated(side, 1);
        if can_enter::<M>(window, (side, 1), diag) {
            *vx += side * gain;
            window.swap(*cur, diag);
            *cur = diag;
            return true;
        }
        window.swap(*cur, beside);
        *cur = beside;
        return true;
    }
    false
}
