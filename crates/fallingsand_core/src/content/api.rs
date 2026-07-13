use super::{ITEM_COUNT, MATERIAL_COUNT};
use crate::item::{ItemId, ItemInfo, Recipe};
use crate::material::{MaterialId, MaterialInfo};

pub fn materials() -> impl Iterator<Item = (MaterialId, &'static MaterialInfo)> {
    (0..MATERIAL_COUNT).map(|index| {
        let id = MaterialId(index as u16);
        (id, super::material(id))
    })
}

pub fn items() -> impl Iterator<Item = (ItemId, &'static ItemInfo)> {
    (1..ITEM_COUNT).map(|index| {
        let id = ItemId(index as u16);
        (id, super::item(id))
    })
}

pub fn try_item(id: ItemId) -> Option<&'static ItemInfo> {
    ((id.0 as usize) < ITEM_COUNT).then(|| super::item(id))
}

pub fn recipes() -> &'static [Recipe] {
    super::RECIPES
}
