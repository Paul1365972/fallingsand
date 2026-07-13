use crate::player::{Life, Mode, Player};
use bevy_ecs::prelude::*;
use fallingsand_core::{
    Inventory as CoreInventory, ItemId, ItemRegistry, ItemStack, RecipeRegistry,
};
use fallingsand_protocol::{GameMode, LifeState, SlotAction};
use std::sync::Arc;

type InventoryDiff = (
    Vec<(u16, Option<ItemStack>)>,
    Option<Option<ItemStack>>,
    Option<Option<ItemStack>>,
);

#[derive(Resource, Clone)]
pub struct ItemReg(pub Arc<ItemRegistry>);

#[derive(Resource, Clone)]
pub struct Recipes(pub Arc<RecipeRegistry>);

pub struct QueuedSlotAction {
    pub entity: Entity,
    pub generation: u64,
    pub action: SlotAction,
}

#[derive(Resource, Default)]
pub struct SlotActions(pub Vec<QueuedSlotAction>);

#[derive(Component)]
pub struct Inventory {
    pub inner: CoreInventory,
    pub cursor: Option<ItemStack>,
    pub trash: Option<ItemStack>,
    pub dirty: bool,
    last_slots: Vec<Option<ItemStack>>,
    last_cursor: Option<ItemStack>,
    last_trash: Option<ItemStack>,
}

impl Inventory {
    pub fn new(inner: CoreInventory) -> Self {
        Self::with(inner, None, None)
    }

    pub fn with(inner: CoreInventory, cursor: Option<ItemStack>, trash: Option<ItemStack>) -> Self {
        Self {
            inner,
            cursor,
            trash,
            dirty: true,
            last_slots: Vec::new(),
            last_cursor: None,
            last_trash: None,
        }
    }

    pub fn delta(&mut self, fresh: bool) -> InventoryDiff {
        if fresh {
            self.dirty = false;
            self.last_slots = self.inner.slots.clone();
            self.last_cursor = self.cursor;
            self.last_trash = self.trash;
            let slots = self
                .inner
                .slots
                .iter()
                .enumerate()
                .map(|(i, stack)| (i as u16, *stack))
                .collect();
            return (slots, Some(self.cursor), Some(self.trash));
        }
        if !self.dirty {
            return (Vec::new(), None, None);
        }
        self.dirty = false;
        let changes: Vec<(u16, Option<ItemStack>)> = self
            .inner
            .slots
            .iter()
            .zip(self.last_slots.iter())
            .enumerate()
            .filter_map(|(i, (cur, last))| (cur != last).then_some((i as u16, *cur)))
            .collect();
        let cursor_changed = self.cursor != self.last_cursor;
        let trash_changed = self.trash != self.last_trash;
        if changes.is_empty() && !cursor_changed && !trash_changed {
            return (Vec::new(), None, None);
        }
        self.last_slots = self.inner.slots.clone();
        self.last_cursor = self.cursor;
        self.last_trash = self.trash;
        let cursor = cursor_changed.then_some(self.cursor);
        let trash = trash_changed.then_some(self.trash);
        (changes, cursor, trash)
    }
}

impl Default for Inventory {
    fn default() -> Self {
        Self::new(CoreInventory::player())
    }
}

pub fn apply_slot_actions(
    mut actions: ResMut<SlotActions>,
    item_reg: Res<ItemReg>,
    recipes: Res<Recipes>,
    mut players: Query<(&Player, &Life, &Mode, &mut Inventory)>,
) {
    let reg = &item_reg.0;
    for queued in actions.0.drain(..) {
        let Ok((player, life, mode, mut pinv)) = players.get_mut(queued.entity) else {
            continue;
        };
        if player.session_generation != queued.generation || life.0 != LifeState::Alive {
            continue;
        }
        let action = queued.action;
        let creative = mode.0 == GameMode::Creative;
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
            SlotAction::Trash => {
                if pinv.cursor.is_some() {
                    pinv.trash = pinv.cursor.take();
                    pinv.dirty = true;
                } else if pinv.trash.is_some() {
                    pinv.cursor = pinv.trash.take();
                    pinv.dirty = true;
                }
            }
            SlotAction::Craft { recipe, all } => {
                let Some(recipe) = recipes.0.get(recipe as usize).cloned() else {
                    continue;
                };
                let mut crafted = false;
                loop {
                    if !recipes.0.can_craft(&recipe, &pinv.inner) {
                        break;
                    }
                    let mut trial = pinv.inner.clone();
                    for &(item, count) in &recipe.inputs {
                        trial.remove_item(item, count);
                    }
                    let output = ItemStack::new(recipe.output.0, recipe.output.1);
                    if trial.insert_first_fit(output, reg).is_some() {
                        break;
                    }
                    pinv.inner = trial;
                    crafted = true;
                    if !all {
                        break;
                    }
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
