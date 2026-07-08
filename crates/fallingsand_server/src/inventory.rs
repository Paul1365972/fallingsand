use crate::persistence::{DroppedRecord, RegionExtras};
use crate::session::{Player, SessionState, Sessions};
use crate::systems::{Mode, PhysicsBody};
use crate::{Registry, SimWorld};
use bevy_ecs::prelude::*;
use fallingsand_core::{
    CellPos, ChunkPos, Fixed, GRAVITY, Inventory as CoreInventory, ItemId, ItemRegistry, ItemStack,
    RecipeRegistry, RegionPos, TICK_DT,
};
use fallingsand_protocol::{EntityId, GameMode, ServerMessage, SlotAction, encode_message};
use fallingsand_sim::physics::{Body, body_submersion, move_body};
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub const ITEM_HALF: Fixed = Fixed::from_f32(1.0);
const ITEM_MAX_FALL: f32 = 160.0;
const ITEM_GROUND_FRICTION: f32 = 0.72;
const ITEM_AIR_FRICTION: f32 = 0.02;
const GRAB_RANGE_SQ: f32 = 34.0 * 34.0;
const PICKUP_RANGE_SQ: f32 = 9.0 * 9.0;
const MAGNET_ACCEL: f32 = 620.0;
const MERGE_RADIUS_SQ: f32 = 6.0 * 6.0;
const PER_CHUNK_ITEM_CAP: usize = 128;
pub const DROP_PICKUP_DELAY: u16 = 36;

#[derive(Resource, Clone)]
pub struct ItemReg(pub Arc<ItemRegistry>);

#[derive(Resource, Clone)]
pub struct Recipes(pub Arc<RecipeRegistry>);

#[derive(Resource, Default)]
pub struct NextEntityId(pub u64);

impl NextEntityId {
    pub fn alloc(&mut self) -> EntityId {
        self.0 += 1;
        EntityId(self.0)
    }
}

#[derive(Resource, Default)]
pub struct SlotActions(pub Vec<(Entity, SlotAction)>);

#[derive(Component)]
pub struct Inventory {
    pub inner: CoreInventory,
    pub cursor: Option<ItemStack>,
    pub dirty: bool,
}

impl Inventory {
    pub fn new(inner: CoreInventory) -> Self {
        Self {
            inner,
            cursor: None,
            dirty: true,
        }
    }
}

impl Default for Inventory {
    fn default() -> Self {
        Self::new(CoreInventory::player())
    }
}

#[derive(Component)]
pub struct DroppedItem {
    pub stack: ItemStack,
    pub id: EntityId,
    pub age_ticks: u64,
    pub pickup_delay: u16,
}

#[derive(Component)]
pub struct ItemBody(pub Body);

pub fn gather_region_extras(
    pos: RegionPos,
    reg: &ItemRegistry,
    items: &Query<(Entity, &DroppedItem, &ItemBody)>,
) -> (RegionExtras, Vec<Entity>) {
    let mut extras = RegionExtras::default();
    let mut entities = Vec::new();
    for (entity, dropped, body) in items.iter() {
        let cell = CellPos::new(body.0.x.floor_cell(), body.0.y.floor_cell());
        if cell.region() != pos {
            continue;
        }
        let Some(def) = reg.try_get(dropped.stack.item) else {
            continue;
        };
        extras.items.push(DroppedRecord {
            x: body.0.x,
            y: body.0.y,
            vx: body.0.vx.to_f32(),
            vy: body.0.vy.to_f32(),
            item: def.name.clone(),
            count: dropped.stack.count,
        });
        entities.push(entity);
    }
    (extras, entities)
}

pub fn region_has_entities(
    pos: RegionPos,
    items: &Query<(Entity, &DroppedItem, &ItemBody)>,
) -> bool {
    items
        .iter()
        .any(|(_, _, b)| CellPos::new(b.0.x.floor_cell(), b.0.y.floor_cell()).region() == pos)
}

pub fn spawn_region_extras(
    commands: &mut Commands,
    next_id: &mut NextEntityId,
    reg: &ItemRegistry,
    extras: &RegionExtras,
) {
    for record in &extras.items {
        if let Some(item) = reg.id_of(&record.item) {
            spawn_dropped_item(
                commands,
                next_id,
                ItemStack::new(item, record.count),
                record.x,
                record.y,
                record.vx,
                record.vy,
                0,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_dropped_item(
    commands: &mut Commands,
    next_id: &mut NextEntityId,
    stack: ItemStack,
    x: Fixed,
    y: Fixed,
    vx: f32,
    vy: f32,
    pickup_delay: u16,
) {
    let id = next_id.alloc();
    let mut body = Body::new(x, y, ITEM_HALF, ITEM_HALF);
    body.vx = Fixed::from_f32(vx);
    body.vy = Fixed::from_f32(vy);
    commands.spawn((
        DroppedItem {
            stack,
            id,
            age_ticks: 0,
            pickup_delay,
        },
        ItemBody(body),
    ));
}

fn inventory_has_room(inv: &CoreInventory, item: ItemId, reg: &ItemRegistry) -> bool {
    let max = reg.stack_max(item);
    inv.slots.iter().any(|slot| match slot {
        None => true,
        Some(s) => s.item == item && s.count < max,
    })
}

fn take_amount(inv: &mut CoreInventory, slot: usize, all: bool) -> Option<ItemStack> {
    let stack = inv.get(slot)?;
    if all || stack.count <= 1 {
        inv.take(slot)
    } else {
        inv.set(slot, Some(ItemStack::new(stack.item, stack.count - 1)));
        Some(ItemStack::new(stack.item, 1))
    }
}

fn throw_item(
    commands: &mut Commands,
    next_id: &mut NextEntityId,
    stack: ItemStack,
    x: Fixed,
    y: Fixed,
    player_vx: Fixed,
) {
    let dir = if player_vx > Fixed::ZERO {
        1.0
    } else if player_vx < Fixed::ZERO {
        -1.0
    } else {
        (stack.item.0 % 2) as f32 * 2.0 - 1.0
    };
    spawn_dropped_item(
        commands,
        next_id,
        stack,
        x,
        y,
        dir * 48.0,
        70.0,
        DROP_PICKUP_DELAY,
    );
}

pub fn apply_slot_actions(
    mut commands: Commands,
    mut actions: ResMut<SlotActions>,
    item_reg: Res<ItemReg>,
    recipes: Res<Recipes>,
    mut next_id: ResMut<NextEntityId>,
    mut players: Query<(&PhysicsBody, &Mode, &mut Inventory), With<Player>>,
) {
    let reg = &item_reg.0;
    for (entity, action) in actions.0.drain(..) {
        let Ok((body, mode, mut pinv)) = players.get_mut(entity) else {
            continue;
        };
        let creative = mode.0 == GameMode::Creative;
        let (px, py, pvx) = (body.0.x, body.0.y, body.0.vx);
        match action {
            SlotAction::LeftClick { slot } => {
                let Inventory { inner, cursor, .. } = &mut *pinv;
                inner.left_click(slot as usize, cursor, reg);
                pinv.dirty = true;
            }
            SlotAction::RightClick { slot } => {
                let Inventory { inner, cursor, .. } = &mut *pinv;
                inner.right_click(slot as usize, cursor, reg);
                pinv.dirty = true;
            }
            SlotAction::QuickMove { slot } => {
                let slot = slot as usize;
                if let Some(stack) = pinv.inner.take(slot) {
                    let dst = if slot < fallingsand_core::HOTBAR_SLOTS {
                        fallingsand_core::HOTBAR_SLOTS..fallingsand_core::PLAYER_SLOTS
                    } else {
                        0..fallingsand_core::HOTBAR_SLOTS
                    };
                    let leftover = pinv.inner.insert_into_range(stack, dst, reg);
                    pinv.inner.set(slot, leftover);
                }
                pinv.dirty = true;
            }
            SlotAction::DropSlot { slot, all } => {
                let slot = slot as usize;
                if let Some(stack) = take_amount(&mut pinv.inner, slot, all) {
                    throw_item(&mut commands, &mut next_id, stack, px, py, pvx);
                }
                pinv.dirty = true;
            }
            SlotAction::DropCursor { all } => {
                if let Some(cursor) = pinv.cursor.as_mut() {
                    let count = if all { cursor.count } else { 1 };
                    let stack = ItemStack::new(cursor.item, count);
                    cursor.count -= count;
                    if cursor.count == 0 {
                        pinv.cursor = None;
                    }
                    throw_item(&mut commands, &mut next_id, stack, px, py, pvx);
                    pinv.dirty = true;
                }
            }
            SlotAction::Craft { recipe, times } => {
                let Some(recipe) = recipes.0.get(recipe as usize).cloned() else {
                    continue;
                };
                let mut crafted = false;
                for _ in 0..times.max(1) {
                    if !recipes.0.can_craft(&recipe, &pinv.inner) {
                        break;
                    }
                    for &(item, count) in &recipe.inputs {
                        pinv.inner.remove_item(item, count);
                    }
                    let output = ItemStack::new(recipe.output.0, recipe.output.1);
                    if let Some(overflow) = pinv.inner.insert_first_fit(output, reg) {
                        throw_item(&mut commands, &mut next_id, overflow, px, py, pvx);
                    }
                    crafted = true;
                }
                if crafted {
                    pinv.dirty = true;
                }
            }
            SlotAction::CreativeGrab { item } => {
                if creative
                    && item != ItemId::NONE
                    && let Some(def) = reg.try_get(item)
                {
                    pinv.cursor = Some(ItemStack::new(item, def.stack_max));
                    pinv.dirty = true;
                }
            }
        }
    }
}

pub fn step_items(
    mut commands: Commands,
    sim: Res<SimWorld>,
    registry: Res<Registry>,
    item_reg: Res<ItemReg>,
    mut items: Query<(Entity, &mut DroppedItem, &mut ItemBody)>,
    mut players: Query<(Entity, &PhysicsBody, &mut Inventory), With<Player>>,
) {
    let reg = &item_reg.0;

    let mut order: Vec<(Entity, EntityId)> = items
        .iter()
        .map(|(entity, dropped, _)| (entity, dropped.id))
        .collect();
    order.sort_unstable_by_key(|&(_, id)| id.0);

    let mut removed: Vec<Entity> = Vec::new();
    merge_items(&order, &mut items, reg, &mut removed);
    cap_items(&order, &items, &removed)
        .into_iter()
        .for_each(|e| removed.push(e));

    let player_positions: Vec<(Entity, f32, f32)> = players
        .iter()
        .map(|(e, body, _)| (e, body.0.x.to_f32(), body.0.y.to_f32()))
        .collect();

    let gravity_step = Fixed::from_f32(GRAVITY * TICK_DT);
    let max_fall = Fixed::from_f32(ITEM_MAX_FALL);
    for &(entity, _) in &order {
        if removed.contains(&entity) {
            continue;
        }
        let Ok((_, mut dropped, mut body)) = items.get_mut(entity) else {
            continue;
        };
        dropped.age_ticks += 1;
        dropped.pickup_delay = dropped.pickup_delay.saturating_sub(1);

        let ix = body.0.x.to_f32();
        let iy = body.0.y.to_f32();
        let mut nearest: Option<(Entity, f32, f32, f32)> = None;
        for &(pe, px, py) in &player_positions {
            let dx = px - ix;
            let dy = py - iy;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq > GRAB_RANGE_SQ {
                continue;
            }
            if dropped.pickup_delay == 0 {
                let room = players
                    .get(pe)
                    .map(|(_, _, pinv)| inventory_has_room(&pinv.inner, dropped.stack.item, reg))
                    .unwrap_or(false);
                if !room {
                    continue;
                }
            }
            if nearest.map(|(_, _, _, d)| dist_sq < d).unwrap_or(true) {
                nearest = Some((pe, dx, dy, dist_sq));
            }
        }

        if let Some((pe, dx, dy, dist_sq)) = nearest
            && dropped.pickup_delay == 0
        {
            if dist_sq <= PICKUP_RANGE_SQ {
                if let Ok((_, _, mut pinv)) = players.get_mut(pe) {
                    let leftover = pinv.inner.insert_first_fit(dropped.stack, reg);
                    pinv.dirty = true;
                    match leftover {
                        Some(rest) => dropped.stack = rest,
                        None => {
                            removed.push(entity);
                            continue;
                        }
                    }
                }
            } else {
                let dist = dist_sq.sqrt().max(0.001);
                let accel = MAGNET_ACCEL * TICK_DT;
                body.0.vx = body.0.vx.add_f32(dx / dist * accel);
                body.0.vy = body.0.vy.add_f32(dy / dist * accel);
            }
        }

        body.0.vy -= gravity_step;
        if body.0.vy < -max_fall {
            body.0.vy = -max_fall;
        }
        let submersion = body_submersion(&sim.0, &registry.0, &body.0).fraction;
        move_body(&sim.0, &registry.0, &mut body.0, submersion);
        let friction = if body.0.on_ground {
            ITEM_GROUND_FRICTION
        } else {
            1.0 - ITEM_AIR_FRICTION
        };
        body.0.vx = body.0.vx.mul(Fixed::from_f32(friction));
    }

    for entity in removed {
        commands.entity(entity).despawn();
    }
}

fn merge_items(
    order: &[(Entity, EntityId)],
    items: &mut Query<(Entity, &mut DroppedItem, &mut ItemBody)>,
    reg: &ItemRegistry,
    removed: &mut Vec<Entity>,
) {
    let mut buckets: FxHashMap<ChunkPos, Vec<Entity>> = FxHashMap::default();
    for &(entity, _) in order {
        if let Ok((_, _, body)) = items.get(entity) {
            let chunk = CellPos::new(body.0.x.floor_cell(), body.0.y.floor_cell()).chunk();
            buckets.entry(chunk).or_default().push(entity);
        }
    }
    for bucket in buckets.values() {
        for i in 0..bucket.len() {
            let a = bucket[i];
            if removed.contains(&a) {
                continue;
            }
            let Ok((_, a_drop, a_body)) = items.get(a) else {
                continue;
            };
            let (ax, ay) = (a_body.0.x.to_f32(), a_body.0.y.to_f32());
            let a_item = a_drop.stack.item;
            let max = reg.stack_max(a_item);
            let mut a_count = a_drop.stack.count;
            for &b in &bucket[i + 1..] {
                if a_count >= max || removed.contains(&b) {
                    continue;
                }
                let Ok((_, b_drop, b_body)) = items.get(b) else {
                    continue;
                };
                if b_drop.stack.item != a_item {
                    continue;
                }
                let dx = b_body.0.x.to_f32() - ax;
                let dy = b_body.0.y.to_f32() - ay;
                if dx * dx + dy * dy > MERGE_RADIUS_SQ {
                    continue;
                }
                let moved = (max - a_count).min(b_drop.stack.count);
                a_count += moved;
                if let Ok((_, mut b_drop, _)) = items.get_mut(b) {
                    b_drop.stack.count -= moved;
                    if b_drop.stack.count == 0 {
                        removed.push(b);
                    }
                }
            }
            if let Ok((_, mut a_drop, _)) = items.get_mut(a) {
                a_drop.stack.count = a_count;
            }
        }
    }
}

fn cap_items(
    order: &[(Entity, EntityId)],
    items: &Query<(Entity, &mut DroppedItem, &mut ItemBody)>,
    removed: &[Entity],
) -> Vec<Entity> {
    let mut buckets: FxHashMap<ChunkPos, Vec<Entity>> = FxHashMap::default();
    for &(entity, _) in order {
        if removed.contains(&entity) {
            continue;
        }
        if let Ok((_, _, body)) = items.get(entity) {
            let chunk = CellPos::new(body.0.x.floor_cell(), body.0.y.floor_cell()).chunk();
            buckets.entry(chunk).or_default().push(entity);
        }
    }
    let mut extra = Vec::new();
    for mut live in buckets.into_values() {
        if live.len() <= PER_CHUNK_ITEM_CAP {
            continue;
        }
        live.sort_unstable_by_key(|&e| items.get(e).map(|(_, d, _)| d.age_ticks).unwrap_or(0));
        for &e in live.iter().skip(PER_CHUNK_ITEM_CAP) {
            extra.push(e);
        }
    }
    extra
}

pub fn sync_inventories(mut sessions: ResMut<Sessions>, mut query: Query<&mut Inventory>) {
    for session in &mut sessions.sessions {
        if !matches!(session.state, SessionState::Playing) {
            continue;
        }
        let Some(entity) = session.entity else {
            continue;
        };
        let Ok(mut inventory) = query.get_mut(entity) else {
            continue;
        };
        if !inventory.dirty {
            continue;
        }
        inventory.dirty = false;
        session.conn.send(encode_message(&ServerMessage::Inventory {
            slots: inventory.inner.slots.clone(),
            cursor: inventory.cursor,
        }));
    }
}
