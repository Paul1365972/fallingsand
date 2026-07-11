#[macro_use]
mod macros;

mod items;
mod reactions;
mod recipes;

pub mod material;

pub use items::item;

use fallingsand_core::{ItemRegistry, MaterialRegistry, RecipeRegistry};

pub fn material_registry() -> MaterialRegistry {
    MaterialRegistry::from_materials(&material::assemble(), &reactions::reactions())
}

pub fn item_registry(materials: &MaterialRegistry) -> ItemRegistry {
    ItemRegistry::build(items::ENTRIES, materials)
}

pub fn recipe_registry(items: &ItemRegistry) -> RecipeRegistry {
    recipes::recipes(items)
}
