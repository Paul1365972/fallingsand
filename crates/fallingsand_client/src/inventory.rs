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

#[derive(Message)]
pub struct SlotChanged(pub usize);

#[derive(Message)]
pub struct TrashChanged;

#[derive(Component)]
pub struct SlotSwatch;

#[derive(Component)]
pub struct SlotCount;

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
            .add_message::<SlotChanged>()
            .add_message::<TrashChanged>()
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

pub fn format_count(count: u32) -> String {
    if count >= 100_000 {
        format!("{}k", count / 1000)
    } else {
        format!("{count}")
    }
}

pub fn apply_swatch(
    stack: Option<ItemStack>,
    items: &ItemRegistry,
    materials: &MaterialRegistry,
    node: &mut Mut<Node>,
    color: &mut Mut<BackgroundColor>,
) {
    match stack {
        Some(stack) => {
            let c = item_color(items, materials, stack.item);
            let target = Color::srgba_u8(c[0], c[1], c[2], c[3]);
            if node.display != Display::Flex {
                node.display = Display::Flex;
            }
            if color.0 != target {
                color.0 = target;
            }
        }
        None => {
            if node.display != Display::None {
                node.display = Display::None;
            }
        }
    }
}

pub fn apply_count(stack: Option<ItemStack>, text: &mut Mut<Text>) {
    let target = match stack {
        Some(stack) if stack.count > 1 => format_count(stack.count),
        _ => String::new(),
    };
    if text.0 != target {
        text.0 = target;
    }
}

fn track_inventory(
    mut inventory: ResMut<LocalInventory>,
    mut frames: MessageReader<TickMessage>,
    mut slot_changes: MessageWriter<SlotChanged>,
    mut trash_changes: MessageWriter<TrashChanged>,
) {
    for TickMessage(tick) in frames.read() {
        for &(index, stack) in &tick.inventory {
            let index = index as usize;
            if index >= inventory.slots.len() {
                inventory.slots.resize(index + 1, None);
            }
            inventory.slots[index] = stack;
            slot_changes.write(SlotChanged(index));
        }
        if let Some(cursor) = tick.cursor {
            inventory.cursor = cursor;
        }
        if let Some(trash) = tick.trash {
            inventory.trash = trash;
            trash_changes.write(TrashChanged);
        }
    }
}

fn cleanup(
    mut inventory: ResMut<LocalInventory>,
    mut open: ResMut<InventoryOpen>,
    mut slot_changes: MessageWriter<SlotChanged>,
    mut trash_changes: MessageWriter<TrashChanged>,
) {
    for index in 0..inventory.slots.len() {
        slot_changes.write(SlotChanged(index));
    }
    trash_changes.write(TrashChanged);
    inventory.slots.clear();
    inventory.cursor = None;
    inventory.trash = None;
    open.0 = false;
}
