use super::Changes;
use fallingsand_core::{BRUSH_RADIUS, Inventory as CoreInventory, ItemId, ItemStack};
use fallingsand_protocol::{SlotAction, TickFrame};

#[derive(Clone, Copy, PartialEq)]
pub enum SlotRegion {
    Player(usize),
    Trash,
    Craft(u16),
    Palette(ItemId),
}

pub struct Inventory {
    store: CoreInventory,
    pub cursor: Option<ItemStack>,
    pub trash: Option<ItemStack>,
    pub selected: usize,
    pub brush: u8,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            store: CoreInventory { slots: Vec::new() },
            cursor: None,
            trash: None,
            selected: 0,
            brush: BRUSH_RADIUS,
        }
    }
}

impl Inventory {
    pub fn slot(&self, index: usize) -> Option<ItemStack> {
        self.store.slots.get(index).copied().flatten()
    }

    pub fn store(&self) -> &CoreInventory {
        &self.store
    }

    pub(super) fn apply(&mut self, tick: &TickFrame, changes: &mut Changes) {
        for &(index, stack) in &tick.inventory {
            let index = index as usize;
            if index >= self.store.slots.len() {
                self.store.slots.resize(index + 1, None);
            }
            self.store.slots[index] = stack;
            changes.slots.push(index);
        }
        if let Some(cursor) = tick.cursor {
            self.cursor = cursor;
        }
        if let Some(trash) = tick.trash {
            self.trash = trash;
            changes.trash = true;
        }
    }

    pub(super) fn reset(&mut self, changes: &mut Changes) {
        changes.slots.extend(0..self.store.slots.len());
        changes.trash = true;
        self.store.slots.clear();
        self.cursor = None;
        self.trash = None;
    }
}

pub(super) fn slot_action(region: SlotRegion, right: bool, shift: bool) -> Option<SlotAction> {
    match region {
        SlotRegion::Player(index) => {
            let slot = index as u16;
            Some(if shift && !right {
                SlotAction::QuickMove { slot }
            } else if right {
                SlotAction::RightClick { slot }
            } else {
                SlotAction::LeftClick { slot }
            })
        }
        SlotRegion::Trash => (!right).then_some(SlotAction::Trash),
        SlotRegion::Craft(recipe) => (!right).then_some(SlotAction::Craft { recipe, all: shift }),
        SlotRegion::Palette(item) => (!right).then_some(SlotAction::CreativeGrab { item }),
    }
}
