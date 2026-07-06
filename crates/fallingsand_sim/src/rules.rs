use crate::obstacles::Obstacles;
use crate::window::SimWindow;
use fallingsand_core::{Cell, CellPos, MaterialId, MaterialRegistry, Phase};
use std::hash::{Hash, Hasher};

const NEIGHBORS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];
const SALT_REACT: u32 = 1;
const SALT_DECAY: u32 = 2;
const SALT_FLICKER: u32 = 3;
const SALT_FLOW: u32 = 4;
const SALT_FALL: u32 = 5;
const FLICKER_RATE: f32 = 18.0;
const FLICKER_CHANCE: f32 = FLICKER_RATE * fallingsand_core::TICK_DT;

pub(crate) fn update_cell(
    window: &mut SimWindow,
    registry: &MaterialRegistry,
    obstacles: &Obstacles,
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
    if registry.is_reactive(cell.material) && react(window, registry, pos, cell, tick, tick_byte) {
        return;
    }
    let material = registry.get(cell.material);
    match material.phase {
        Phase::Empty | Phase::Solid => {}
        Phase::Powder => update_powder(window, registry, obstacles, pos, cell, tick),
        Phase::Liquid => update_liquid(window, registry, pos, cell, tick),
        Phase::Gas | Phase::Fire => update_gas(window, registry, pos, cell, tick),
    }
}

fn react(
    window: &mut SimWindow,
    registry: &MaterialRegistry,
    pos: CellPos,
    cell: Cell,
    tick: u64,
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
            if roll(pos, tick, SALT_REACT, reaction.chance) {
                note_structural(window, registry, pos, cell.material);
                note_structural(window, registry, neighbor_pos, neighbor.material);
                set_product(window, registry, pos, reaction.becomes, tick, tick_byte);
                set_product(
                    window,
                    registry,
                    neighbor_pos,
                    reaction.other_becomes,
                    tick,
                    tick_byte,
                );
                return true;
            }
        }
    }
    if let Some((chance, product)) = registry.decay(cell.material) {
        let material = registry.get(cell.material);
        if material.phase == Phase::Fire && sustained(window, registry, pos, cell.material) {
            if roll(pos, tick, SALT_FLICKER, FLICKER_CHANCE) {
                let mut flicker = cell;
                flicker.set_shade(hash_shade(pos, tick));
                flicker.updated = tick_byte;
                window.set(pos, flicker);
            } else {
                window.mark(pos);
            }
            return true;
        }
        if roll(pos, tick, SALT_DECAY, chance) {
            set_product(window, registry, pos, product, tick, tick_byte);
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

fn set_product(
    window: &mut SimWindow,
    registry: &MaterialRegistry,
    pos: CellPos,
    material: MaterialId,
    tick: u64,
    tick_byte: u8,
) {
    let mut cell = Cell::new(material, hash_shade(pos, tick));
    cell.updated = tick_byte;
    window.set(pos, cell);
    if matches!(
        registry.get(material).phase,
        Phase::Empty | Phase::Gas | Phase::Fire
    ) {
        wake_range(window, pos);
    }
}

fn hash_shade(pos: CellPos, tick: u64) -> u8 {
    let mut hasher = rustc_hash::FxHasher::default();
    (pos.x, pos.y, tick).hash(&mut hasher);
    (hasher.finish() & 0xF) as u8
}

fn roll(pos: CellPos, tick: u64, salt: u32, chance: f32) -> bool {
    if chance <= 0.0 {
        return false;
    }
    let mut hasher = rustc_hash::FxHasher::default();
    (pos.x, pos.y, tick, salt).hash(&mut hasher);
    ((hasher.finish() as u32) as f32) < chance * u32::MAX as f32
}

fn fluid_displaceable(
    window: &SimWindow,
    registry: &MaterialRegistry,
    density: f32,
    pos: CellPos,
) -> bool {
    match window.get(pos) {
        Some(target) => {
            let material = registry.get(target.material);
            matches!(
                material.phase,
                Phase::Empty | Phase::Liquid | Phase::Gas | Phase::Fire
            ) && density > material.density
        }
        None => false,
    }
}

fn lighter_fluid_above(
    window: &SimWindow,
    registry: &MaterialRegistry,
    density: f32,
    pos: CellPos,
) -> bool {
    match window.get(pos) {
        Some(target) => {
            let material = registry.get(target.material);
            matches!(
                material.phase,
                Phase::Empty | Phase::Liquid | Phase::Gas | Phase::Fire
            ) && material.density > density
        }
        None => false,
    }
}

fn passable(window: &SimWindow, registry: &MaterialRegistry, pos: CellPos) -> bool {
    window.get(pos).is_some_and(|cell| {
        matches!(
            registry.get(cell.material).phase,
            Phase::Empty | Phase::Liquid | Phase::Gas | Phase::Fire
        )
    })
}

fn free_path(window: &SimWindow, registry: &MaterialRegistry, pos: CellPos) -> bool {
    window.get(pos).is_some_and(|cell| {
        matches!(
            registry.get(cell.material).phase,
            Phase::Empty | Phase::Gas | Phase::Fire
        )
    })
}

const WAKE_SPAN: i32 = 32;

fn wake_range(window: &mut SimWindow, pos: CellPos) {
    for row in pos.y - 1..=pos.y + 1 {
        let end = pos.x + WAKE_SPAN;
        let mut x = pos.x - WAKE_SPAN;
        loop {
            window.mark(CellPos::new(x, row));
            let chunk_end = x | (fallingsand_core::CHUNK_SIZE as i32 - 1);
            if chunk_end >= end {
                window.mark(CellPos::new(end, row));
                break;
            }
            window.mark(CellPos::new(chunk_end, row));
            x = chunk_end + 1;
        }
    }
}

fn flows(pos: CellPos, tick: u64, chance: f32) -> bool {
    chance >= 1.0 || roll(pos, tick, SALT_FLOW, chance)
}

fn fall_count(registry: &MaterialRegistry, id: MaterialId, pos: CellPos, tick: u64) -> i32 {
    let (base, frac) = registry.fall_steps(id);
    base as i32 + roll(pos, tick, SALT_FALL, frac) as i32
}

fn flow_order(cell: Cell, pos: CellPos, tick: u64) -> [i32; 2] {
    match cell.flow_state() {
        Cell::FLOW_LEFT => [-1, 1],
        Cell::FLOW_RIGHT => [1, -1],
        _ => side_order(pos, tick),
    }
}

fn dir_state(dir: i32) -> u8 {
    if dir < 0 {
        Cell::FLOW_LEFT
    } else {
        Cell::FLOW_RIGHT
    }
}

fn stamp_flow_state(window: &mut SimWindow, pos: CellPos, state: u8) {
    if let Some(mut cell) = window.get(pos)
        && (cell.flow_state() != state || cell.flow_spent())
    {
        cell.set_flow_state(state);
        cell.set_flow_spent(false);
        window.set(pos, cell);
    }
}

fn stamp_flow_dir(window: &mut SimWindow, pos: CellPos, state: u8) {
    if let Some(mut cell) = window.get(pos)
        && cell.flow_state() != state
    {
        cell.set_flow_state(state);
        window.set(pos, cell);
    }
}

fn coin_flip(pos: CellPos, tick: u64) -> bool {
    let mut hasher = rustc_hash::FxHasher::default();
    (pos.x, pos.y, tick).hash(&mut hasher);
    hasher.finish() & 1 == 1
}

fn side_order(pos: CellPos, tick: u64) -> [i32; 2] {
    if coin_flip(pos, tick) {
        [1, -1]
    } else {
        [-1, 1]
    }
}

fn update_powder(
    window: &mut SimWindow,
    registry: &MaterialRegistry,
    obstacles: &Obstacles,
    pos: CellPos,
    cell: Cell,
    tick: u64,
) {
    let density = registry.get(cell.material).density;
    let below = pos.translated(0, -1);
    if fluid_displaceable(window, registry, density, below) && !obstacles.occupied(below) {
        let steps = fall_count(registry, cell.material, pos, tick);
        if steps == 0 {
            window.mark(pos);
            return;
        }
        let mut target = below;
        if free_path(window, registry, below) {
            for _ in 1..steps {
                let next = target.translated(0, -1);
                if !fluid_displaceable(window, registry, density, next)
                    || obstacles.occupied(next)
                    || !free_path(window, registry, next)
                {
                    break;
                }
                target = next;
            }
        }
        window.swap(pos, target);
        wake_range(window, pos);
        note_undermined(window, registry, pos);
        return;
    }
    let below_open = passable(window, registry, below) && !obstacles.occupied(below);
    for side in side_order(pos, tick) {
        let beside = pos.translated(side, 0);
        let beside_open = passable(window, registry, beside) && !obstacles.occupied(beside);
        if !below_open && !beside_open {
            continue;
        }
        let diag = pos.translated(side, -1);
        if fluid_displaceable(window, registry, density, diag) && !obstacles.occupied(diag) {
            window.swap(pos, diag);
            wake_range(window, pos);
            note_undermined(window, registry, pos);
            return;
        }
    }
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

fn update_liquid(
    window: &mut SimWindow,
    registry: &MaterialRegistry,
    pos: CellPos,
    cell: Cell,
    tick: u64,
) {
    let material = registry.get(cell.material);
    let density = material.density;
    let dispersion = material.dispersion;
    let flow_chance = registry.flow_chance(cell.material);
    let below = pos.translated(0, -1);
    if window.get(below).is_some_and(|cell| cell.is_body()) {
        window.note_structural(below);
    }
    if fluid_displaceable(window, registry, density, below) {
        if free_path(window, registry, below) {
            let steps = fall_count(registry, cell.material, pos, tick);
            if steps == 0 {
                window.mark(pos);
                return;
            }
            let mut target = below;
            for _ in 1..steps {
                let next = target.translated(0, -1);
                if !fluid_displaceable(window, registry, density, next)
                    || !free_path(window, registry, next)
                {
                    break;
                }
                target = next;
            }
            window.swap(pos, target);
            wake_range(window, pos);
            let splash = match cell.flow_state() {
                Cell::FLOW_NONE => dir_state(if coin_flip(pos, tick) { 1 } else { -1 }),
                state => state,
            };
            stamp_flow_state(window, target, splash);
        } else if flows(pos, tick, flow_chance) {
            window.swap(pos, below);
            wake_range(window, pos);
        } else {
            window.mark(pos);
        }
        return;
    }
    let order = flow_order(cell, pos, tick);
    let Some((target, dir)) = liquid_flow_target(window, registry, pos, density, dispersion, order)
    else {
        creep(window, registry, pos, cell, tick, flow_chance);
        return;
    };
    if flows(pos, tick, flow_chance) {
        window.swap(pos, target);
        wake_range(window, pos);
        stamp_flow_state(window, target, dir_state(dir));
    } else {
        window.mark(pos);
    }
}

fn creep(
    window: &mut SimWindow,
    registry: &MaterialRegistry,
    pos: CellPos,
    cell: Cell,
    tick: u64,
    flow_chance: f32,
) {
    let below = pos.translated(0, -1);
    let on_liquid = window
        .get(below)
        .is_some_and(|c| registry.get(c.material).phase == Phase::Liquid);
    if !on_liquid {
        return;
    }
    let density = registry.get(cell.material).density;
    let open = |window: &SimWindow, side: i32| {
        fluid_displaceable(window, registry, density, pos.translated(side, 0))
    };
    let surface_on = |window: &SimWindow, side: i32| {
        open(window, side)
            && window
                .get(pos.translated(side, -1))
                .is_some_and(|c| registry.get(c.material).phase == Phase::Liquid)
    };
    let dir = match cell.flow_state() {
        Cell::FLOW_LEFT => -1,
        Cell::FLOW_RIGHT => 1,
        _ => {
            let Some(dir) = side_order(pos, tick)
                .into_iter()
                .find(|&side| surface_on(window, side))
            else {
                return;
            };
            dir
        }
    };
    if open(window, dir) {
        if flows(pos, tick, flow_chance) {
            let target = pos.translated(dir, 0);
            window.swap(pos, target);
            wake_range(window, pos);
            stamp_flow_dir(window, target, dir_state(dir));
        } else {
            window.mark(pos);
        }
    } else if !cell.flow_spent() && surface_on(window, -dir) {
        let mut reversed = cell;
        reversed.set_flow_state(dir_state(-dir));
        reversed.set_flow_spent(true);
        reversed.updated = tick as u8;
        window.set(pos, reversed);
    }
}

fn liquid_flow_target(
    window: &SimWindow,
    registry: &MaterialRegistry,
    pos: CellPos,
    density: f32,
    dispersion: u8,
    order: [i32; 2],
) -> Option<(CellPos, i32)> {
    let below = pos.translated(0, -1);
    let below_open = passable(window, registry, below);
    for side in order {
        let beside = pos.translated(side, 0);
        let beside_open = passable(window, registry, beside);
        if !below_open && !beside_open {
            continue;
        }
        let diag = pos.translated(side, -1);
        if fluid_displaceable(window, registry, density, diag) {
            return Some((diag, side));
        }
    }
    let reach = dispersion.max(1) as i32;
    for side in order {
        for distance in 1..=WAKE_SPAN {
            let target = pos.translated(side * distance, 0);
            if !fluid_displaceable(window, registry, density, target) {
                break;
            }
            let drop = target.translated(0, -1);
            if fluid_displaceable(window, registry, density, drop) {
                return Some((pos.translated(side * distance.min(reach), 0), side));
            }
        }
    }
    None
}

fn update_gas(
    window: &mut SimWindow,
    registry: &MaterialRegistry,
    pos: CellPos,
    cell: Cell,
    tick: u64,
) {
    let material = registry.get(cell.material);
    let density = material.density;
    let dispersion = material.dispersion;
    let flow_chance = registry.flow_chance(cell.material);
    let above = pos.translated(0, 1);
    if lighter_fluid_above(window, registry, density, above) {
        if free_path(window, registry, above) || flows(pos, tick, flow_chance) {
            window.swap(pos, above);
            wake_range(window, pos);
        } else {
            window.mark(pos);
        }
        return;
    }
    let order = flow_order(cell, pos, tick);
    let Some((target, dir)) = gas_flow_target(window, registry, pos, density, dispersion, order)
    else {
        return;
    };
    if flows(pos, tick, flow_chance) {
        window.swap(pos, target);
        wake_range(window, pos);
        stamp_flow_state(window, target, dir_state(dir));
    } else {
        window.mark(pos);
    }
}

fn gas_flow_target(
    window: &SimWindow,
    registry: &MaterialRegistry,
    pos: CellPos,
    density: f32,
    dispersion: u8,
    order: [i32; 2],
) -> Option<(CellPos, i32)> {
    let above = pos.translated(0, 1);
    let above_open = passable(window, registry, above);
    for side in order {
        let beside = pos.translated(side, 0);
        let beside_open = passable(window, registry, beside);
        if !above_open && !beside_open {
            continue;
        }
        let diag = pos.translated(side, 1);
        if lighter_fluid_above(window, registry, density, diag) {
            return Some((diag, side));
        }
    }
    let reach = dispersion.max(1) as i32;
    for side in order {
        for distance in 1..=WAKE_SPAN {
            let target = pos.translated(side * distance, 0);
            if !lighter_fluid_above(window, registry, density, target) {
                break;
            }
            let rise = target.translated(0, 1);
            if lighter_fluid_above(window, registry, density, rise) {
                return Some((pos.translated(side * distance.min(reach), 0), side));
            }
        }
    }
    None
}
