use crate::window::SimWindow;
use fallingsand_core::{
    Cell, CellPos, Dynamics, GRID_GRAVITY, MaterialId, MaterialRegistry, Phase, Product, TICK_DT,
    TICK_RATE, VEL_ONE, per_tick_chance,
};
use fallingsand_rng::{Hash, Rng};
use std::sync::LazyLock;

const NEIGHBORS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];
const FLICKER_RATE: f32 = 18.0;
static FLICKER_CHANCE: LazyLock<f32> = LazyLock::new(|| per_tick_chance(FLICKER_RATE));

const VEL_MAX: i32 = 2000 * VEL_ONE;
const MAX_STEP: i32 = 32;
const SETTLE: i32 = (7.5 * VEL_ONE as f32) as i32;
const SUBMERGED_DENSITY: f32 = 100.0;
const SUBMERGED_DRAG: f32 = 6.0;
static GRAVITY_DV: LazyLock<i32> =
    LazyLock::new(|| (GRID_GRAVITY * TICK_DT * VEL_ONE as f32).round() as i32);

pub(crate) fn update_cell(
    window: &mut SimWindow,
    registry: &MaterialRegistry,
    pos: CellPos,
    tick: u64,
    tick_byte: u8,
) {
    let Some(cell) = window.get(pos) else {
        return;
    };
    if cell.updated == tick_byte {
        return;
    }
    let mut rng = Hash::seed(tick).pos(pos.x, pos.y).rng();
    if registry.is_reactive(cell.material)
        && react(window, registry, pos, cell, &mut rng, tick_byte)
    {
        return;
    }
    let material = registry.get(cell.material);
    match material.phase {
        Phase::Empty | Phase::Solid => {}
        Phase::Powder | Phase::Liquid | Phase::Gas | Phase::Fire => {
            update_dynamic(window, registry, pos, cell, &mut rng, tick_byte)
        }
    }
}

fn react(
    window: &mut SimWindow,
    registry: &MaterialRegistry,
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
        if let Some(reaction) = registry.reaction(cell.material, neighbor.material) {
            keep = true;
            let factor = ignition_factor(window, registry, pos, cell.material, reaction.becomes)
                * ignition_factor(
                    window,
                    registry,
                    neighbor_pos,
                    neighbor.material,
                    reaction.other_becomes,
                );
            if factor > 0.0 && rng.draw().chance(reaction.chance * factor) {
                note_structural(window, registry, pos, cell.material);
                note_structural(window, registry, neighbor_pos, neighbor.material);
                let becomes = resolve_product(registry, reaction.becomes, cell.material, rng);
                let other_becomes =
                    resolve_product(registry, reaction.other_becomes, neighbor.material, rng);
                set_product(window, pos, becomes, rng, tick_byte);
                set_product(window, neighbor_pos, other_becomes, rng, tick_byte);
                return true;
            }
        }
    }
    if let Some((chance, product)) = registry.emits(cell.material) {
        keep = true;
        if rng.draw().chance(chance) {
            let (dx, dy) = NEIGHBORS[rng.draw().bits(2) as usize];
            let target = pos.translated(dx, dy);
            if window
                .get(target)
                .is_some_and(|neighbor| neighbor.material == MaterialId::AIR)
            {
                set_product(window, target, product, rng, tick_byte);
            }
        }
    }
    if let Some((chance, _)) = registry.decay(cell.material) {
        let material = registry.get(cell.material);
        if material.phase == Phase::Fire && sustained(window, registry, pos, cell.material) {
            if rng.draw().chance(*FLICKER_CHANCE) {
                let mut flicker = cell;
                flicker.set_shade(rng.draw().bits(4) as u8);
                flicker.updated = tick_byte;
                window.set(pos, flicker);
            } else {
                window.mark(pos);
            }
            return true;
        }
        if rng.draw().chance(chance) {
            let out = burnout_product(registry, cell.material, rng);
            set_product(window, pos, out, rng, tick_byte);
            return true;
        }
        keep = true;
    }
    if keep {
        window.mark(pos);
    }
    false
}

fn note_structural(
    window: &mut SimWindow,
    registry: &MaterialRegistry,
    pos: CellPos,
    material: MaterialId,
) {
    if registry.get(material).phase != Phase::Solid {
        return;
    }
    for (dx, dy) in NEIGHBORS {
        window.note_structural(pos.translated(dx, dy));
    }
}

fn ignition_factor(
    window: &SimWindow,
    registry: &MaterialRegistry,
    pos: CellPos,
    from: MaterialId,
    to: Product,
) -> f32 {
    let Product::Material(to) = to else {
        return 1.0;
    };
    if registry.is_ember(to) && !registry.is_ember(from) && !oxygen_exposed(window, registry, pos) {
        registry.smoulder(to)
    } else {
        1.0
    }
}

fn oxygen_exposed(window: &SimWindow, registry: &MaterialRegistry, pos: CellPos) -> bool {
    NEIGHBORS.iter().any(|&(dx, dy)| {
        window.get(pos.translated(dx, dy)).is_some_and(|neighbor| {
            matches!(
                registry.get(neighbor.material).phase,
                Phase::Empty | Phase::Gas | Phase::Fire
            )
        })
    })
}

fn sustained(
    window: &SimWindow,
    registry: &MaterialRegistry,
    pos: CellPos,
    material: MaterialId,
) -> bool {
    NEIGHBORS.iter().any(|&(dx, dy)| {
        window
            .get(pos.translated(dx, dy))
            .is_some_and(|neighbor| registry.sustains(material, neighbor.material))
    })
}

fn resolve_product(
    registry: &MaterialRegistry,
    product: Product,
    current: MaterialId,
    rng: &mut Rng,
) -> MaterialId {
    match product {
        Product::Material(id) => id,
        Product::Burnout => burnout_product(registry, current, rng),
    }
}

fn burnout_product(registry: &MaterialRegistry, material: MaterialId, rng: &mut Rng) -> MaterialId {
    let decayed = registry
        .decay(material)
        .map_or(material, |(_, decayed)| decayed);
    match registry.residue(material) {
        Some((chance, residue)) if rng.draw().chance(chance) => residue,
        _ => decayed,
    }
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

fn note_undermined(window: &mut SimWindow, registry: &MaterialRegistry, vacated: CellPos) {
    let above = vacated.translated(0, 1);
    let rigid = window.get(above).is_some_and(|cell| {
        let material = registry.get(cell.material);
        material.phase == Phase::Solid && material.rigid_capable
    });
    if rigid {
        window.note_structural(above);
    }
}

fn ambient_density(window: &SimWindow, registry: &MaterialRegistry, pos: CellPos) -> f32 {
    match window.get(pos.translated(0, -1)) {
        Some(below)
            if matches!(
                registry.get(below.material).phase,
                Phase::Liquid | Phase::Gas | Phase::Fire
            ) =>
        {
            registry.get(below.material).density
        }
        _ => registry.get(MaterialId::AIR).density,
    }
}

fn neighbor_mean_vel(
    window: &SimWindow,
    registry: &MaterialRegistry,
    pos: CellPos,
    phase: Phase,
) -> Option<(i32, i32)> {
    let mut sum_x = 0;
    let mut sum_y = 0;
    let mut count = 0;
    for (dx, dy) in NEIGHBORS {
        if let Some(cell) = window.get(pos.translated(dx, dy))
            && registry.get(cell.material).phase == phase
        {
            sum_x += cell.vx as i32;
            sum_y += cell.vy as i32;
            count += 1;
        }
    }
    (count > 0).then(|| (sum_x / count, sum_y / count))
}

fn can_enter(
    window: &SimWindow,
    registry: &MaterialRegistry,
    density: f32,
    dir: (i32, i32),
    target: CellPos,
) -> bool {
    let Some(cell) = window.get(target) else {
        return false;
    };
    let material = registry.get(cell.material);
    if !matches!(
        material.phase,
        Phase::Empty | Phase::Liquid | Phase::Gas | Phase::Fire
    ) {
        return false;
    }
    match dir.1 {
        dy if dy < 0 => density > material.density,
        dy if dy > 0 => density < material.density,
        _ => density > material.density || cell.is_air(),
    }
}

fn step_cells(v: i32, rng: &mut Rng) -> i32 {
    let denom = VEL_ONE * TICK_RATE as i32;
    let mag = v.abs();
    let cells =
        (mag / denom + rng.draw().chance((mag % denom) as f32 / denom as f32) as i32).min(MAX_STEP);
    cells * v.signum()
}

fn reflect(v: i32, restitution: f32) -> i32 {
    -(v as f32 * restitution).round() as i32
}

fn update_dynamic(
    window: &mut SimWindow,
    registry: &MaterialRegistry,
    pos: CellPos,
    cell: Cell,
    rng: &mut Rng,
    tick_byte: u8,
) {
    let phase = registry.get(cell.material).phase;
    let density = registry.get(cell.material).density;
    let dynamics = registry.dynamics(cell.material);
    let is_powder = phase == Phase::Powder;

    let (mut vx, mut vy) = cell.vel();

    if matches!(phase, Phase::Liquid) {
        let above = pos.translated(0, 1);
        if let Some(top) = window.get(above)
            && registry.get(top.material).phase == Phase::Liquid
            && registry.get(top.material).density > density
        {
            window.swap(pos, above);
            return;
        }
    }

    let sinks = matches!(phase, Phase::Powder | Phase::Liquid);
    let ambient = if sinks {
        ambient_density(window, registry, pos)
    } else {
        registry.get(MaterialId::AIR).density
    };
    if sinks {
        let buoy = ((density - ambient) / density).clamp(0.0, 1.0);
        vy -= (*GRAVITY_DV as f32 * buoy).round() as i32;
    } else {
        vy += *GRAVITY_DV;
    }

    let mut drag_loss = 1.0 - dynamics.drag_keep;
    if ambient > SUBMERGED_DENSITY {
        drag_loss *= SUBMERGED_DRAG;
    }
    let keep = 1.0 - drag_loss.min(0.9);
    vx = (vx as f32 * keep).round() as i32;
    vy = (vy as f32 * keep).round() as i32;

    if dynamics.turbulence > 0.0 {
        let r = (rng.draw().bits(16) as i32 - 32768) as f32 / 32768.0;
        vx += (dynamics.turbulence * r).round() as i32;
    }

    let below = pos.translated(0, -1);
    let supported = !can_enter(window, registry, density, (0, -1), below);
    if supported {
        vx = (vx as f32 * dynamics.friction_keep).round() as i32;
    }
    if window.get(below).is_some_and(|c| c.is_body()) {
        window.note_structural(below);
    }

    if dynamics.cohesion > 0.0
        && let Some((mean_x, mean_y)) = neighbor_mean_vel(window, registry, pos, phase)
    {
        vx += (dynamics.cohesion * (mean_x - vx) as f32).round() as i32;
        vy += (dynamics.cohesion * (mean_y - vy) as f32).round() as i32;
    }

    vx = vx.clamp(-VEL_MAX, VEL_MAX);
    vy = vy.clamp(-VEL_MAX, VEL_MAX);

    let tx = step_cells(vx, rng);
    let ty = step_cells(vy, rng);
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
            if can_enter(window, registry, density, (sx, 0), next) {
                window.swap(cur, next);
                cur = next;
                moved = true;
                done_x += 1;
            } else {
                vx = reflect(vx, dynamics.restitution);
                done_x = ix;
            }
        } else {
            let next = cur.translated(0, sy);
            if can_enter(window, registry, density, (0, sy), next) {
                window.swap(cur, next);
                cur = next;
                moved = true;
                done_y += 1;
            } else {
                done_y = iy;
            }
        }
    }

    let gdir = if sinks { -1 } else { 1 };
    let ahead = cur.translated(0, gdir);
    if !can_enter(window, registry, density, (0, gdir), ahead) {
        moved |= redirect(
            window, registry, &mut cur, density, is_powder, gdir, &mut vx, vy, dynamics, rng,
        );
    }

    if moved {
        note_undermined(window, registry, pos);
    }

    for (dx, dy) in NEIGHBORS {
        let into = if dx != 0 { vx * dx > 0 } else { vy * dy > 0 };
        let target = cur.translated(dx, dy);
        if into && !can_enter(window, registry, density, (dx, dy), target) {
            if dx != 0 {
                vx = reflect(vx, dynamics.restitution);
            } else {
                vy = reflect(vy, dynamics.restitution);
            }
        }
    }
    let settled = !can_enter(
        window,
        registry,
        density,
        (0, gdir),
        cur.translated(0, gdir),
    );
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
    }
}

#[allow(clippy::too_many_arguments)]
fn redirect(
    window: &mut SimWindow,
    registry: &MaterialRegistry,
    cur: &mut CellPos,
    density: f32,
    is_powder: bool,
    vdir: i32,
    vx: &mut i32,
    vy: i32,
    dynamics: Dynamics,
    rng: &mut Rng,
) -> bool {
    let gain = (dynamics.redirect_keep * vy.unsigned_abs() as f32).round() as i32;
    let prefer = match (*vx).cmp(&0) {
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => {
            if rng.draw().bit() {
                1
            } else {
                -1
            }
        }
    };
    let can_flow = dynamics.flow_chance >= 1.0 || rng.draw().chance(dynamics.flow_chance);
    for side in [prefer, -prefer] {
        let beside = cur.translated(side, 0);
        if !can_enter(window, registry, density, (side, 0), beside) {
            continue;
        }
        let diag = cur.translated(side, vdir);
        let diag_open = can_enter(window, registry, density, (side, vdir), diag);
        if is_powder {
            if diag_open && rng.draw().chance(dynamics.slide_chance) {
                *vx += side * gain;
                window.swap(*cur, diag);
                *cur = diag;
                return true;
            }
            continue;
        }
        if !can_flow {
            window.mark(*cur);
            return false;
        }
        if diag_open {
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
