use bevy::prelude::*;
use fallingsand_core::{ItemId, content};

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

pub fn load_item_icons(mut commands: Commands, assets: Res<AssetServer>) {
    let missing = assets.load("items/missing.png");
    let handles = (0..content::ITEM_COUNT)
        .map(|id| {
            let info = content::item(ItemId(id as u16));
            if info.sprite.is_empty() {
                missing.clone()
            } else {
                assets.load(format!("items/{}.png", info.sprite))
            }
        })
        .collect();
    commands.insert_resource(ItemIcons { handles, missing });
}
