use crate::inventory::{Inventory, ItemReg};
use crate::player::{DigState, Mode, Player, PlayerActor};
use crate::{Registry, SimWorld};
use bevy_ecs::prelude::*;
use fallingsand_core::{
    CellPos, Fixed, ItemId, ItemRegistry, ItemStack, MAX_BRUSH, MaterialId, MaterialRegistry,
    Phase, REACH, SURVIVAL_REACH, TICK_DT,
};
use fallingsand_protocol::GameMode;

pub fn apply_player_inputs(
    mut sim: ResMut<SimWorld>,
    registry: Res<Registry>,
    item_reg: Res<ItemReg>,
    mut bodies: ResMut<crate::bodies::PixelBodies>,
    mut query: Query<(&Player, &PlayerActor, &Mode, &mut DigState, &mut Inventory)>,
) {
    let reg = &item_reg.0;
    let player_mask = registry.0.tag_mask("player");
    for (player, body, mode, mut dig, mut inventory) in &mut query {
        let input = &player.input;
        let survival = mode.0 == GameMode::Survival;
        let radius = player.brush_radius.min(MAX_BRUSH) as i32;
        if !input.primary {
            dig.budget = 0.0;
        }
        if !input.primary && !input.secondary {
            continue;
        }
        let reach = if survival { SURVIVAL_REACH } else { REACH };
        let dx = (Fixed::from_cell(input.aim.x) - body.0.x).to_f32();
        let dy = (Fixed::from_cell(input.aim.y) - body.0.y).to_f32();
        if dx * dx + dy * dy > reach * reach {
            continue;
        }
        let mut dug = false;
        if input.primary {
            if survival {
                dug = survival_dig(
                    &mut sim.0,
                    &registry.0,
                    reg,
                    &mut dig,
                    &mut inventory,
                    input.aim,
                    radius,
                );
            } else {
                for (_, pos) in brush_cells(input.aim, radius) {
                    let Some(cell) = sim.0.get_cell(pos) else {
                        continue;
                    };
                    if registry.0.has_tag(cell.material, player_mask) {
                        continue;
                    }
                    if registry.0.get(cell.material).phase != Phase::Empty {
                        sim.0.place_material(pos, MaterialId::AIR);
                        dug = true;
                    }
                }
            }
        } else if input.secondary {
            let slot = player.selected_slot as usize;
            if slot < fallingsand_core::HOTBAR_SLOTS
                && let Some(stack) = inventory.inner.get(slot)
                && let Some(material) = reg.try_get(stack.item).and_then(|def| def.place)
            {
                let mut placed = 0u32;
                let budget = if survival { stack.count } else { u32::MAX };
                for (_, pos) in brush_cells(input.aim, radius) {
                    if placed >= budget {
                        break;
                    }
                    let Some(cell) = sim.0.get_cell(pos) else {
                        continue;
                    };
                    if !cell.is_air() {
                        continue;
                    }
                    sim.0.place_material(pos, material);
                    placed += 1;
                }
                if survival && placed > 0 {
                    consume_slot(&mut inventory, slot, placed);
                }
            }
        }
        if dug {
            let ring = radius + 1;
            for oy in -ring..=ring {
                for ox in -ring..=ring {
                    let dist_sq = ox * ox + oy * oy;
                    if dist_sq <= radius * radius || dist_sq > ring * ring {
                        continue;
                    }
                    bodies.candidates.push(input.aim.translated(ox, oy));
                }
            }
        }
    }
}

fn consume_slot(inventory: &mut Inventory, slot: usize, amount: u32) {
    if let Some(stack) = inventory.inner.get(slot) {
        let count = stack.count.saturating_sub(amount);
        inventory
            .inner
            .set(slot, Some(ItemStack::new(stack.item, count)));
    }
    inventory.dirty = true;
}

fn brush_cells(aim: CellPos, radius: i32) -> impl Iterator<Item = (i32, CellPos)> {
    (-radius..=radius).flat_map(move |oy| {
        (-radius..=radius).filter_map(move |ox| {
            let dist_sq = ox * ox + oy * oy;
            (dist_sq <= radius * radius).then_some((dist_sq, aim.translated(ox, oy)))
        })
    })
}

pub fn survival_dig(
    world: &mut fallingsand_sim::CellWorld,
    registry: &MaterialRegistry,
    item_reg: &ItemRegistry,
    dig: &mut DigState,
    inventory: &mut Inventory,
    aim: CellPos,
    radius: i32,
) -> bool {
    let player_mask = registry.tag_mask("player");
    let mut candidates: Vec<(i32, i32, i32)> = brush_cells(aim, radius)
        .filter(|&(_, pos)| {
            world.get_cell(pos).is_some_and(|cell| {
                !registry.has_tag(cell.material, player_mask)
                    && matches!(
                        registry.get(cell.material).phase,
                        Phase::Solid | Phase::Powder
                    )
            })
        })
        .map(|(dist_sq, pos)| (dist_sq, pos.y, pos.x))
        .collect();
    candidates.sort_unstable();
    let Some(&(_, y, x)) = candidates.first() else {
        dig.budget = 0.0;
        return false;
    };
    let closest_cost = world
        .get_cell(CellPos::new(x, y))
        .map(|cell| registry.get(cell.material).hardness)
        .unwrap_or(0.0);
    dig.budget = (dig.budget + TICK_DT).min(closest_cost + TICK_DT);
    let mut dug = false;
    for &(_, y, x) in &candidates {
        let pos = CellPos::new(x, y);
        let Some(cell) = world.get_cell(pos) else {
            continue;
        };
        let cost = registry.get(cell.material).hardness;
        if dig.budget < cost {
            break;
        }
        let item = item_reg.item_for_material(cell.material);
        if item != ItemId::NONE {
            if inventory
                .inner
                .insert_first_fit(ItemStack::new(item, 1), item_reg)
                .is_some()
            {
                continue;
            }
            inventory.dirty = true;
        }
        dig.budget -= cost;
        world.place_material(pos, MaterialId::AIR);
        dug = true;
    }
    dug
}
