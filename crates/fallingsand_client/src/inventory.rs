use crate::camera::WORLD_LAYER;
use crate::interpolation::{Interpolated, interpolate};
use crate::net::{NetSet, SessionEnded, TickMessage};
use crate::{AppState, ClientItemRegistry, ClientRegistry};
use bevy::camera::visibility::RenderLayers;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use fallingsand_core::{IconSpec, ItemId, ItemRegistry, ItemStack, MaterialRegistry};
use fallingsand_protocol::EntityId;

pub struct InventoryPlugin;

const ITEM_SIZE: f32 = 0.6;
const BOB_AMPLITUDE: f32 = 0.35;
const BOB_SPEED: f32 = 3.2;

#[derive(Resource, Default)]
pub struct LocalInventory {
    pub slots: Vec<Option<ItemStack>>,
    pub cursor: Option<ItemStack>,
}

#[derive(Resource, Default)]
pub struct InventoryOpen(pub bool);

#[derive(Resource, Default)]
pub struct SelectedSlot(pub usize);

#[derive(Resource)]
pub struct BrushRadius(pub u8);

impl Default for BrushRadius {
    fn default() -> Self {
        Self(3)
    }
}

#[derive(Component)]
pub struct DroppedItemVisual {
    bob_phase: f32,
}

#[derive(Resource, Default)]
pub struct DroppedVisuals(pub HashMap<EntityId, Entity>);

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalInventory>()
            .init_resource::<InventoryOpen>()
            .init_resource::<SelectedSlot>()
            .init_resource::<BrushRadius>()
            .init_resource::<DroppedVisuals>()
            .add_systems(
                PreUpdate,
                (track_inventory, track_items)
                    .after(NetSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                bob_items
                    .after(interpolate)
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
    }
}

fn track_items(
    mut commands: Commands,
    mut visuals: ResMut<DroppedVisuals>,
    item_reg: Res<ClientItemRegistry>,
    registry: Res<ClientRegistry>,
    mut frames: MessageReader<TickMessage>,
    mut query: Query<(&mut Interpolated, &mut Sprite)>,
) {
    for TickMessage(tick) in frames.read() {
        let spawned = &tick.items.spawned;
        let moved = &tick.items.moved;
        let despawned = &tick.items.despawned;
        for state in spawned {
            let target = Vec2::new(state.x.to_f32(), state.y.to_f32());
            let color = item_color(&item_reg.0, &registry.0, state.stack.item);
            if let Some(&entity) = visuals.0.get(&state.id) {
                if let Ok((mut interp, mut sprite)) = query.get_mut(entity) {
                    interp.record(target, 0.0, true);
                    sprite.color = Color::srgba_u8(color[0], color[1], color[2], color[3]);
                }
            } else {
                let phase = (state.id.0 as f32 * 1.37) % (std::f32::consts::TAU);
                let entity = commands
                    .spawn((
                        DroppedItemVisual { bob_phase: phase },
                        Interpolated::snapped(target, 0.0),
                        Sprite::from_color(
                            Color::srgba_u8(color[0], color[1], color[2], color[3]),
                            Vec2::splat(ITEM_SIZE),
                        ),
                        Transform::from_xyz(target.x, target.y, 8.0),
                        RenderLayers::layer(WORLD_LAYER),
                    ))
                    .id();
                visuals.0.insert(state.id, entity);
            }
        }
        for mv in moved {
            if let Some(&entity) = visuals.0.get(&mv.id)
                && let Ok((mut interp, _)) = query.get_mut(entity)
            {
                let target = Vec2::new(mv.x.to_f32(), mv.y.to_f32());
                let snap = interp.target_position().distance_squared(target) > 64.0 * 64.0;
                interp.record(target, 0.0, snap);
            }
        }
        for id in despawned {
            if let Some(entity) = visuals.0.remove(id) {
                commands.entity(entity).despawn();
            }
        }
    }
}

fn bob_items(time: Res<Time>, mut query: Query<(&DroppedItemVisual, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (visual, mut transform) in &mut query {
        let bob = (t * BOB_SPEED + visual.bob_phase).sin() * BOB_AMPLITUDE;
        transform.translation.y += bob + BOB_AMPLITUDE;
    }
}

fn cleanup(
    mut commands: Commands,
    mut visuals: ResMut<DroppedVisuals>,
    mut inventory: ResMut<LocalInventory>,
    mut open: ResMut<InventoryOpen>,
) {
    for (_, entity) in visuals.0.drain() {
        commands.entity(entity).despawn();
    }
    inventory.slots.clear();
    inventory.cursor = None;
    open.0 = false;
}
