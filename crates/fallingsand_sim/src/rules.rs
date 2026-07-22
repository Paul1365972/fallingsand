use crate::{chemistry, gas, liquid, motion, powder, window::SimWindow};
use fallingsand_core::content::MatSpec;
use fallingsand_core::{Cell, CellPos, Dynamics, Phase, content};
use fallingsand_math::Hash;

const EFFECT_SALT: Hash = Hash::label("simulation.effect");

macro_rules! material_dispatch {
    ($(($idx:literal, $name:ident, $spec:ident)),* $(,)?) => {
        pub(crate) fn effect_cell(window: &mut SimWindow, pos: CellPos, tick: u64) {
            let Some(cell) = window.get(pos) else {
                return;
            };
            match cell.material.0 {
                $( $idx => apply_effects::<content::specs::$spec>(window, pos, cell, tick), )*
                _ => unreachable!("invalid material id"),
            }
        }
    };
}
fallingsand_core::for_each_material!(material_dispatch);

fn apply_effects<M: MatSpec>(window: &mut SimWindow, pos: CellPos, cell: Cell, tick: u64) {
    if cell.is_body() {
        return;
    }
    let mut rng = Hash::seed(tick).salt(EFFECT_SALT).pos(pos.x, pos.y).rng();
    if chemistry::apply::<M>(window, pos, &mut rng) {
        return;
    }
    match const { M::DYNAMICS } {
        Dynamics::None => {}
        Dynamics::Powder(dynamics) => {
            powder::apply_effects::<M>(window, pos, cell, dynamics, &mut rng)
        }
        Dynamics::Liquid(dynamics) => liquid::apply_effects(window, pos, cell, dynamics),
        Dynamics::Gas(dynamics) => gas::apply_effects(window, pos, cell, dynamics),
    }
}

pub(crate) fn move_cell(window: &mut SimWindow, pos: CellPos, tick: u64) {
    let Some(cell) = window.get(pos) else {
        return;
    };
    if cell.is_body() {
        return;
    }
    if cell.flags & Cell::MOVED != 0 {
        window.mark(pos);
        return;
    }
    match content::phase(cell.material) {
        Phase::Powder if cell.vel() != (0, 0) => motion::move_cell(window, pos, cell, tick),
        Phase::Liquid => liquid::move_cell(window, pos, cell, tick),
        Phase::Gas => gas::move_cell(window, pos, cell, tick),
        Phase::Empty | Phase::Solid | Phase::Powder => {}
    }
}

pub(crate) fn random_tick(_window: &mut SimWindow, _pos: CellPos, _tick: u64) {}
