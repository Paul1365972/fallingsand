use crate::SimWorld;
use crate::inventory::{Inventory, ItemReg};
use crate::player::{Life, Mode, Player, PlayerActor};
use bevy_ecs::prelude::*;
use fallingsand_core::content;
use fallingsand_core::{
    CellPos, ItemId, ItemRegistry, ItemStack, MaterialId, Phase, REACH, SURVIVAL_REACH, TICK_DT,
    Tag,
};
use fallingsand_protocol::{CursorMode, GameMode, InteractionState, InteractionStatus, LifeState};

const BARE_HAND_SPEED: f32 = 0.55;

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

#[derive(Component, Default)]
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

pub fn apply_player_inputs(
    mut sim: ResMut<SimWorld>,
    item_reg: Res<ItemReg>,
    mut bodies: ResMut<crate::bodies::PixelBodies>,
    mut query: Query<(
        &Player,
        &PlayerActor,
        &Life,
        &Mode,
        &mut DigState,
        &mut Inventory,
    )>,
) {
    let reg = &item_reg.0;
    for (player, body, life, mode, mut dig, mut inventory) in &mut query {
        if life.0 != LifeState::Alive {
            *dig = DigState::default();
            continue;
        }
        let survival = mode.0 == GameMode::Survival;
        let reach = if survival { SURVIVAL_REACH } else { REACH };
        let body = &body.0;

        if player.input.primary {
            active_dig(
                &mut sim.0,
                reg,
                &mut bodies,
                player,
                body,
                survival,
                &mut dig,
                &mut inventory,
                reach,
            );
        } else if player.input.secondary {
            active_place(
                &mut sim.0,
                reg,
                player,
                body,
                survival,
                &mut dig,
                &mut inventory,
                reach,
            );
        } else {
            dig.clear_progress();
            dig.interaction = idle_preview(&sim.0, reg, player, body, survival, &inventory, reach);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn active_dig(
    world: &mut World,
    reg: &ItemRegistry,
    bodies: &mut crate::bodies::PixelBodies,
    player: &Player,
    body: &Actor,
    survival: bool,
    dig: &mut DigState,
    inventory: &mut Inventory,
    reach: f32,
) {
    let Some(target) = select_dig(world, player, body, reach) else {
        dig.clear_progress();
        dig.interaction = Some(interaction(
            player.input.aim,
            miss_reason(body, player, reach),
            0.0,
        ));
        return;
    };
    let plan = match classify_dig(
        world,
        reg,
        inventory,
        player.selected_slot,
        survival,
        target,
    ) {
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
    let duration = if survival { hardness / plan.speed } else { 0.0 };
    let fraction = if duration <= 0.0 {
        1.0
    } else {
        (progress.elapsed / duration).clamp(0.0, 1.0)
    };
    if fraction < 1.0 {
        dig.interaction = Some(dig_interaction(target, fraction, plan.material));
        return;
    }

    if survival {
        let leftover = inventory
            .inner
            .insert_first_fit(ItemStack::new(plan.item, 1), reg);
        debug_assert!(leftover.is_none());
        inventory.dirty = true;
    }
    world.place_material(target, MaterialId::AIR);
    for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
        bodies.candidates.push(target.translated(dx, dy));
    }
    dig.clear_progress();
    dig.interaction = Some(dig_interaction(target, 1.0, plan.material));
}

#[allow(clippy::too_many_arguments)]
fn active_place(
    world: &mut World,
    reg: &ItemRegistry,
    player: &Player,
    body: &Actor,
    survival: bool,
    dig: &mut DigState,
    inventory: &mut Inventory,
    reach: f32,
) {
    dig.clear_progress();
    let slot = player.selected_slot as usize;
    let Some(material) = inventory
        .inner
        .get(slot)
        .and_then(|stack| reg.try_get(stack.item).and_then(|def| def.place))
    else {
        dig.interaction = Some(interaction(
            player.input.aim,
            InteractionStatus::NotPlaceable,
            0.0,
        ));
        return;
    };
    let Some(target) = select_place(world, player, body, reach) else {
        dig.interaction = Some(interaction(
            player.input.aim,
            miss_reason(body, player, reach),
            0.0,
        ));
        return;
    };

    world.place_material(target, material);
    if survival {
        let stack = inventory.inner.get(slot).expect("placeable slot occupied");
        let count = stack.count.saturating_sub(1);
        inventory.inner.set(
            slot,
            (count > 0).then_some(ItemStack::new(stack.item, count)),
        );
        inventory.dirty = true;
    }
    dig.interaction = Some(interaction(target, InteractionStatus::Valid, 1.0));
}

fn idle_preview(
    world: &World,
    reg: &ItemRegistry,
    player: &Player,
    body: &Actor,
    survival: bool,
    inventory: &Inventory,
    reach: f32,
) -> Option<InteractionState> {
    let slot = player.selected_slot as usize;
    let placeable = inventory
        .inner
        .get(slot)
        .and_then(|stack| reg.try_get(stack.item).and_then(|def| def.place))
        .is_some();

    if placeable {
        let target = select_place(world, player, body, reach)?;
        return Some(interaction(target, InteractionStatus::Valid, 0.0));
    }
    if survival && inventory.inner.get(slot).is_some() && !is_tool(reg, inventory, slot) {
        return None;
    }
    let target = select_dig(world, player, body, reach)?;
    match classify_dig(
        world,
        reg,
        inventory,
        player.selected_slot,
        survival,
        target,
    ) {
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
    reg: &ItemRegistry,
    inventory: &Inventory,
    selected_slot: u8,
    survival: bool,
    target: CellPos,
) -> Result<DigPlan, InteractionStatus> {
    let Some(cell) = world.get_cell(target) else {
        return Err(InteractionStatus::OutOfReach);
    };
    let material = cell.material;
    if content::tags(material).contains(Tag::Player)
        || !matches!(content::phase(material), Phase::Solid | Phase::Powder)
    {
        return Err(InteractionStatus::Occupied);
    }
    let item = reg.item_for_material(material);
    if item == ItemId::NONE {
        return Err(InteractionStatus::Undiggable);
    }
    let slot = selected_slot as usize;
    let held = inventory.inner.get(slot);
    let (method, speed, tier) = match held.and_then(|stack| {
        reg.try_get(stack.item)
            .and_then(|def| def.tool.map(|t| (stack.item, t)))
    }) {
        Some((id, tool)) => (MiningMethod::Tool(id), tool.speed, tool.tier),
        None => {
            if survival && held.is_some() {
                return Err(InteractionStatus::WrongTool);
            }
            (MiningMethod::Hands, BARE_HAND_SPEED, 0)
        }
    };
    if survival {
        if tier < content::material(material).mining_tier {
            return Err(InteractionStatus::WrongTool);
        }
        if !inventory.inner.can_insert(ItemStack::new(item, 1), reg) {
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

fn is_tool(reg: &ItemRegistry, inventory: &Inventory, slot: usize) -> bool {
    inventory
        .inner
        .get(slot)
        .and_then(|stack| reg.try_get(stack.item))
        .is_some_and(|def| def.tool.is_some())
}

fn select_dig(world: &World, player: &Player, body: &Actor, reach: f32) -> Option<CellPos> {
    let aim = player.input.aim;
    match player.input.cursor_mode {
        CursorMode::Precise => {
            (diggable(world, aim) && cell_distance_sq(body, aim) <= reach * reach).then_some(aim)
        }
        CursorMode::Smart => smart_dig_target(world, body, aim, reach),
    }
}

fn smart_dig_target(world: &World, body: &Actor, aim: CellPos, reach: f32) -> Option<CellPos> {
    let footprint = body.footprint();
    let aim_offset_x = aim.x as f32 + 0.5 - body.x.to_f32();
    let aim_offset_y = aim.y as f32 + 0.5 - body.y.to_f32();
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
                .filter(|&pos| diggable(world, pos) && in_reach(pos))
                .min_by_key(|pos| (pos.y - aim.y).abs())
        } else {
            let y = if positive_direction {
                footprint.y1 + distance
            } else {
                footprint.y0 - distance
            };
            (footprint.x0..=footprint.x1)
                .map(|x| CellPos::new(x, y))
                .filter(|&pos| diggable(world, pos) && in_reach(pos))
                .min_by_key(|pos| (pos.x - aim.x).abs())
        };
        if let Some(target) = nearest_target {
            return Some(target);
        }
    }
    None
}

fn select_place(world: &World, player: &Player, body: &Actor, reach: f32) -> Option<CellPos> {
    let aim = player.input.aim;
    let target = match player.input.cursor_mode {
        CursorMode::Precise => world
            .get_cell(aim)
            .filter(|cell| cell.is_air())
            .map(|_| aim)?,
        CursorMode::Smart => {
            let start = body.cell();
            let end = clamp_to_reach(start, aim, reach);
            last_air_before_obstruction(world, start, end)?
        }
    };
    (cell_distance_sq(body, target) <= reach * reach).then_some(target)
}

fn miss_reason(body: &Actor, player: &Player, reach: f32) -> InteractionStatus {
    if cell_distance_sq(body, player.input.aim) <= reach * reach {
        InteractionStatus::NoTarget
    } else {
        InteractionStatus::OutOfReach
    }
}

fn clamp_to_reach(start: CellPos, aim: CellPos, reach: f32) -> CellPos {
    let dx = (aim.x as i64 - start.x as i64) as f64;
    let dy = (aim.y as i64 - start.y as i64) as f64;
    let dist = (dx * dx + dy * dy).sqrt();
    let max = reach as f64 + 1.0;
    if dist <= max || dist == 0.0 {
        return aim;
    }
    let scale = max / dist;
    CellPos::new(
        start.x + (dx * scale).round() as i32,
        start.y + (dy * scale).round() as i32,
    )
}

fn ray_cells(start: CellPos, end: CellPos) -> impl Iterator<Item = CellPos> {
    let ex = end.x as i64;
    let ey = end.y as i64;
    let dx = (ex - start.x as i64).abs();
    let dy = -(ey - start.y as i64).abs();
    let sx: i64 = if start.x < end.x { 1 } else { -1 };
    let sy: i64 = if start.y < end.y { 1 } else { -1 };
    let mut x = start.x as i64;
    let mut y = start.y as i64;
    let mut error = dx + dy;
    std::iter::from_fn(move || {
        if x == ex && y == ey {
            return None;
        }
        let twice = 2 * error;
        if twice >= dy {
            error += dy;
            x += sx;
        }
        if twice <= dx {
            error += dx;
            y += sy;
        }
        Some(CellPos::new(x as i32, y as i32))
    })
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

fn diggable(world: &World, pos: CellPos) -> bool {
    world.get_cell(pos).is_some_and(|cell| {
        !content::tags(cell.material).contains(Tag::Player)
            && matches!(content::phase(cell.material), Phase::Solid | Phase::Powder)
    })
}

fn cell_distance_sq(body: &Actor, pos: CellPos) -> f32 {
    let dx = pos.x as f32 + 0.5 - body.x.to_f32();
    let dy = pos.y as f32 + 0.5 - body.y.to_f32();
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
