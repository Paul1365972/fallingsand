use crate::{MaterialId, Tag, content};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::Range;

pub const HOTBAR_SLOTS: usize = 9;
pub const MAIN_SLOTS: usize = 27;
pub const PLAYER_SLOTS: usize = HOTBAR_SLOTS + MAIN_SLOTS;
const MATERIAL_STACK_MAX: u32 = 10_000;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconSpec {
    MaterialSwatch(MaterialId),
    Atlas(u16),
}

#[derive(Debug, Clone)]
pub struct ItemDef {
    pub name: String,
    pub display: String,
    pub stack_max: u32,
    pub icon: IconSpec,
    pub place: Option<MaterialId>,
}

#[derive(Debug, Clone, Copy)]
pub struct ItemEntry {
    pub name: &'static str,
    pub display: &'static str,
    pub stack_max: u32,
    pub icon: IconSpec,
}

#[derive(Debug, Clone)]
pub struct ItemRegistry {
    items: Vec<ItemDef>,
    by_name: HashMap<String, ItemId>,
    mat_to_item: Vec<ItemId>,
}

impl ItemRegistry {
    pub fn build(entries: &[ItemEntry]) -> Self {
        let material_items = content::materials()
            .filter(|&(id, _)| is_material_item(id))
            .count();
        let total = 1 + entries.len() + material_items;
        assert!(total <= u16::MAX as usize, "too many items: {total}");

        let mut items: Vec<ItemDef> = Vec::new();
        let mut by_name: HashMap<String, ItemId> = HashMap::new();

        items.push(ItemDef {
            name: "none".into(),
            display: "None".into(),
            stack_max: 0,
            icon: IconSpec::MaterialSwatch(MaterialId::AIR),
            place: None,
        });

        for entry in entries {
            let def = ItemDef {
                name: entry.name.to_ascii_lowercase(),
                display: entry.display.into(),
                stack_max: entry.stack_max.max(1),
                icon: entry.icon,
                place: None,
            };
            let id = ItemId(items.len() as u16);
            assert!(
                by_name.insert(def.name.clone(), id).is_none(),
                "duplicate item name {:?}",
                def.name
            );
            items.push(def);
        }

        let mut mat_to_item = vec![ItemId::NONE; content::MATERIAL_COUNT];
        for (id, info) in content::materials() {
            if !is_material_item(id) {
                continue;
            }
            let name = format!("mat:{}", info.name);
            let def = ItemDef {
                display: pretty_name(info.name),
                stack_max: MATERIAL_STACK_MAX,
                icon: IconSpec::MaterialSwatch(id),
                place: Some(id),
                name: name.clone(),
            };
            let item_id = ItemId(items.len() as u16);
            assert!(
                by_name.insert(name.clone(), item_id).is_none(),
                "duplicate item name {name:?}"
            );
            mat_to_item[id.0 as usize] = item_id;
            items.push(def);
        }

        Self {
            items,
            by_name,
            mat_to_item,
        }
    }

    #[inline]
    pub fn get(&self, id: ItemId) -> &ItemDef {
        &self.items[id.0 as usize]
    }

    #[inline]
    pub fn try_get(&self, id: ItemId) -> Option<&ItemDef> {
        self.items.get(id.0 as usize)
    }

    pub fn id_of(&self, name: &str) -> Option<ItemId> {
        self.by_name.get(name).copied()
    }

    #[inline]
    pub fn item_for_material(&self, material: MaterialId) -> ItemId {
        self.mat_to_item
            .get(material.0 as usize)
            .copied()
            .unwrap_or(ItemId::NONE)
    }

    pub fn iter(&self) -> impl Iterator<Item = (ItemId, &ItemDef)> {
        self.items
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, def)| (ItemId(i as u16), def))
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.len() <= 1
    }

    #[inline]
    pub fn stack_max(&self, item: ItemId) -> u32 {
        self.try_get(item).map(|def| def.stack_max).unwrap_or(1)
    }
}

fn is_material_item(id: MaterialId) -> bool {
    content::phase(id) != crate::Phase::Empty
        && !content::tags(id).contains(Tag::Player)
        && !content::is_fuel_ember(id)
}

fn pretty_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for (i, word) in raw.split('_').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
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

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
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
        reg: &ItemRegistry,
    ) -> Option<ItemStack> {
        if stack.item == ItemId::NONE || stack.count == 0 {
            return None;
        }
        let max = reg.stack_max(stack.item);
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

    pub fn insert_first_fit(&mut self, stack: ItemStack, reg: &ItemRegistry) -> Option<ItemStack> {
        let range = 0..self.slots.len();
        self.insert_into_range(stack, range, reg)
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

    pub fn left_click(&mut self, index: usize, cursor: &mut Option<ItemStack>, reg: &ItemRegistry) {
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
                let max = reg.stack_max(s.item);
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

    pub fn right_click(
        &mut self,
        index: usize,
        cursor: &mut Option<ItemStack>,
        reg: &ItemRegistry,
    ) {
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
            (Some(s), Some(c)) if s.item == c.item && s.count < reg.stack_max(s.item) => {
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

#[derive(Debug, Clone)]
pub struct Recipe {
    pub inputs: Vec<(ItemId, u32)>,
    pub output: (ItemId, u32),
}

#[derive(Debug, Clone, Default)]
pub struct RecipeRegistry {
    recipes: Vec<Recipe>,
}

impl RecipeRegistry {
    pub fn new(recipes: Vec<Recipe>) -> Self {
        Self { recipes }
    }

    pub fn recipes(&self) -> &[Recipe] {
        &self.recipes
    }

    pub fn get(&self, index: usize) -> Option<&Recipe> {
        self.recipes.get(index)
    }

    pub fn can_craft(&self, recipe: &Recipe, inv: &Inventory) -> bool {
        recipe
            .inputs
            .iter()
            .all(|&(item, count)| inv.count_item(item) >= count as u64)
    }
}
