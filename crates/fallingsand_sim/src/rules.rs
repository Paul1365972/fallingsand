use crate::obstacles::Obstacles;
use crate::window::SimWindow;
use fallingsand_core::{Cell, CellPos, MaterialId, MaterialRegistry, Phase};
use std::hash::{Hash, Hasher};

const NEIGHBORS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];
const SALT_REACT: u32 = 1;
const SALT_DECAY: u32 = 2;
const SALT_FLICKER: u32 = 3;
const FLICKER_CHANCE: f32 = 0.3;

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
        Phase::Powder => update_powder(window, registry, obstacles, pos, material.density, tick),
        Phase::Liquid => update_liquid(
            window,
            registry,
            pos,
            material.density,
            material.dispersion,
            tick,
        ),
        Phase::Gas | Phase::Fire => update_gas(
            window,
            registry,
            pos,
            material.density,
            material.dispersion,
            tick,
        ),
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
                set_product(window, pos, reaction.becomes, tick, tick_byte);
                set_product(
                    window,
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
            set_product(window, pos, product, tick, tick_byte);
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
    pos: CellPos,
    material: MaterialId,
    tick: u64,
    tick_byte: u8,
) {
    let mut cell = Cell::new(material, hash_shade(pos, tick));
    cell.updated = tick_byte;
    window.set(pos, cell);
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
    density: f32,
    tick: u64,
) {
    let below = pos.translated(0, -1);
    if fluid_displaceable(window, registry, density, below) && !obstacles.occupied(below) {
        window.swap(pos, below);
        return;
    }
    for side in side_order(pos, tick) {
        let diag = pos.translated(side, -1);
        if fluid_displaceable(window, registry, density, diag) && !obstacles.occupied(diag) {
            window.swap(pos, diag);
            return;
        }
    }
}

fn update_liquid(
    window: &mut SimWindow,
    registry: &MaterialRegistry,
    pos: CellPos,
    density: f32,
    dispersion: u8,
    tick: u64,
) {
    let below = pos.translated(0, -1);
    if fluid_displaceable(window, registry, density, below) {
        window.swap(pos, below);
        return;
    }
    for side in side_order(pos, tick) {
        let diag = pos.translated(side, -1);
        if fluid_displaceable(window, registry, density, diag) {
            window.swap(pos, diag);
            return;
        }
    }
    for side in side_order(pos, tick) {
        let mut best: Option<CellPos> = None;
        for distance in 1..=dispersion.max(1) as i32 {
            let target = pos.translated(side * distance, 0);
            if !fluid_displaceable(window, registry, density, target) {
                break;
            }
            best = Some(target);
            if fluid_displaceable(window, registry, density, target.translated(0, -1)) {
                break;
            }
        }
        if let Some(target) = best {
            window.swap(pos, target);
            return;
        }
    }
}

fn update_gas(
    window: &mut SimWindow,
    registry: &MaterialRegistry,
    pos: CellPos,
    density: f32,
    dispersion: u8,
    tick: u64,
) {
    let above = pos.translated(0, 1);
    if lighter_fluid_above(window, registry, density, above) {
        window.swap(pos, above);
        return;
    }
    for side in side_order(pos, tick) {
        let diag = pos.translated(side, 1);
        if lighter_fluid_above(window, registry, density, diag) {
            window.swap(pos, diag);
            return;
        }
    }
    for side in side_order(pos, tick) {
        let mut best: Option<CellPos> = None;
        for distance in 1..=dispersion.max(1) as i32 {
            let target = pos.translated(side * distance, 0);
            if !lighter_fluid_above(window, registry, density, target) {
                break;
            }
            best = Some(target);
        }
        if let Some(target) = best {
            window.swap(pos, target);
            return;
        }
    }
}
