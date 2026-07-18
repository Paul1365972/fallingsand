use super::materials::flora::{PLANKS, WOOD};
use super::materials::fluids::WATER;
use super::materials::ores::{COAL, GOLD_ORE, IRON_ORE};
use super::materials::terrain::{CLAY, GRAVEL, SAND, SANDSTONE, STONE};
use crate::{Catalog, ItemKey, item, recipe};

pub const STICK: ItemKey = ItemKey::new("STICK");
pub const IRON_INGOT: ItemKey = ItemKey::new("IRON_INGOT");
pub const GOLD_INGOT: ItemKey = ItemKey::new("GOLD_INGOT");
pub const WOODEN_PICKAXE: ItemKey = ItemKey::new("WOODEN_PICKAXE");
pub const STONE_PICKAXE: ItemKey = ItemKey::new("STONE_PICKAXE");
pub const IRON_PICKAXE: ItemKey = ItemKey::new("IRON_PICKAXE");

pub fn define(catalog: &mut Catalog) {
    catalog.add_item(STICK, item("Stick").stack(99));
    catalog.add_item(IRON_INGOT, item("Iron Ingot").stack(99));
    catalog.add_item(GOLD_INGOT, item("Gold Ingot").stack(99));
    catalog.add_item(WOODEN_PICKAXE, item("Wooden Pickaxe").tool(1, 2.0));
    catalog.add_item(STONE_PICKAXE, item("Stone Pickaxe").tool(2, 3.2));
    catalog.add_item(IRON_PICKAXE, item("Iron Pickaxe").tool(3, 4.8));

    catalog.craft(recipe().input(WOOD, 1).output(PLANKS, 4));
    catalog.craft(recipe().input(PLANKS, 2).output(STICK, 4));
    catalog.craft(
        recipe()
            .input(IRON_ORE, 1)
            .input(COAL, 1)
            .output(IRON_INGOT, 1),
    );
    catalog.craft(
        recipe()
            .input(GOLD_ORE, 1)
            .input(COAL, 1)
            .output(GOLD_INGOT, 1),
    );
    catalog.craft(
        recipe()
            .input(PLANKS, 3)
            .input(STICK, 2)
            .output(WOODEN_PICKAXE, 1),
    );
    catalog.craft(
        recipe()
            .input(STONE, 3)
            .input(STICK, 2)
            .output(STONE_PICKAXE, 1),
    );
    catalog.craft(
        recipe()
            .input(IRON_INGOT, 3)
            .input(STICK, 2)
            .output(IRON_PICKAXE, 1),
    );
    catalog.craft(recipe().input(SAND, 4).output(SANDSTONE, 1));
    catalog.craft(recipe().input(GRAVEL, 2).input(WATER, 1).output(CLAY, 1));
}
