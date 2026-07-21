use crate::window::SimWindow;
use fallingsand_core::content::{self, MatSpec, material};
use fallingsand_core::{
    Burning, BurningKind, CARDINAL_NEIGHBORS as NEIGHBORS, Cell, CellPos, Ignition, MaterialId,
    Phase, SealedBurn, Tag,
};
use fallingsand_math::Rng;

pub(crate) fn apply<M: MatSpec>(window: &mut SimWindow, pos: CellPos, rng: &mut Rng) -> bool {
    if let Some(burning) = const { M::BURNING }
        && burn(window, pos, burning, rng)
    {
        return true;
    }
    if let Some(ignition) = const { M::IGNITION }
        && ignite(window, pos, ignition, rng)
    {
        return true;
    }
    (const { M::IS_REACTIVE }) && react::<M>(window, pos, rng)
}

fn react<M: MatSpec>(window: &mut SimWindow, pos: CellPos, rng: &mut Rng) -> bool {
    let mut pending = false;
    for (dx, dy) in NEIGHBORS {
        let neighbor_pos = pos.translated(dx, dy);
        let Some(neighbor) = window.get(neighbor_pos) else {
            continue;
        };
        let reaction = M::REACTIONS[neighbor.material.0 as usize];
        if reaction.threshold == 0 {
            continue;
        }
        pending = true;
        if rng.draw().below(reaction.threshold) {
            set_product(window, pos, reaction.becomes, rng);
            set_product(window, neighbor_pos, reaction.other_becomes, rng);
            return true;
        }
    }
    if let Some((threshold, product)) = const { M::DECAY } {
        pending = true;
        if rng.draw().below(threshold) {
            set_product(window, pos, product, rng);
            return true;
        }
    }
    if pending {
        window.mark(pos);
    }
    false
}

fn ignite(window: &mut SimWindow, pos: CellPos, ignition: Ignition, rng: &mut Rng) -> bool {
    if ignition.open == 0 && ignition.sealed == 0 {
        return false;
    }
    let hot = NEIGHBORS
        .iter()
        .filter(|&&(dx, dy)| {
            window
                .get(pos.translated(dx, dy))
                .is_some_and(|cell| content::tags(cell.material).contains(Tag::Hot))
        })
        .count();
    let threshold = if oxygen_exposed(window, pos) {
        ignition.open
    } else {
        ignition.sealed
    };
    if hot == 0 || threshold == 0 {
        return false;
    }
    window.mark(pos);
    if (0..hot).any(|_| rng.draw().below(threshold)) {
        let Some(mut cell) = window.get(pos) else {
            return true;
        };
        cell.material = ignition.into;
        cell.aux = 0;
        cell.set_body(false);
        window.set(pos, cell);
        return true;
    }
    false
}

fn burn(window: &mut SimWindow, pos: CellPos, burning: Burning, rng: &mut Rng) -> bool {
    if let Some(water) = adjacent_water(window, pos) {
        if burning.kind == BurningKind::Flame {
            set_product(window, pos, material::STEAM, rng);
        } else {
            burn_out(window, pos, burning, rng);
            set_product(window, water, material::STEAM, rng);
        }
        return true;
    }
    if rng.draw().below(burning.emit) {
        emit(window, pos, material::FIRE, rng);
    }
    let threshold = if oxygen_exposed(window, pos) {
        burning.burn
    } else {
        match burning.sealed {
            SealedBurn::Becomes(material) => {
                let Some(mut cell) = window.get(pos) else {
                    return true;
                };
                cell.material = material;
                cell.aux = 0;
                window.set(pos, cell);
                return true;
            }
            SealedBurn::Smoulder(threshold) => threshold,
        }
    };
    if rng.draw().below(threshold) {
        burn_out(window, pos, burning, rng);
        return true;
    }
    window.mark(pos);
    false
}

fn burn_out(window: &mut SimWindow, pos: CellPos, burning: Burning, rng: &mut Rng) {
    let material = match burning.residue {
        Some((threshold, material)) if rng.draw().below(threshold) => material,
        _ => burning.burnout,
    };
    set_product(window, pos, material, rng);
}

fn emit(window: &mut SimWindow, pos: CellPos, material: MaterialId, rng: &mut Rng) {
    let (dx, dy) = rng.draw().choose(&NEIGHBORS);
    let target = pos.translated(dx, dy);
    if window.get(target).is_some_and(Cell::is_air) {
        set_product(window, target, material, rng);
    }
}

fn adjacent_water(window: &SimWindow, pos: CellPos) -> Option<CellPos> {
    NEIGHBORS.iter().find_map(|&(dx, dy)| {
        let target = pos.translated(dx, dy);
        window
            .get(target)
            .is_some_and(|cell| cell.material == material::WATER)
            .then_some(target)
    })
}

fn oxygen_exposed(window: &SimWindow, pos: CellPos) -> bool {
    NEIGHBORS.iter().any(|&(dx, dy)| {
        window
            .get(pos.translated(dx, dy))
            .is_some_and(|cell| matches!(content::phase(cell.material), Phase::Empty | Phase::Gas))
    })
}

fn set_product(window: &mut SimWindow, pos: CellPos, material: MaterialId, rng: &mut Rng) {
    window.set(pos, Cell::new(material, rng.draw().bits(4) as u8));
}
