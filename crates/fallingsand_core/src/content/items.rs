use crate::content::macros::items;

const STICK_ART: &str = "  ██\n  ██\n ██ \n ██ \n██  ";
const INGOT_ART: &str = " ███ \n█████\n█████\n ███ ";
const PICK_ART: &str = "█████\n  ██ \n  ██ \n ██  \n██   ";

items! {
    STICK          "Stick"          99 glyph(STICK_ART, [154, 105, 62, 255]);
    IRON_INGOT     "Iron Ingot"     99 glyph(INGOT_ART, [190, 198, 210, 255]);
    GOLD_INGOT     "Gold Ingot"     99 glyph(INGOT_ART, [236, 190, 48, 255]);
    WOODEN_PICKAXE "Wooden Pickaxe" 1  glyph(PICK_ART, [154, 105, 62, 255]) tool(tier: 1, speed: 2.0);
    STONE_PICKAXE  "Stone Pickaxe"  1  glyph(PICK_ART, [132, 134, 142, 255]) tool(tier: 2, speed: 3.2);
    IRON_PICKAXE   "Iron Pickaxe"   1  glyph(PICK_ART, [190, 198, 210, 255]) tool(tier: 3, speed: 4.8);
}
