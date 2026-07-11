use super::MATERIAL_COUNT;
use crate::material::{MaterialId, MaterialInfo};

pub fn materials() -> impl Iterator<Item = (MaterialId, &'static MaterialInfo)> {
    (0..MATERIAL_COUNT).map(|index| {
        let id = MaterialId(index as u16);
        (id, super::material(id))
    })
}

pub fn item_registry() -> crate::item::ItemRegistry {
    crate::item::ItemRegistry::build(super::items::ENTRIES)
}

pub fn recipe_registry(items: &crate::item::ItemRegistry) -> crate::item::RecipeRegistry {
    super::recipes::recipes(items)
}
