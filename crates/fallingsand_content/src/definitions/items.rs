use super::materials::crafted::{
    BEAM, BRICK, GLASS, GUNPOWDER, IRON, LUMEN_LAMP, PLANKS, ROCKWOOL, ROPE, STEEL, TORCH,
};
use super::materials::fire::CHARCOAL;
use super::materials::flora::{FUNGWOOD, HEARTWOOD, KELP, REED, VINE, WOOD};
use super::materials::fluids::WATER;
use super::materials::ores::{
    AMBER, FLINT, GOLD, GYPSUM, IRON_ORE, LUMEN, NATIVE_COPPER, SALTPETER, SULFUR, TIN_ORE,
};
use super::materials::soil::{CLAY, GRAVEL, PEAT, SAND};
use super::materials::stone::{BASALT, LIMESTONE, OBSIDIAN, RUBBLE};
use crate::{Catalog, ItemKey, item, recipe};

pub const STICK: ItemKey = ItemKey::new("STICK");
pub const CORDAGE: ItemKey = ItemKey::new("CORDAGE");
pub const COPPER_INGOT: ItemKey = ItemKey::new("COPPER_INGOT");
pub const TIN_INGOT: ItemKey = ItemKey::new("TIN_INGOT");
pub const BRONZE_INGOT: ItemKey = ItemKey::new("BRONZE_INGOT");
pub const IRON_INGOT: ItemKey = ItemKey::new("IRON_INGOT");
pub const STEEL_INGOT: ItemKey = ItemKey::new("STEEL_INGOT");
pub const GOLD_INGOT: ItemKey = ItemKey::new("GOLD_INGOT");
pub const LUMEN_DUST: ItemKey = ItemKey::new("LUMEN_DUST");
pub const QUICKLIME: ItemKey = ItemKey::new("QUICKLIME");
pub const PLASTER: ItemKey = ItemKey::new("PLASTER");

pub const FLINT_PICK: ItemKey = ItemKey::new("FLINT_PICK");
pub const COPPER_PICK: ItemKey = ItemKey::new("COPPER_PICK");
pub const BRONZE_PICK: ItemKey = ItemKey::new("BRONZE_PICK");
pub const IRON_PICK: ItemKey = ItemKey::new("IRON_PICK");
pub const STEEL_PICK: ItemKey = ItemKey::new("STEEL_PICK");
pub const OBSIDIAN_BLADE: ItemKey = ItemKey::new("OBSIDIAN_BLADE");

pub fn define(catalog: &mut Catalog) {
    catalog.add_item(STICK, item("Stick").stack(99));
    catalog.add_item(CORDAGE, item("Cordage").stack(99));
    catalog.add_item(COPPER_INGOT, item("Copper Ingot").stack(99));
    catalog.add_item(TIN_INGOT, item("Tin Ingot").stack(99));
    catalog.add_item(BRONZE_INGOT, item("Bronze Ingot").stack(99));
    catalog.add_item(IRON_INGOT, item("Iron Ingot").stack(99));
    catalog.add_item(STEEL_INGOT, item("Steel Ingot").stack(99));
    catalog.add_item(GOLD_INGOT, item("Gold Ingot").stack(99));
    catalog.add_item(LUMEN_DUST, item("Lumen Dust").stack(99));
    catalog.add_item(QUICKLIME, item("Quicklime").stack(99));
    catalog.add_item(PLASTER, item("Plaster").stack(99));

    catalog.add_item(FLINT_PICK, item("Flint Pick").tool(1.8));
    catalog.add_item(COPPER_PICK, item("Copper Pick").tool(2.8));
    catalog.add_item(BRONZE_PICK, item("Bronze Pick").tool(3.6));
    catalog.add_item(IRON_PICK, item("Iron Pick").tool(4.6));
    catalog.add_item(STEEL_PICK, item("Steel Pick").tool(6.0));
    catalog.add_item(OBSIDIAN_BLADE, item("Obsidian Blade").tool(3.2));

    catalog.craft(recipe().input(WOOD, 1).output(PLANKS, 4));
    catalog.craft(recipe().input(HEARTWOOD, 1).output(PLANKS, 6));
    catalog.craft(recipe().input(FUNGWOOD, 1).output(PLANKS, 4));
    catalog.craft(recipe().input(PLANKS, 2).output(STICK, 4));
    catalog.craft(recipe().input(PLANKS, 4).output(BEAM, 1));
    catalog.craft(recipe().input(REED, 4).output(CORDAGE, 1));
    catalog.craft(recipe().input(VINE, 4).output(CORDAGE, 1));
    catalog.craft(recipe().input(KELP, 4).output(CORDAGE, 1));
    catalog.craft(recipe().input(CORDAGE, 3).output(ROPE, 16));
    catalog.craft(recipe().input(STICK, 1).input(CHARCOAL, 1).output(TORCH, 2));
    catalog.craft(recipe().input(STICK, 1).input(AMBER, 1).output(TORCH, 4));
    catalog.craft(recipe().input(PEAT, 4).output(CHARCOAL, 1));
    catalog.craft(recipe().input(WOOD, 2).output(CHARCOAL, 3));

    catalog.craft(
        recipe()
            .input(FLINT, 2)
            .input(STICK, 2)
            .output(FLINT_PICK, 1),
    );
    catalog.craft(
        recipe()
            .input(OBSIDIAN, 2)
            .input(STICK, 2)
            .output(OBSIDIAN_BLADE, 1),
    );

    catalog.craft(recipe().input(NATIVE_COPPER, 2).output(COPPER_INGOT, 1));
    catalog.craft(recipe().input(TIN_ORE, 2).output(TIN_INGOT, 1));
    catalog.craft(
        recipe()
            .input(COPPER_INGOT, 3)
            .input(TIN_INGOT, 1)
            .output(BRONZE_INGOT, 4),
    );
    catalog.craft(
        recipe()
            .input(IRON_ORE, 1)
            .input(CHARCOAL, 1)
            .output(IRON_INGOT, 1),
    );
    catalog.craft(
        recipe()
            .input(IRON_INGOT, 1)
            .input(CHARCOAL, 2)
            .output(STEEL_INGOT, 1),
    );
    catalog.craft(recipe().input(GOLD, 2).output(GOLD_INGOT, 1));

    catalog.craft(
        recipe()
            .input(COPPER_INGOT, 3)
            .input(STICK, 2)
            .output(COPPER_PICK, 1),
    );
    catalog.craft(
        recipe()
            .input(BRONZE_INGOT, 3)
            .input(STICK, 2)
            .output(BRONZE_PICK, 1),
    );
    catalog.craft(
        recipe()
            .input(IRON_INGOT, 3)
            .input(STICK, 2)
            .output(IRON_PICK, 1),
    );
    catalog.craft(
        recipe()
            .input(STEEL_INGOT, 3)
            .input(STICK, 2)
            .output(STEEL_PICK, 1),
    );
    catalog.craft(recipe().input(IRON_INGOT, 1).output(IRON, 1));
    catalog.craft(recipe().input(STEEL_INGOT, 1).output(STEEL, 1));

    catalog.craft(
        recipe()
            .input(SALTPETER, 3)
            .input(SULFUR, 1)
            .input(CHARCOAL, 1)
            .output(GUNPOWDER, 4),
    );

    catalog.craft(recipe().input(LUMEN, 1).output(LUMEN_DUST, 4));
    catalog.craft(
        recipe()
            .input(LUMEN_DUST, 2)
            .input(GLASS, 1)
            .output(LUMEN_LAMP, 1),
    );

    catalog.craft(recipe().input(SAND, 2).output(GLASS, 1));
    catalog.craft(recipe().input(CLAY, 2).output(BRICK, 1));
    catalog.craft(recipe().input(GRAVEL, 2).input(WATER, 1).output(CLAY, 1));
    catalog.craft(recipe().input(RUBBLE, 4).output(GRAVEL, 4));
    catalog.craft(recipe().input(LIMESTONE, 2).output(QUICKLIME, 1));
    catalog.craft(recipe().input(GYPSUM, 2).output(PLASTER, 2));
    catalog.craft(recipe().input(BASALT, 2).output(ROCKWOOL, 4));
}
