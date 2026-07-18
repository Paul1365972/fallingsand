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
const IMPACT: i32 = SUBCELL_UNITS_PER_CELL;

const DEPTH_MAX: i32 = 7;
const EXCESS_MAX: i32 = 3;
const ARTESIAN_MIN: i32 = 2;
const BREACH_MIN: i32 = 3;
const JET_HEAD_MAX: i32 = 8;
const HOP_THRESHOLD: u64 = u64::MAX / 4;
const JET_THRESHOLD: u64 = u64::MAX / 8;

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
        Dynamics::Liquid(d) => liquid_effects::<M>(window, pos, cell, d, tick, &mut rng),
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
    tick: u64,
    rng: &mut Rng,
) {
    let (cell, converged) = relax_head(window, pos, cell, tick);
    let (mut vx, mut vy) = cell.vel();
    let grounded = supported(window, cell, pos);
    let mut rising = 0;
    if grounded {
        vx = d.ground_friction_keep.apply(vx);
        rising = buoyant_rise::<M>(window, pos);
        vy += rising;
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
    if grounded && converged && neighborhood_still(window, pos, cell) {
        pressure_jets(window, pos, cell, &mut vx, &mut vy, rng);
    }
    if vy <= -IMPACT
        && window
            .get(pos.translated(0, -1))
            .is_some_and(|below| content::phase(below.material) == Phase::Liquid)
    {
        vx += prefer_side(vx, rng) * d.deflect_keep.apply(vy.abs() / 2);
    }
    finish_effects(window, pos, cell, vx, vy, grounded && rising == 0);
}

const fn depth_of(aux: u8) -> i32 {
    (aux & 0x07) as i32
}

const fn rightward_of(aux: u8) -> i32 {
    ((aux >> 3) & 0x03) as i32
}

const fn leftward_of(aux: u8) -> i32 {
    ((aux >> 5) & 0x03) as i32
}

const fn excess_of(aux: u8) -> i32 {
    let (rightward, leftward) = (rightward_of(aux), leftward_of(aux));
    if rightward > leftward {
        rightward
    } else {
        leftward
    }
}

const fn head_of(aux: u8) -> i32 {
    depth_of(aux) + excess_of(aux)
}

fn relax_head(window: &mut SimWindow, pos: CellPos, cell: Cell, tick: u64) -> (Cell, bool) {
    let same = |dx: i32, dy: i32| {
        window
            .get(pos.translated(dx, dy))
            .filter(|neighbor| neighbor.material == cell.material)
    };
    let (above, below) = (same(0, 1), same(0, -1));
    let (left, right) = (same(-1, 0), same(1, 0));
    let depth = if let Some(top) = above {
        (depth_of(top.aux) + 1).min(DEPTH_MAX)
    } else if opens(window, pos.translated(0, 1)) {
        0
    } else {
        let covered = |n: Option<Cell>| n.map_or(0, |n| depth_of(n.aux) - 1);
        covered(left).max(covered(right)).max(0)
    };
    let carried = |n: Option<Cell>, flow: fn(u8) -> i32, dv: i32| {
        n.map_or(0, |n| depth_of(n.aux) + flow(n.aux) - dv)
    };
    let rightward_head =
        depth
            .max(carried(left, rightward_of, 0))
            .max(carried(below, rightward_of, 1));
    let leftward_head =
        depth
            .max(carried(right, leftward_of, 0))
            .max(carried(below, leftward_of, 1));
    let rightward_new = (rightward_head - depth).clamp(0, EXCESS_MAX);
    let leftward_new = (leftward_head - depth).clamp(0, EXCESS_MAX);
    let (rightward, leftward) = if crate::kernel::row_reverse(tick, pos.y) {
        (rightward_of(cell.aux), leftward_new)
    } else {
        (rightward_new, leftward_of(cell.aux))
    };
    let pending = rightward != rightward_new || leftward != leftward_new;
    let target = depth as u8 | (rightward as u8) << 3 | (leftward as u8) << 5;
    if target == cell.aux {
        if pending {
            window.mark(pos);
        }
        return (cell, !pending);
    }
    let mut relaxed = cell;
    relaxed.aux = target;
    window.set(pos, relaxed);
    (relaxed, false)
}

fn still(cell: Cell) -> bool {
    (cell.vx as i32).abs() < SETTLE && (cell.vy as i32).abs() < SETTLE
}

fn neighborhood_still(window: &SimWindow, pos: CellPos, cell: Cell) -> bool {
    still(cell)
        && NEIGHBORS
            .iter()
            .all(|&(dx, dy)| window.get(pos.translated(dx, dy)).is_none_or(still))
}

fn pressure_jets(
    window: &SimWindow,
    pos: CellPos,
    cell: Cell,
    vx: &mut i32,
    vy: &mut i32,
    rng: &mut Rng,
) {
    let excess = excess_of(cell.aux);
    let head = head_of(cell.aux);
    if opens(window, pos.translated(0, 1)) {
        if excess >= ARTESIAN_MIN && rng.draw().below(JET_THRESHOLD) {
            *vy += GRAVITY_DV * excess;
        }
    } else if head >= BREACH_MIN {
        for side in [-1, 1] {
            if opens(window, pos.translated(side, 0)) {
                *vx += side * GRAVITY_DV * head.min(JET_HEAD_MAX);
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
    let phase = content::phase(cell.material);
    if matches!(phase, Phase::Empty | Phase::Solid) {
        return;
    }
    let (mut vx, mut vy) = cell.vel();
    let material = cell.material;
    let mut rng = Hash::seed(tick).salt(MOVEMENT_SALT).pos(pos.x, pos.y).rng();
    if vx == 0 && vy == 0 {
        match phase {
            Phase::Liquid => flow_liquid(window, pos, cell, &mut rng),
            Phase::Gas => flow_gas(window, pos, material, &mut rng),
            _ => {}
        }
        return;
    }

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
    let gdir = if phase == Phase::Gas { 1 } else { -1 };
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

fn flow_liquid(window: &mut SimWindow, pos: CellPos, cell: Cell, rng: &mut Rng) {
    let material = cell.material;
    match entry(window, material, (0, -1), pos.translated(0, -1)) {
        Entry::Open => {
            window.swap(pos, pos.translated(0, -1));
            return;
        }
        Entry::Busy => {
            window.mark(pos);
            return;
        }
        Entry::Blocked => {}
    }
    let prefer = prefer_side(0, rng);
    let mut target = None;
    let mut gate = u64::MAX;
    for side in [prefer, -prefer] {
        let over = pos.translated(side, 0);
        match entry(window, material, (side, 0), over) {
            Entry::Open => {}
            Entry::Busy => {
                window.mark(pos);
                continue;
            }
            Entry::Blocked => continue,
        }
        let diag = pos.translated(side, -1);
        match entry(window, material, (side, -1), diag) {
            Entry::Open => {
                target = Some(diag);
                gate = u64::MAX;
                break;
            }
            Entry::Busy => window.mark(pos),
            Entry::Blocked => {
                if target.is_none()
                    && let Some(support) = window.get(diag)
                {
                    if support.material == material {
                        if excess_of(support.aux) >= ARTESIAN_MIN
                            && excess_of(support.aux) > excess_of(cell.aux)
                            && support.flags & Cell::MOVED == 0
                            && still(support)
                        {
                            target = Some(over);
                            gate = HOP_THRESHOLD;
                        }
                    } else if has_liquid_neighbor(window, pos, material) {
                        target = Some(over);
                    }
                }
            }
        }
    }
    let Some(target) = target else {
        return;
    };
    if rng
        .draw()
        .below(content::flow_threshold(material).min(gate))
    {
        window.swap(pos, target);
    } else {
        window.mark(pos);
    }
}

fn has_liquid_neighbor(window: &SimWindow, pos: CellPos, material: MaterialId) -> bool {
    NEIGHBORS.iter().any(|&(dx, dy)| {
        window
            .get(pos.translated(dx, dy))
            .is_some_and(|neighbor| neighbor.material == material)
    })
}

fn flow_gas(window: &mut SimWindow, pos: CellPos, material: MaterialId, rng: &mut Rng) {
    match entry(window, material, (0, 1), pos.translated(0, 1)) {
        Entry::Open => {
            window.swap(pos, pos.translated(0, 1));
            return;
        }
        Entry::Busy => {
            window.mark(pos);
            return;
        }
        Entry::Blocked => {}
    }
    let prefer = prefer_side(0, rng);
    for side in [prefer, -prefer] {
        match entry(window, material, (side, 0), pos.translated(side, 0)) {
            Entry::Open => {}
            Entry::Busy => {
                window.mark(pos);
                continue;
            }
            Entry::Blocked => continue,
        }
        let target = match entry(window, material, (side, 1), pos.translated(side, 1)) {
            Entry::Open => pos.translated(side, 1),
            Entry::Busy => {
                window.mark(pos);
                continue;
            }
            Entry::Blocked => pos.translated(side, 0),
        };
        if rng.draw().below(content::flow_threshold(material)) {
            window.swap(pos, target);
        } else {
            window.mark(pos);
        }
        return;
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

fn opens(window: &SimWindow, pos: CellPos) -> bool {
    window
        .get(pos)
        .is_some_and(|cell| matches!(content::phase(cell.material), Phase::Empty | Phase::Gas))
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
