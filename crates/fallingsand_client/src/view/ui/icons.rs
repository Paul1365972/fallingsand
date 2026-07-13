use crate::view::Game;
use bevy::prelude::*;
use fallingsand_core::ItemId;

#[derive(Resource)]
pub struct ItemIcons {
    handles: Vec<Handle<Image>>,
    missing: Handle<Image>,
}

impl ItemIcons {
    pub fn get(&self, item: ItemId) -> Handle<Image> {
        self.handles
            .get(item.0 as usize)
            .cloned()
            .unwrap_or_else(|| self.missing.clone())
    }

    pub fn missing(&self) -> Handle<Image> {
        self.missing.clone()
    }
}

pub fn load_item_icons(mut commands: Commands, game: Res<Game>, assets: Res<AssetServer>) {
    let items = &game.0.registries.items;
    let missing = assets.load("items/missing.png");
    let handles = (0..items.len())
        .map(|id| {
            let def = items.get(ItemId(id as u16));
            if def.sprite.is_empty() {
                missing.clone()
            } else {
                assets.load(format!("items/{}.png", def.sprite))
            }
        })
        .collect();
    commands.insert_resource(ItemIcons { handles, missing });
}
