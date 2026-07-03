use crate::window::SimWindow;
use fallingsand_core::{CellPos, MaterialRegistry, Phase};
use std::hash::{Hash, Hasher};

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
    let material = registry.get(cell.material);
    match material.phase {
        Phase::Empty | Phase::Solid => {}
        Phase::Powder => update_powder(window, registry, pos, material.density, tick),
        Phase::Liquid => update_liquid(
            window,
            registry,
            pos,
            material.density,
            material.dispersion,
            tick,
        ),
        Phase::Gas => update_gas(
            window,
            registry,
            pos,
            material.density,
            material.dispersion,
            tick,
        ),
    }
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
            matches!(material.phase, Phase::Empty | Phase::Liquid | Phase::Gas)
                && density > material.density
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
            matches!(material.phase, Phase::Empty | Phase::Liquid | Phase::Gas)
                && material.density > density
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
    pos: CellPos,
    density: f32,
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
