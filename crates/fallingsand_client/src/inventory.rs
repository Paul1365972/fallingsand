use crate::AppState;
use crate::net::{NetSet, SessionEnded, TickMessage};
use bevy::prelude::*;
use fallingsand_core::{BRUSH_RADIUS, IconSpec, ItemId, ItemRegistry, ItemStack, MaterialRegistry};

pub struct InventoryPlugin;

#[derive(Resource, Default)]
pub struct LocalInventory {
    pub slots: Vec<Option<ItemStack>>,
    pub cursor: Option<ItemStack>,
    pub trash: Option<ItemStack>,
}

#[derive(Resource, Default)]
pub struct InventoryOpen(pub bool);

#[derive(Resource, Default)]
pub struct SelectedSlot(pub usize);

#[derive(Resource)]
pub struct BrushRadius(pub u8);

impl Default for BrushRadius {
    fn default() -> Self {
        Self(BRUSH_RADIUS)
    }
}

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalInventory>()
            .init_resource::<InventoryOpen>()
            .init_resource::<SelectedSlot>()
            .init_resource::<BrushRadius>()
            .add_systems(
                PreUpdate,
                track_inventory
                    .after(NetSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(Update, cleanup.run_if(on_message::<SessionEnded>))
            .add_systems(OnExit(AppState::InGame), cleanup);
    }
}

pub fn item_color(item_reg: &ItemRegistry, materials: &MaterialRegistry, item: ItemId) -> [u8; 4] {
    match item_reg.try_get(item).map(|def| def.icon) {
        Some(IconSpec::MaterialSwatch(material)) => materials.get(material).colors[0],
        _ => [180, 180, 190, 255],
    }
}

fn track_inventory(mut inventory: ResMut<LocalInventory>, mut frames: MessageReader<TickMessage>) {
    for TickMessage(tick) in frames.read() {
        for &(index, stack) in &tick.inventory {
            let index = index as usize;
            if index >= inventory.slots.len() {
                inventory.slots.resize(index + 1, None);
            }
            inventory.slots[index] = stack;
        }
        if let Some(cursor) = tick.cursor {
            inventory.cursor = cursor;
        }
        if let Some(trash) = tick.trash {
            inventory.trash = trash;
        }
    }
}

fn cleanup(mut inventory: ResMut<LocalInventory>, mut open: ResMut<InventoryOpen>) {
    inventory.slots.clear();
    inventory.cursor = None;
    inventory.trash = None;
    open.0 = false;
}
