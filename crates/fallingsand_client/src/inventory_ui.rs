use crate::inventory::{InventoryOpen, LocalInventory, item_color};
use crate::net::Session;
use crate::player::LocalMode;
use crate::{ClientItemRegistry, ClientRegistry, GameState, PauseState};
use bevy::prelude::*;
use fallingsand_core::{HOTBAR_SLOTS, Inventory as CoreInventory, ItemId, ItemStack, MAIN_SLOTS};
use fallingsand_protocol::{ClientMessage, GameMode, SlotAction};

pub struct InventoryUiPlugin;

const SLOT: f32 = 40.0;
const GAP: f32 = 3.0;

#[derive(Component)]
struct OverlayRoot;

#[derive(Component)]
struct DropZone;

#[derive(Component)]
struct CursorFollow;

#[derive(Component)]
struct CursorFollowCount;

#[derive(Component)]
struct Tooltip;

#[derive(Component)]
struct TooltipText;

#[derive(Clone, Copy, PartialEq)]
enum SlotRegion {
    Player,
    Craft(u16),
    Palette(ItemId),
}

#[derive(Component, Clone, Copy)]
struct UiSlot {
    region: SlotRegion,
    index: usize,
}

#[derive(Default, PartialEq)]
struct OverlaySig {
    mode: GameMode,
    slots: Vec<Option<ItemStack>>,
    cursor: Option<ItemStack>,
    craftable: Vec<bool>,
}

impl Plugin for InventoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                toggle_inventory,
                manage_overlay,
                rebuild_overlay,
                handle_clicks,
                update_cursor_follow,
                update_tooltip,
            )
                .chain()
                .run_if(in_state(GameState::Playing))
                .run_if(in_state(PauseState::Running)),
        )
        .add_systems(OnExit(GameState::Playing), despawn_overlay);
    }
}

fn toggle_inventory(
    keys: Res<ButtonInput<KeyCode>>,
    chat_open: Res<crate::chat::ChatOpen>,
    mut open: ResMut<InventoryOpen>,
) {
    if chat_open.0 || !keys.just_pressed(KeyCode::KeyE) {
        return;
    }
    open.0 = !open.0;
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
            DropZone,
            Button,
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
        .with_child((PlayerPanel, panel_node()))
        .with_child((SidePanel, panel_node()));

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
struct PlayerPanel;

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

#[allow(clippy::too_many_arguments)]
fn rebuild_overlay(
    mut commands: Commands,
    open: Res<InventoryOpen>,
    registry: Res<ClientRegistry>,
    item_reg: Res<ClientItemRegistry>,
    recipes: Res<crate::ClientRecipes>,
    inventory: Res<LocalInventory>,
    mode: Res<LocalMode>,
    player_panel: Query<Entity, With<PlayerPanel>>,
    side_panel: Query<Entity, With<SidePanel>>,
    mut sig: Local<Option<OverlaySig>>,
) {
    if !open.0 {
        *sig = None;
        return;
    }
    if sig.is_some() && !inventory.is_changed() && !mode.is_changed() {
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
    let next = OverlaySig {
        mode: mode.0,
        slots: inventory.slots.clone(),
        cursor: inventory.cursor,
        craftable: craftable.clone(),
    };
    if sig.as_ref() == Some(&next) {
        return;
    }
    let (Ok(player_panel), Ok(side_panel)) = (player_panel.single(), side_panel.single()) else {
        return;
    };
    *sig = Some(next);

    commands.entity(player_panel).despawn_related::<Children>();
    commands.entity(side_panel).despawn_related::<Children>();

    let materials = &registry.0;
    let items = &item_reg.0;

    commands.entity(player_panel).with_children(|panel| {
        panel.spawn(label_node("Inventory"));
        for row in 0..(MAIN_SLOTS / 9) {
            panel.spawn(row_node()).with_children(|r| {
                for col in 0..9 {
                    let index = HOTBAR_SLOTS + row * 9 + col;
                    spawn_slot(
                        r,
                        SlotRegion::Player,
                        index,
                        inventory.slots.get(index).copied().flatten(),
                        items,
                        materials,
                    );
                }
            });
        }
        panel.spawn(Node {
            height: px(6),
            ..default()
        });
        panel.spawn(row_node()).with_children(|r| {
            for index in 0..HOTBAR_SLOTS {
                spawn_slot(
                    r,
                    SlotRegion::Player,
                    index,
                    inventory.slots.get(index).copied().flatten(),
                    items,
                    materials,
                );
            }
        });
    });

    commands.entity(side_panel).with_children(|panel| {
        if mode.0 == GameMode::Creative {
            panel.spawn(label_node("Items"));
            let all: Vec<ItemId> = items.iter().map(|(id, _)| id).collect();
            for chunk in all.chunks(9) {
                panel.spawn(row_node()).with_children(|r| {
                    for &id in chunk {
                        spawn_slot(
                            r,
                            SlotRegion::Palette(id),
                            0,
                            Some(ItemStack::new(id, 1)),
                            items,
                            materials,
                        );
                    }
                });
            }
        } else {
            panel.spawn(label_node("Crafting"));
            for (i, recipe) in recipes.0.recipes().iter().enumerate() {
                let ok = craftable.get(i).copied().unwrap_or(false);
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
                        BackgroundColor(if ok {
                            Color::srgba(0.12, 0.16, 0.12, 0.9)
                        } else {
                            Color::srgba(0.10, 0.10, 0.12, 0.7)
                        }),
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
                            Text::new(format!("{} x{}", name, recipe.output.1)),
                            TextFont {
                                font_size: FontSize::Px(12.0),
                                ..default()
                            },
                            TextColor(if ok {
                                Color::WHITE
                            } else {
                                Color::srgba(0.6, 0.6, 0.6, 1.0)
                            }),
                            Pickable::IGNORE,
                        ));
                    });
            }
        }
    });
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

fn spawn_slot(
    parent: &mut ChildSpawnerCommands,
    region: SlotRegion,
    index: usize,
    stack: Option<ItemStack>,
    items: &fallingsand_core::ItemRegistry,
    materials: &fallingsand_core::MaterialRegistry,
) {
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
            BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        ))
        .with_children(|slot| {
            if let Some(item) = stack {
                let color = item_color(items, materials, item.item);
                slot.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(6),
                        top: px(6),
                        width: px(SLOT - 12.0),
                        height: px(SLOT - 12.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba_u8(color[0], color[1], color[2], color[3])),
                    Pickable::IGNORE,
                ));
                if item.count > 1 {
                    slot.spawn((
                        Text::new(crate::hud::format_count(item.count)),
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
                }
            }
        });
}

fn handle_clicks(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    open: Res<InventoryOpen>,
    slots: Query<(&UiSlot, &Interaction)>,
    drop_zone: Query<&Interaction, With<DropZone>>,
    session: Option<ResMut<Session>>,
) {
    if !open.0 {
        return;
    }
    let left = buttons.just_pressed(MouseButton::Left);
    let right = buttons.just_pressed(MouseButton::Right);
    if !left && !right {
        return;
    }
    let Some(mut session) = session else {
        return;
    };
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

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
            SlotRegion::Craft(recipe) => {
                if !left {
                    return;
                }
                SlotAction::Craft {
                    recipe,
                    times: if shift { 10 } else { 1 },
                }
            }
            SlotRegion::Palette(item) => {
                if !left {
                    return;
                }
                SlotAction::CreativeGrab { item }
            }
        };
        session.send(&ClientMessage::Slot(action));
        return;
    }

    let on_backdrop = drop_zone
        .iter()
        .any(|interaction| !matches!(interaction, Interaction::None));
    if on_backdrop {
        session.send(&ClientMessage::Slot(SlotAction::DropCursor { all: left }));
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
                    crate::hud::format_count(stack.count)
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
