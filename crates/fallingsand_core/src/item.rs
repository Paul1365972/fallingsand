use crate::{MaterialId, content};
use serde::{Deserialize, Serialize};
use std::ops::Range;

pub const HOTBAR_SLOTS: usize = 10;
pub const MAIN_SLOTS: usize = 20;
pub const PLAYER_SLOTS: usize = HOTBAR_SLOTS + MAIN_SLOTS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ItemId(pub u16);

impl ItemId {
    pub const NONE: Self = Self(0);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStack {
    pub item: ItemId,
    pub count: u32,
}

impl ItemStack {
    pub fn new(item: ItemId, count: u32) -> Self {
        Self { item, count }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ItemInfo {
    pub name: &'static str,
    pub display: &'static str,
    pub stack_max: u32,
    pub sprite: &'static str,
    pub place: Option<MaterialId>,
    pub tool: Option<ToolSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolSpec {
    pub tier: u8,
    pub speed: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Recipe {
    pub inputs: &'static [(ItemId, u32)],
    pub output: (ItemId, u32),
}

impl Recipe {
    pub fn can_craft(&self, inventory: &Inventory) -> bool {
        self.inputs
            .iter()
            .all(|&(item, count)| inventory.count_item(item) >= count as u64)
    }
}

#[inline]
fn stack_limit(item: ItemId) -> u32 {
    content::item(item).stack_max
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Inventory {
    pub slots: Vec<Option<ItemStack>>,
}

impl Inventory {
    pub fn player() -> Self {
        Self {
            slots: vec![None; PLAYER_SLOTS],
        }
    }

    pub fn with_capacity(n: usize) -> Self {
        Self {
            slots: vec![None; n],
        }
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    pub fn has_no_items(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    #[inline]
    pub fn get(&self, index: usize) -> Option<ItemStack> {
        self.slots.get(index).copied().flatten()
    }

    pub fn set(&mut self, index: usize, stack: Option<ItemStack>) {
        if let Some(slot) = self.slots.get_mut(index) {
            *slot = stack.filter(|s| s.count > 0);
        }
    }

    pub fn take(&mut self, index: usize) -> Option<ItemStack> {
        self.slots.get_mut(index).and_then(Option::take)
    }

    pub fn insert_into_range(
        &mut self,
        mut stack: ItemStack,
        range: Range<usize>,
    ) -> Option<ItemStack> {
        if stack.item == ItemId::NONE || stack.count == 0 {
            return None;
        }
        let max = stack_limit(stack.item);
        for slot in &mut self.slots[range.clone()] {
            if let Some(existing) = slot
                && existing.item == stack.item
                && existing.count < max
            {
                let moved = (max - existing.count).min(stack.count);
                existing.count += moved;
                stack.count -= moved;
                if stack.count == 0 {
                    return None;
                }
            }
        }
        for slot in &mut self.slots[range] {
            if slot.is_none() {
                let moved = max.min(stack.count);
                *slot = Some(ItemStack::new(stack.item, moved));
                stack.count -= moved;
                if stack.count == 0 {
                    return None;
                }
            }
        }
        Some(stack)
    }

    pub fn insert_first_fit(&mut self, stack: ItemStack) -> Option<ItemStack> {
        let range = 0..self.slots.len();
        self.insert_into_range(stack, range)
    }

    pub fn can_insert(&self, stack: ItemStack) -> bool {
        if stack.item == ItemId::NONE || stack.count == 0 {
            return true;
        }
        let max = stack_limit(stack.item);
        let mut remaining = stack.count;
        for slot in &self.slots {
            let capacity = match slot {
                Some(existing) if existing.item == stack.item => max.saturating_sub(existing.count),
                None => max,
                Some(_) => 0,
            };
            remaining = remaining.saturating_sub(capacity);
            if remaining == 0 {
                return true;
            }
        }
        false
    }

    fn count_item(&self, item: ItemId) -> u64 {
        self.slots
            .iter()
            .filter_map(|slot| slot.as_ref())
            .filter(|s| s.item == item)
            .map(|s| s.count as u64)
            .sum()
    }

    pub fn remove_item(&mut self, item: ItemId, mut count: u32) -> bool {
        if count == 0 {
            return true;
        }
        for slot in self.slots.iter_mut() {
            if let Some(existing) = slot
                && existing.item == item
            {
                let taken = existing.count.min(count);
                existing.count -= taken;
                count -= taken;
                if existing.count == 0 {
                    *slot = None;
                }
                if count == 0 {
                    return true;
                }
            }
        }
        false
    }

    pub fn left_click(&mut self, index: usize, cursor: &mut Option<ItemStack>) {
        let Some(slot) = self.slots.get_mut(index) else {
            return;
        };
        match (slot.as_mut(), cursor.as_mut()) {
            (None, Some(_)) => {
                *slot = cursor.take();
            }
            (Some(_), None) => {
                *cursor = slot.take();
            }
            (Some(s), Some(c)) if s.item == c.item => {
                let max = stack_limit(s.item);
                let space = max.saturating_sub(s.count);
                let moved = space.min(c.count);
                s.count += moved;
                c.count -= moved;
                if c.count == 0 {
                    *cursor = None;
                }
            }
            (Some(_), Some(_)) => std::mem::swap(slot, cursor),
            (None, None) => {}
        }
    }

    pub fn right_click(&mut self, index: usize, cursor: &mut Option<ItemStack>) {
        let Some(slot) = self.slots.get_mut(index) else {
            return;
        };
        match (slot.as_mut(), cursor.as_mut()) {
            (Some(s), None) => {
                let take = s.count.div_ceil(2);
                let keep = s.count - take;
                *cursor = Some(ItemStack::new(s.item, take));
                if keep == 0 {
                    *slot = None;
                } else {
                    s.count = keep;
                }
            }
            (None, Some(c)) => {
                *slot = Some(ItemStack::new(c.item, 1));
                c.count -= 1;
                if c.count == 0 {
                    *cursor = None;
                }
            }
            (Some(s), Some(c)) if s.item == c.item && s.count < stack_limit(s.item) => {
                s.count += 1;
                c.count -= 1;
                if c.count == 0 {
                    *cursor = None;
                }
            }
            _ => {}
        }
    }
}
