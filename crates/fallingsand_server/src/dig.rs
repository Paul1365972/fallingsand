use crate::inventory::Inventory;
use crate::player::{PlayerLife, Players};
use fallingsand_core::content;
use fallingsand_core::{CellPos, ItemId, ItemStack, MaterialId, Phase, TICK_DT, Tag, ray_cells};
use fallingsand_protocol::{
    CursorMode, GameMode, InputState, InteractionState, InteractionStatus, UseButton,
};

const BARE_HAND_SPEED: f32 = 0.55;
const CREATIVE_REACH: f32 = 100.0;
const SURVIVAL_REACH: f32 = 20.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum MiningMethod {
    Hands,
    Tool(ItemId),
}

struct DigProgress {
    target: CellPos,
    material: MaterialId,
    method: MiningMethod,
    elapsed: f32,
}

#[derive(Default)]
pub struct DigState {
    progress: Option<DigProgress>,
    pub interaction: Option<InteractionState>,
}

impl DigState {
    fn clear_progress(&mut self) {
        self.progress = None;
    }
}

type Actor = fallingsand_sim::physics::Actor;
type World = fallingsand_sim::CellWorld;

#[derive(Clone, Copy)]
struct InteractionContext {
    input: InputState,
    selected_slot: u8,
    survival: bool,
    reach: f32,
}

impl InteractionContext {
    fn with_aim(self, aim: CellPos) -> Self {
        Self {
            input: InputState { aim, ..self.input },
            ..self
        }
    }
}

pub fn apply_player_inputs(sim: &mut World, players: &mut Players) {
    for (_, player) in players.iter_mut() {
        let input = player.control.input;
        let uses = std::mem::take(&mut player.control.pending_uses);
        let survival = player.profile.mode == GameMode::Survival;
        let PlayerLife::Alive(avatar) = &mut player.life else {
            continue;
        };
        let context = InteractionContext {
            input,
            selected_slot: player.profile.selected_slot,
            survival,
            reach: if survival {
                SURVIVAL_REACH
            } else {
                CREATIVE_REACH
            },
        };
        let body = &avatar.actor;
        let dig = &mut avatar.dig;
        let inventory = &mut player.profile.inventory;

        let mut tapped_dig = None;
        for (button, cell) in uses {
            match button {
                UseButton::Primary if survival => tapped_dig = Some(cell),
                UseButton::Primary => {
                    active_dig(sim, &context.with_aim(cell), body, dig, inventory)
                }
                UseButton::Secondary => {
                    active_place(sim, &context.with_aim(cell), body, dig, inventory)
                }
            }
        }

        if survival && input.primary {
            active_dig(sim, &context, body, dig, inventory);
        } else if let Some(cell) = tapped_dig {
            active_dig(sim, &context.with_aim(cell), body, dig, inventory);
        } else {
            dig.clear_progress();
            if !input.primary && !input.secondary {
                dig.interaction = idle_preview(sim, &context, body, inventory);
            }
        }
    }
}

fn active_dig(
    world: &mut World,
    context: &InteractionContext,
    body: &Actor,
    dig: &mut DigState,
    inventory: &mut Inventory,
) {
    let Some(target) = select_dig(world, &context.input, body, context.reach, context.survival)
    else {
        dig.clear_progress();
        dig.interaction = Some(interaction(
            context.input.aim,
            miss_reason(body, &context.input, context.reach),
            0.0,
        ));
        return;
    };
    let plan = match classify_dig(world, inventory, context, target) {
        Ok(plan) => plan,
        Err(status) => {
            dig.clear_progress();
            dig.interaction = Some(interaction(target, status, 0.0));
            return;
        }
    };

    let matches_progress = dig.progress.as_ref().is_some_and(|p| {
        p.target == target && p.material == plan.material && p.method == plan.method
    });
    if !matches_progress {
        dig.progress = Some(DigProgress {
            target,
            material: plan.material,
            method: plan.method,
            elapsed: 0.0,
        });
    }
    let progress = dig.progress.as_mut().unwrap();
    progress.elapsed += TICK_DT;
    let hardness = content::material(plan.material).hardness.max(0.01);
    let duration = if context.survival {
        hardness / plan.speed
    } else {
        0.0
    };
    let fraction = if duration <= 0.0 {
        1.0
    } else {
        (progress.elapsed / duration).clamp(0.0, 1.0)
    };
    if fraction < 1.0 {
        dig.interaction = Some(dig_interaction(target, fraction, plan.material));
        return;
    }

    if context.survival {
        let leftover = inventory
            .inner
            .insert_first_fit(ItemStack::new(plan.item, 1));
        if leftover.is_some() {
            dig.interaction = Some(interaction(target, InteractionStatus::InventoryFull, 0.0));
            return;
        }
    }
    world.clear_cell(target);
    dig.clear_progress();
    dig.interaction = Some(dig_interaction(target, 1.0, plan.material));
}

fn active_place(
    world: &mut World,
    context: &InteractionContext,
    body: &Actor,
    dig: &mut DigState,
    inventory: &mut Inventory,
) {
    dig.clear_progress();
    let slot = context.selected_slot as usize;
    let Some(material) = inventory
        .inner
        .get(slot)
        .and_then(|stack| content::try_item(stack.item).and_then(|info| info.place))
    else {
        dig.interaction = Some(interaction(
            context.input.aim,
            InteractionStatus::NotPlaceable,
            0.0,
        ));
        return;
    };
    let Some(target) = select_place(world, &context.input, body, context.reach) else {
        dig.interaction = Some(interaction(
            context.input.aim,
            miss_reason(body, &context.input, context.reach),
            0.0,
        ));
        return;
    };

    let placed = if context.survival {
        world.fill_material(target, material)
    } else {
        world.fill_material_quiet(target, material)
    };
    if !placed {
        dig.interaction = Some(interaction(target, InteractionStatus::Occupied, 0.0));
        return;
    }
    if context.survival {
        let stack = inventory.inner.get(slot).expect("placeable slot occupied");
        let count = stack.count.saturating_sub(1);
        inventory.inner.set(
            slot,
            (count > 0).then_some(ItemStack::new(stack.item, count)),
        );
    }
    dig.interaction = Some(interaction(target, InteractionStatus::Valid, 1.0));
}

fn idle_preview(
    world: &World,
    context: &InteractionContext,
    body: &Actor,
    inventory: &Inventory,
) -> Option<InteractionState> {
    let slot = context.selected_slot as usize;
    let placeable = inventory
        .inner
        .get(slot)
        .and_then(|stack| content::try_item(stack.item).and_then(|info| info.place))
        .is_some();

    if placeable {
        let target = select_place(world, &context.input, body, context.reach)?;
        return Some(interaction(target, InteractionStatus::Valid, 0.0));
    }
    let target = select_dig(world, &context.input, body, context.reach, context.survival)?;
    match classify_dig(world, inventory, context, target) {
        Ok(_) => Some(interaction(target, InteractionStatus::Valid, 0.0)),
        Err(_) => None,
    }
}

struct DigPlan {
    method: MiningMethod,
    speed: f32,
    item: ItemId,
    material: MaterialId,
}

fn classify_dig(
    world: &World,
    inventory: &Inventory,
    context: &InteractionContext,
    target: CellPos,
) -> Result<DigPlan, InteractionStatus> {
    let Some(cell) = world.get_cell(target) else {
        return Err(InteractionStatus::OutOfReach);
    };
    let material = cell.material;
    if !destructible(material, context.survival) {
        return Err(InteractionStatus::Occupied);
    }
    let item = content::item_for_material(material);
    if context.survival && item == ItemId::NONE {
        return Err(InteractionStatus::Undiggable);
    }
    let slot = context.selected_slot as usize;
    let held = inventory.inner.get(slot);
    let (method, speed, tier) = match held.and_then(|stack| {
        content::try_item(stack.item).and_then(|info| info.tool.map(|t| (stack.item, t)))
    }) {
        Some((id, tool)) => (MiningMethod::Tool(id), tool.speed, tool.tier),
        None => (MiningMethod::Hands, BARE_HAND_SPEED, 0),
    };
    if context.survival {
        if tier < content::material(material).mining_tier {
            return Err(InteractionStatus::WrongTool);
        }
        if !inventory.inner.can_insert(ItemStack::new(item, 1)) {
            return Err(InteractionStatus::InventoryFull);
        }
    }
    Ok(DigPlan {
        method,
        speed,
        item,
        material,
    })
}

fn select_dig(
    world: &World,
    input: &InputState,
    body: &Actor,
    reach: f32,
    survival: bool,
) -> Option<CellPos> {
    let aim = input.aim;
    match input.cursor_mode {
        CursorMode::Precise => (diggable(world, aim, survival)
            && cell_distance_sq(body, aim) <= reach * reach)
            .then_some(aim),
        CursorMode::Smart => smart_dig_target(world, body, aim, reach, survival),
    }
}

fn smart_dig_target(
    world: &World,
    body: &Actor,
    aim: CellPos,
    reach: f32,
    survival: bool,
) -> Option<CellPos> {
    let footprint = body.footprint();
    let aim_offset_x = aim.x as f32 + 0.5 - body.x.to_cells();
    let aim_offset_y = aim.y as f32 + 0.5 - body.y.to_cells();
    let max_distance = reach.ceil() as i32 + 1;
    let sweep_horizontally = aim_offset_x.abs() >= aim_offset_y.abs();
    let positive_direction = (if sweep_horizontally {
        aim_offset_x
    } else {
        aim_offset_y
    }) >= 0.0;
    let in_reach = |pos: CellPos| cell_distance_sq(body, pos) <= reach * reach;
    for distance in 1..=max_distance {
        let nearest_target = if sweep_horizontally {
            let x = if positive_direction {
                footprint.x1 + distance
            } else {
                footprint.x0 - distance
            };
            (footprint.y0..=footprint.y1)
                .map(|y| CellPos::new(x, y))
                .filter(|&pos| diggable(world, pos, survival) && in_reach(pos))
                .min_by_key(|pos| (pos.y - aim.y).abs())
        } else {
            let y = if positive_direction {
                footprint.y1 + distance
            } else {
                footprint.y0 - distance
            };
            (footprint.x0..=footprint.x1)
                .map(|x| CellPos::new(x, y))
                .filter(|&pos| diggable(world, pos, survival) && in_reach(pos))
                .min_by_key(|pos| (pos.x - aim.x).abs())
        };
        if let Some(target) = nearest_target {
            return Some(target);
        }
    }
    None
}

fn select_place(world: &World, input: &InputState, body: &Actor, reach: f32) -> Option<CellPos> {
    let aim = input.aim;
    let target = match input.cursor_mode {
        CursorMode::Precise => world
            .get_cell(aim)
            .filter(|cell| cell.is_air())
            .map(|_| aim)?,
        CursorMode::Smart => {
            let start = body.cell();
            let end = clamp_to_reach(body, aim, reach);
            last_air_before_obstruction(world, start, end)?
        }
    };
    (cell_distance_sq(body, target) <= reach * reach).then_some(target)
}

fn miss_reason(body: &Actor, input: &InputState, reach: f32) -> InteractionStatus {
    if cell_distance_sq(body, input.aim) <= reach * reach {
        InteractionStatus::NoTarget
    } else {
        InteractionStatus::OutOfReach
    }
}

fn clamp_to_reach(body: &Actor, aim: CellPos, reach: f32) -> CellPos {
    let cx = body.x.to_cells();
    let cy = body.y.to_cells();
    let dx = aim.x as f32 + 0.5 - cx;
    let dy = aim.y as f32 + 0.5 - cy;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist <= reach || dist == 0.0 {
        return aim;
    }
    let scale = reach / dist;
    CellPos::new(
        (cx + dx * scale - 0.5).round() as i32,
        (cy + dy * scale - 0.5).round() as i32,
    )
}

fn last_air_before_obstruction(world: &World, start: CellPos, end: CellPos) -> Option<CellPos> {
    let mut last = None;
    for pos in ray_cells(start, end) {
        let Some(cell) = world.get_cell(pos) else {
            break;
        };
        if cell.is_air() {
            last = Some(pos);
        } else if !content::tags(cell.material).contains(Tag::Player) {
            break;
        }
    }
    last
}

fn destructible(material: MaterialId, survival: bool) -> bool {
    if content::tags(material).contains(Tag::Player) {
        return false;
    }
    match content::phase(material) {
        Phase::Solid | Phase::Powder => true,
        Phase::Liquid | Phase::Gas => !survival,
        Phase::Empty => false,
    }
}

fn diggable(world: &World, pos: CellPos, survival: bool) -> bool {
    world
        .get_cell(pos)
        .is_some_and(|cell| destructible(cell.material, survival))
}

fn cell_distance_sq(body: &Actor, pos: CellPos) -> f32 {
    let dx = pos.x as f32 + 0.5 - body.x.to_cells();
    let dy = pos.y as f32 + 0.5 - body.y.to_cells();
    dx * dx + dy * dy
}

fn interaction(target: CellPos, status: InteractionStatus, progress: f32) -> InteractionState {
    InteractionState {
        target,
        status,
        progress,
        dig_material: None,
    }
}

fn dig_interaction(target: CellPos, progress: f32, material: MaterialId) -> InteractionState {
    InteractionState {
        target,
        status: InteractionStatus::Valid,
        progress,
        dig_material: Some(material),
    }
}
