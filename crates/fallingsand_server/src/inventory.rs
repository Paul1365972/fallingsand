use crate::player::Players;
use fallingsand_core::{
    Inventory as CoreInventory, ItemId, ItemRegistry, ItemStack, RecipeRegistry,
};
use fallingsand_protocol::{GameMode, SlotAction};

pub struct Inventory {
    pub inner: CoreInventory,
    pub cursor: Option<ItemStack>,
    pub trash: Option<ItemStack>,
}

impl Inventory {
    pub fn with(inner: CoreInventory, cursor: Option<ItemStack>, trash: Option<ItemStack>) -> Self {
        Self {
            inner,
            cursor,
            trash,
        }
    }
}

impl Default for Inventory {
    fn default() -> Self {
        Self::with(CoreInventory::player(), None, None)
    }
}

pub fn apply_slot_actions(players: &mut Players, reg: &ItemRegistry, recipes: &RecipeRegistry) {
    for (_, player) in players.iter_mut() {
        let actions = std::mem::take(&mut player.control.pending_slot_actions);
        if !player.is_alive() || actions.is_empty() {
            continue;
        }
        let creative = player.profile.mode == GameMode::Creative;
        for action in actions {
            apply_action(
                action,
                creative,
                &mut player.profile.inventory,
                reg,
                recipes,
            );
        }
    }
}

fn apply_action(
    action: SlotAction,
    creative: bool,
    inventory: &mut Inventory,
    reg: &ItemRegistry,
    recipes: &RecipeRegistry,
) {
    match action {
        SlotAction::LeftClick { slot } => {
            inventory
                .inner
                .left_click(slot as usize, &mut inventory.cursor, reg);
        }
        SlotAction::RightClick { slot } => {
            inventory
                .inner
                .right_click(slot as usize, &mut inventory.cursor, reg);
        }
        SlotAction::QuickMove { slot } => {
            let slot = slot as usize;
            if let Some(stack) = inventory.inner.take(slot) {
                let dst = if slot < fallingsand_core::HOTBAR_SLOTS {
                    fallingsand_core::HOTBAR_SLOTS..fallingsand_core::PLAYER_SLOTS
                } else {
                    0..fallingsand_core::HOTBAR_SLOTS
                };
                let leftover = inventory.inner.insert_into_range(stack, dst, reg);
                inventory.inner.set(slot, leftover);
            }
        }
        SlotAction::Trash => {
            if inventory.cursor.is_some() {
                inventory.trash = inventory.cursor.take();
            } else if inventory.trash.is_some() {
                inventory.cursor = inventory.trash.take();
            }
        }
        SlotAction::Craft { recipe, all } => {
            let Some(recipe) = recipes.get(recipe as usize).cloned() else {
                return;
            };
            loop {
                if !recipes.can_craft(&recipe, &inventory.inner) {
                    break;
                }
                let mut trial = inventory.inner.clone();
                for &(item, count) in &recipe.inputs {
                    trial.remove_item(item, count);
                }
                let output = ItemStack::new(recipe.output.0, recipe.output.1);
                if trial.insert_first_fit(output, reg).is_some() {
                    break;
                }
                inventory.inner = trial;
                if !all {
                    break;
                }
            }
        }
        SlotAction::CreativeGrab { item } => {
            if creative
                && item != ItemId::NONE
                && let Some(def) = reg.try_get(item)
            {
                inventory.cursor = Some(ItemStack::new(item, def.stack_max));
            }
        }
    }
}
