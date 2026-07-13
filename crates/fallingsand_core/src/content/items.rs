use crate::content::macros::items;

items! {
    STICK          "Stick"          99;
    IRON_INGOT     "Iron Ingot"     99;
    GOLD_INGOT     "Gold Ingot"     99;
    WOODEN_PICKAXE "Wooden Pickaxe" 1  tool(tier: 1, speed: 2.0);
    STONE_PICKAXE  "Stone Pickaxe"  1  tool(tier: 2, speed: 3.2);
    IRON_PICKAXE   "Iron Pickaxe"   1  tool(tier: 3, speed: 4.8);
}
