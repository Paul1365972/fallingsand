use crate::input::{InputAccumulator, LocalAction, Modifiers, Pointer};
use crate::inventory::{
    InventoryOpen, LocalInventory, SlotChanged, SlotCount, SlotSwatch, TrashChanged, apply_count,
    apply_swatch, format_count, item_color,
};
use crate::player::LocalPlayerState;
use crate::{ClientItemRegistry, ClientRegistry, GameState, PauseState};
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use fallingsand_core::{HOTBAR_SLOTS, Inventory as CoreInventory, ItemId, ItemStack, MAIN_SLOTS};
use fallingsand_protocol::{GameMode, InputAction, SlotAction};

pub struct InventoryUiPlugin;

const SLOT: f32 = 40.0;
const GAP: f32 = 3.0;

#[derive(Component)]
struct OverlayRoot;

#[derive(Component)]
struct CursorFollow;

#[derive(Component)]
struct CursorFollowCount;

#[derive(Component)]
struct Tooltip;

#[derive(Component)]
struct TooltipText;

#[derive(Component)]
struct CraftName;

#[derive(Clone, Copy, PartialEq)]
enum SlotRegion {
    Player,
    Trash,
    Craft(u16),
    Palette(ItemId),
}

#[derive(Component, Clone, Copy)]
struct UiSlot {
    region: SlotRegion,
    index: usize,
}

impl Plugin for InventoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (toggle_inventory, manage_overlay)
                .chain()
                .run_if(in_state(GameState::Playing))
                .run_if(in_state(PauseState::Running)),
        )
        .add_systems(
            Update,
            (build_side_panel, sync_overlay_slots, sync_craftable)
                .chain()
                .after(manage_overlay)
                .run_if(in_state(GameState::Playing)),
        )
        .add_systems(
            Update,
            (handle_clicks, update_cursor_follow, update_tooltip)
                .chain()
                .after(manage_overlay)
                .run_if(in_state(GameState::Playing))
                .run_if(in_state(PauseState::Running)),
        )
        .add_systems(OnExit(GameState::Playing), despawn_overlay);
    }
}

fn toggle_inventory(mut actions: MessageReader<LocalAction>, mut open: ResMut<InventoryOpen>) {
    for action in actions.read() {
        if *action == LocalAction::ToggleInventory {
            open.0 = !open.0;
        }
    }
}

fn manage_overlay(
    mut commands: Commands,
    open: Res<InventoryOpen>,
    roots: Query<Entity, With<OverlayRoot>>,
) {
    let exists = !roots.is_empty();
    if open.0 && !exists {
        spawn_overlay(&mut commands);
    } else if !open.0 && exists {
        for entity in &roots {
            commands.entity(entity).despawn();
        }
    }
}

fn despawn_overlay(mut commands: Commands, roots: Query<Entity, With<OverlayRoot>>) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
}

fn spawn_overlay(commands: &mut Commands) {
    commands
        .spawn((
            OverlayRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: px(16),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            GlobalZIndex(30),
        ))
        .with_children(|overlay| {
            overlay.spawn(panel_node()).with_children(|panel| {
                panel.spawn(label_node("Inventory"));
                for row in 0..(MAIN_SLOTS / 9) {
                    panel.spawn(row_node()).with_children(|r| {
                        for col in 0..9 {
                            spawn_slot(r, SlotRegion::Player, HOTBAR_SLOTS + row * 9 + col);
                        }
                    });
                }
                panel.spawn(Node {
                    height: px(6),
                    ..default()
                });
                panel.spawn(row_node()).with_children(|r| {
                    for index in 0..HOTBAR_SLOTS {
                        spawn_slot(r, SlotRegion::Player, index);
                    }
                });
                panel.spawn(Node {
                    height: px(6),
                    ..default()
                });
                panel.spawn(label_node("Trash"));
                panel.spawn(row_node()).with_children(|r| {
                    spawn_slot(r, SlotRegion::Trash, 0);
                });
            });
            overlay.spawn((SidePanel, panel_node()));
        });

    commands.spawn((
        OverlayRoot,
        Tooltip,
        Node {
            position_type: PositionType::Absolute,
            padding: UiRect::axes(px(6), px(3)),
            display: Display::None,
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.02, 0.04, 0.92)),
        BorderColor::all(Color::srgba(0.6, 0.6, 0.7, 0.5)),
        GlobalZIndex(41),
        Pickable::IGNORE,
        children![(
            TooltipText,
            Text::new(""),
            TextFont {
                font_size: FontSize::Px(12.0),
                ..default()
            },
            TextColor(Color::WHITE),
            Pickable::IGNORE,
        )],
    ));

    commands.spawn((
        OverlayRoot,
        CursorFollow,
        Node {
            position_type: PositionType::Absolute,
            width: px(SLOT - 12.0),
            height: px(SLOT - 12.0),
            display: Display::None,
            ..default()
        },
        BackgroundColor(Color::srgba(0.6, 0.6, 0.6, 0.9)),
        GlobalZIndex(40),
        Pickable::IGNORE,
        children![(
            CursorFollowCount,
            Text::new(""),
            TextFont {
                font_size: FontSize::Px(11.0),
                ..default()
            },
            TextColor(Color::WHITE),
            Node {
                position_type: PositionType::Absolute,
                right: px(0),
                bottom: px(0),
                ..default()
            },
            Pickable::IGNORE,
        )],
    ));
}

#[derive(Component)]
struct SidePanel;

fn panel_node() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        row_gap: px(GAP),
        padding: UiRect::all(px(8)),
        ..default()
    }
}

fn craft_colors(ok: bool) -> (Color, Color) {
    if ok {
        (Color::srgba(0.12, 0.16, 0.12, 0.9), Color::WHITE)
    } else {
        (
            Color::srgba(0.10, 0.10, 0.12, 0.7),
            Color::srgba(0.6, 0.6, 0.6, 1.0),
        )
    }
}

fn build_side_panel(
    mut commands: Commands,
    state: Res<LocalPlayerState>,
    registry: Res<ClientRegistry>,
    item_reg: Res<ClientItemRegistry>,
    recipes: Res<crate::ClientRecipes>,
    panels: Query<Entity, With<SidePanel>>,
    mut shown_mode: Local<Option<GameMode>>,
) {
    let Ok(panel) = panels.single() else {
        *shown_mode = None;
        return;
    };
    if *shown_mode == Some(state.mode) {
        return;
    }
    *shown_mode = Some(state.mode);

    let materials = &registry.0;
    let items = &item_reg.0;

    commands.entity(panel).despawn_related::<Children>();
    commands.entity(panel).with_children(|panel| {
        if state.mode == GameMode::Creative {
            panel.spawn(label_node("Items"));
            let all: Vec<ItemId> = items.iter().map(|(id, _)| id).collect();
            for chunk in all.chunks(9) {
                panel.spawn(row_node()).with_children(|r| {
                    for &id in chunk {
                        spawn_slot(r, SlotRegion::Palette(id), 0);
                    }
                });
            }
        } else {
            panel.spawn(label_node("Crafting"));
            for (i, recipe) in recipes.0.recipes().iter().enumerate() {
                let (background, text_color) = craft_colors(false);
                panel
                    .spawn((
                        UiSlot {
                            region: SlotRegion::Craft(i as u16),
                            index: 0,
                        },
                        Button,
                        Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: px(6),
                            padding: UiRect::all(px(3)),
                            ..default()
                        },
                        BackgroundColor(background),
                    ))
                    .with_children(|entry| {
                        let color = item_color(items, materials, recipe.output.0);
                        entry.spawn((
                            Node {
                                width: px(20),
                                height: px(20),
                                ..default()
                            },
                            BackgroundColor(Color::srgba_u8(
                                color[0], color[1], color[2], color[3],
                            )),
                            Pickable::IGNORE,
                        ));
                        let name = items
                            .try_get(recipe.output.0)
                            .map(|d| d.display.clone())
                            .unwrap_or_default();
                        entry.spawn((
                            CraftName,
                            Text::new(format!("{} x{}", name, recipe.output.1)),
                            TextFont {
                                font_size: FontSize::Px(12.0),
                                ..default()
                            },
                            TextColor(text_color),
                            Pickable::IGNORE,
                        ));
                    });
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn sync_overlay_slots(
    mut slot_changes: MessageReader<SlotChanged>,
    mut trash_changes: MessageReader<TrashChanged>,
    added: Query<Entity, Added<UiSlot>>,
    inventory: Res<LocalInventory>,
    registry: Res<ClientRegistry>,
    item_reg: Res<ClientItemRegistry>,
    slots: Query<&UiSlot>,
    mut swatches: Query<(&ChildOf, &mut Node, &mut BackgroundColor), With<SlotSwatch>>,
    mut counts: Query<(&ChildOf, &mut Text), With<SlotCount>>,
) {
    if slot_changes.is_empty() && trash_changes.is_empty() && added.is_empty() {
        return;
    }
    let changed: HashSet<usize> = slot_changes.read().map(|change| change.0).collect();
    let trash_changed = !trash_changes.is_empty();
    trash_changes.clear();
    let added: HashSet<Entity> = added.iter().collect();

    let stack_for = |entity: Entity| -> Option<Option<ItemStack>> {
        let slot = slots.get(entity).ok()?;
        let fresh = added.contains(&entity);
        match slot.region {
            SlotRegion::Player if fresh || changed.contains(&slot.index) => {
                Some(inventory.slots.get(slot.index).copied().flatten())
            }
            SlotRegion::Trash if fresh || trash_changed => Some(inventory.trash),
            SlotRegion::Palette(id) if fresh => Some(Some(ItemStack::new(id, 1))),
            _ => None,
        }
    };

    for (child_of, mut node, mut color) in &mut swatches {
        if let Some(stack) = stack_for(child_of.parent()) {
            apply_swatch(stack, &item_reg.0, &registry.0, &mut node, &mut color);
        }
    }
    for (child_of, mut text) in &mut counts {
        if let Some(stack) = stack_for(child_of.parent()) {
            apply_count(stack, &mut text);
        }
    }
}

fn sync_craftable(
    mut slot_changes: MessageReader<SlotChanged>,
    added: Query<(), Added<UiSlot>>,
    inventory: Res<LocalInventory>,
    recipes: Res<crate::ClientRecipes>,
    slots: Query<&UiSlot>,
    mut rows: Query<(&UiSlot, &mut BackgroundColor)>,
    mut names: Query<(&ChildOf, &mut TextColor), With<CraftName>>,
) {
    let dirty = !slot_changes.is_empty();
    slot_changes.clear();
    if !dirty && added.is_empty() {
        return;
    }
    if names.is_empty() {
        return;
    }
    let core = CoreInventory {
        slots: inventory.slots.clone(),
    };
    let craftable: Vec<bool> = recipes
        .0
        .recipes()
        .iter()
        .map(|recipe| recipes.0.can_craft(recipe, &core))
        .collect();

    for (slot, mut background) in &mut rows {
        let SlotRegion::Craft(i) = slot.region else {
            continue;
        };
        let ok = craftable.get(i as usize).copied().unwrap_or(false);
        let (target, _) = craft_colors(ok);
        if background.0 != target {
            background.0 = target;
        }
    }
    for (child_of, mut color) in &mut names {
        let Ok(slot) = slots.get(child_of.parent()) else {
            continue;
        };
        let SlotRegion::Craft(i) = slot.region else {
            continue;
        };
        let ok = craftable.get(i as usize).copied().unwrap_or(false);
        let (_, target) = craft_colors(ok);
        if color.0 != target {
            color.0 = target;
        }
    }
}

fn label_node(text: &str) -> impl Bundle {
    (
        Text::new(text.to_string()),
        TextFont {
            font_size: FontSize::Px(13.0),
            ..default()
        },
        TextColor(Color::srgba(0.9, 0.9, 1.0, 0.9)),
        Node {
            margin: UiRect::bottom(px(3)),
            ..default()
        },
        Pickable::IGNORE,
    )
}

fn row_node() -> Node {
    Node {
        flex_direction: FlexDirection::Row,
        column_gap: px(GAP),
        ..default()
    }
}

fn spawn_slot(parent: &mut ChildSpawnerCommands, region: SlotRegion, index: usize) {
    parent
        .spawn((
            UiSlot { region, index },
            Button,
            Node {
                width: px(SLOT),
                height: px(SLOT),
                border: UiRect::all(px(2)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.06, 0.07, 0.10, 0.92)),
            BorderColor::all(if region == SlotRegion::Trash {
                Color::srgba(0.6, 0.15, 0.1, 0.8)
            } else {
                Color::srgba(0.0, 0.0, 0.0, 0.6)
            }),
        ))
        .with_children(|slot| {
            slot.spawn((
                SlotSwatch,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(6),
                    top: px(6),
                    width: px(SLOT - 12.0),
                    height: px(SLOT - 12.0),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(Color::NONE),
                Pickable::IGNORE,
            ));
            slot.spawn((
                SlotCount,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(11.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Absolute,
                    right: px(2),
                    bottom: px(0),
                    ..default()
                },
                Pickable::IGNORE,
            ));
        });
}

fn handle_clicks(
    pointer: Res<Pointer>,
    modifiers: Res<Modifiers>,
    open: Res<InventoryOpen>,
    slots: Query<(&UiSlot, &Interaction)>,
    mut acc: ResMut<InputAccumulator>,
) {
    if !open.0 {
        return;
    }
    let left = pointer.primary_click;
    let right = pointer.secondary_click;
    if !left && !right {
        return;
    }
    let shift = modifiers.shift;

    let hovered = slots
        .iter()
        .find(|(_, interaction)| !matches!(interaction, Interaction::None))
        .map(|(slot, _)| *slot);

    if let Some(slot) = hovered {
        let action = match slot.region {
            SlotRegion::Player => {
                let s = slot.index as u16;
                if shift && left {
                    SlotAction::QuickMove { slot: s }
                } else if left {
                    SlotAction::LeftClick { slot: s }
                } else {
                    SlotAction::RightClick { slot: s }
                }
            }
            SlotRegion::Trash => {
                if !left {
                    return;
                }
                SlotAction::Trash
            }
            SlotRegion::Craft(recipe) => {
                if !left {
                    return;
                }
                SlotAction::Craft { recipe, all: shift }
            }
            SlotRegion::Palette(item) => {
                if !left {
                    return;
                }
                SlotAction::CreativeGrab { item }
            }
        };
        acc.queue(InputAction::Slot(action));
    }
}

fn update_cursor_follow(
    open: Res<InventoryOpen>,
    inventory: Res<LocalInventory>,
    registry: Res<ClientRegistry>,
    item_reg: Res<ClientItemRegistry>,
    window: Single<&Window>,
    mut follow: Query<(&mut Node, &mut BackgroundColor), With<CursorFollow>>,
    mut count: Query<&mut Text, With<CursorFollowCount>>,
) {
    let Ok((mut node, mut color)) = follow.single_mut() else {
        return;
    };
    let cursor = window.cursor_position();
    match (open.0, inventory.cursor, cursor) {
        (true, Some(stack), Some(pos)) => {
            node.display = Display::Flex;
            node.left = px(pos.x - (SLOT - 12.0) / 2.0);
            node.top = px(pos.y - (SLOT - 12.0) / 2.0);
            let c = item_color(&item_reg.0, &registry.0, stack.item);
            *color = BackgroundColor(Color::srgba_u8(c[0], c[1], c[2], c[3]));
            if let Ok(mut text) = count.single_mut() {
                **text = if stack.count > 1 {
                    format_count(stack.count)
                } else {
                    String::new()
                };
            }
        }
        _ => node.display = Display::None,
    }
}

fn update_tooltip(
    open: Res<InventoryOpen>,
    inventory: Res<LocalInventory>,
    item_reg: Res<ClientItemRegistry>,
    window: Single<&Window>,
    slots: Query<(&UiSlot, &Interaction)>,
    mut tooltip: Query<&mut Node, With<Tooltip>>,
    mut text: Query<&mut Text, With<TooltipText>>,
) {
    let Ok(mut node) = tooltip.single_mut() else {
        return;
    };
    let hovered = if open.0 {
        slots
            .iter()
            .find(|(_, interaction)| {
                matches!(interaction, Interaction::Hovered | Interaction::Pressed)
            })
            .map(|(slot, _)| *slot)
    } else {
        None
    };
    let item = hovered.and_then(|slot| match slot.region {
        SlotRegion::Player => inventory.slots.get(slot.index).copied().flatten(),
        SlotRegion::Trash => inventory.trash,
        SlotRegion::Palette(id) => Some(ItemStack::new(id, 1)),
        SlotRegion::Craft(_) => None,
    });
    match (item, window.cursor_position()) {
        (Some(stack), Some(pos)) => {
            if let Some(def) = item_reg.0.try_get(stack.item)
                && let Ok(mut text) = text.single_mut()
            {
                **text = def.display.clone();
            }
            node.display = Display::Flex;
            node.left = px(pos.x + 14.0);
            node.top = px(pos.y + 14.0);
        }
        _ => node.display = Display::None,
    }
}
