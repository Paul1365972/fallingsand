pub mod api;
mod items;
mod macros;
mod recipes;
pub mod spec;

pub use api::*;
pub use items::item;
pub use spec::MatSpec;

fallingsand_macros::content! {
    ember_colors: [
        [255, 190, 60, 255],
        [255, 150, 36, 255],
        [255, 220, 90, 255],
        [236, 120, 24, 255],
    ],
    materials: [
        "materials/special.rs",
        "materials/terrain.rs",
        "materials/ores.rs",
        "materials/fluids.rs",
        "materials/flora.rs",
        "materials/fire.rs",
    ],
    reactions: "reactions.rs",
}
