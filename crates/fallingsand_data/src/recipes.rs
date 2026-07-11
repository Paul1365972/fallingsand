use crate::items::item;
use crate::material;
use fallingsand_core::{ItemId, ItemRegistry, MaterialId};

pub(crate) enum Ingredient {
    Material(MaterialId),
    Item(ItemId),
}

impl From<MaterialId> for Ingredient {
    fn from(id: MaterialId) -> Self {
        Ingredient::Material(id)
    }
}

impl From<ItemId> for Ingredient {
    fn from(id: ItemId) -> Self {
        Ingredient::Item(id)
    }
}

pub(crate) fn resolve(ingredient: impl Into<Ingredient>, registry: &ItemRegistry) -> ItemId {
    match ingredient.into() {
        Ingredient::Material(id) => registry.item_for_material(id),
        Ingredient::Item(id) => id,
    }
}

recipes! {
    1 material::WOOD => 4 material::PLANKS;
    2 material::PLANKS => 4 item::STICK;
    1 material::IRON_ORE, 1 material::COAL => 1 item::IRON_INGOT;
    1 material::GOLD_ORE, 1 material::COAL => 1 item::GOLD_INGOT;
    3 material::PLANKS, 2 item::STICK => 1 item::WOODEN_PICKAXE;
    2 item::STICK, 3 item::IRON_INGOT => 1 item::STONE_PICKAXE;
    4 material::SAND => 1 material::SANDSTONE;
    2 material::GRAVEL, 1 material::WATER => 1 material::CLAY;
}
