use super::icons::ItemIcons;
use crate::game::inventory::{Inventory, SlotRegion};
use crate::game::{ClientGame, InGame};
use crate::view::Game;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use fallingsand_core::{HOTBAR_SLOTS, ItemId, ItemStack, MAIN_SLOTS, content};
use fallingsand_protocol::GameMode;

const SLOT: f32 = 40.0;
const GAP: f32 = 3.0;

fn region_stack(region: SlotRegion, inventory: &Inventory) -> Option<ItemStack> {
    match region {
        SlotRegion::Player(index) => inventory.slot(index),
        SlotRegion::Trash => inventory.trash,
        SlotRegion::Palette(id) => Some(ItemStack::new(id, 1)),
        SlotRegion::Craft(_) => None,
    }
}

#[derive(Component)]
pub(crate) struct OverlayRoot;

#[derive(Component)]
pub(crate) struct CursorFollow;

#[derive(Component)]
pub(crate) struct CursorFollowCount;

#[derive(Component)]
pub(crate) struct Tooltip;

#[derive(Component)]
pub(crate) struct TooltipText;

#[derive(Component)]
pub(crate) struct CraftName;

#[derive(Component)]
pub(crate) struct SidePanel;

#[derive(Component, Clone, Copy)]
pub struct UiSlot(pub SlotRegion);

#[derive(Component)]
pub struct SlotIcon;

#[derive(Component)]
pub struct SlotCount;

pub fn spawn_slot_widgets(
    slot: &mut ChildSpawnerCommands,
    size: f32,
    inset: f32,
    stack: Option<ItemStack>,
    icons: &ItemIcons,
) {
    let (image, display) = match stack {
        Some(stack) => (icons.get(stack.item), Display::Flex),
        None => (icons.missing(), Display::None),
    };
    let count = match stack {
        Some(stack) if stack.count > 1 => stack.count.to_string(),
        _ => String::new(),
    };
    slot.spawn((
        SlotIcon,
        ImageNode::new(image),
        Node {
            position_type: PositionType::Absolute,
            left: px(inset),
            top: px(inset),
            width: px(size - 2.0 * inset),
            height: px(size - 2.0 * inset),
            display,
            ..default()
        },
        Pickable::IGNORE,
    ));
    slot.spawn((
        SlotCount,
        Text::new(count),
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

pub fn sync_slots(
    stack_for: impl Fn(Entity) -> Option<Option<ItemStack>>,
    icons: &ItemIcons,
    slot_icons: &mut Query<(&ChildOf, &mut ImageNode, &mut Node), With<SlotIcon>>,
    counts: &mut Query<(&ChildOf, &mut Text), With<SlotCount>>,
) {
    for (child_of, mut image, mut node) in slot_icons {
        if let Some(stack) = stack_for(child_of.parent()) {
            apply_icon(stack, icons, &mut image, &mut node);
        }
    }
    for (child_of, mut text) in counts {
        if let Some(stack) = stack_for(child_of.parent()) {
            apply_count(stack, &mut text);
        }
    }
}

fn apply_icon(
    stack: Option<ItemStack>,
    icons: &ItemIcons,
    image: &mut Mut<ImageNode>,
    node: &mut Mut<Node>,
) {
    match stack {
        Some(stack) => {
            let handle = icons.get(stack.item);
            if image.image != handle {
                image.image = handle;
            }
            if node.display != Display::Flex {
                node.display = Display::Flex;
            }
        }
        None => {
            if node.display != Display::None {
                node.display = Display::None;
            }
        }
    }
}

fn apply_count(stack: Option<ItemStack>, text: &mut Mut<Text>) {
    let target = match stack {
        Some(stack) if stack.count > 1 => stack.count.to_string(),
        _ => String::new(),
    };
    if text.0 != target {
        text.0 = target;
    }
}

pub fn sync_overlay(
    mut commands: Commands,
    game: Res<Game>,
    icons: Res<ItemIcons>,
    roots: Query<Entity, With<OverlayRoot>>,
    panels: Query<Entity, With<SidePanel>>,
) {
    let should_exist = game
        .0
        .playing()
        .is_some_and(|ingame| ingame.inventory_open());
    let exists = !roots.is_empty();
    if should_exist && !exists {
        spawn_overlay(&mut commands, &game.0, &icons);
        return;
    }
    if !should_exist && exists {
        for entity in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }
    if should_exist
        && game.0.changes.mode
        && let Ok(panel) = panels.single()
        && let Some(ingame) = game.0.playing()
    {
        commands.entity(panel).despawn_related::<Children>();
        commands.entity(panel).with_children(|panel| {
            build_side_panel(panel, ingame, &icons);
        });
    }
}

pub fn patch_overlay_slots(
    game: Res<Game>,
    icons: Res<ItemIcons>,
    slots: Query<&UiSlot>,
    mut slot_icons: Query<(&ChildOf, &mut ImageNode, &mut Node), With<SlotIcon>>,
    mut counts: Query<(&ChildOf, &mut Text), With<SlotCount>>,
) {
    let changes = &game.0.changes;
    if changes.slots.is_empty() && !changes.trash {
        return;
    }
    let Some(ingame) = game.0.playing() else {
        return;
    };
    let changed: HashSet<usize> = changes.slots.iter().copied().collect();
    let stack_for = |entity: Entity| -> Option<Option<ItemStack>> {
        let slot = slots.get(entity).ok()?;
        match slot.0 {
            SlotRegion::Player(index) if changed.contains(&index) => {
                Some(ingame.inventory.slot(index))
            }
            SlotRegion::Trash if changes.trash => Some(ingame.inventory.trash),
            _ => None,
        }
    };
    sync_slots(stack_for, &icons, &mut slot_icons, &mut counts);
}

pub fn sync_craftable(
    game: Res<Game>,
    slots: Query<&UiSlot>,
    mut rows: Query<(&UiSlot, &mut BackgroundColor)>,
    mut names: Query<(&ChildOf, &mut TextColor), With<CraftName>>,
) {
    if game.0.changes.slots.is_empty() || names.is_empty() {
        return;
    }
    let Some(ingame) = game.0.playing() else {
        return;
    };
    let craftable = craftable_flags(ingame);

    for (slot, mut background) in &mut rows {
        let SlotRegion::Craft(i) = slot.0 else {
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
        let SlotRegion::Craft(i) = slot.0 else {
            continue;
        };
        let ok = craftable.get(i as usize).copied().unwrap_or(false);
        let (_, target) = craft_colors(ok);
        if color.0 != target {
            color.0 = target;
        }
    }
}

fn craftable_flags(ingame: &InGame) -> Vec<bool> {
    content::recipes()
        .iter()
        .map(|recipe| recipe.can_craft(ingame.inventory.store()))
        .collect()
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

fn spawn_overlay(commands: &mut Commands, game: &ClientGame, icons: &ItemIcons) {
    let Some(ingame) = game.playing() else {
        return;
    };
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
            GlobalZIndex(super::depth::INVENTORY),
        ))
        .with_children(|overlay| {
            overlay.spawn(panel_node()).with_children(|panel| {
                panel.spawn(label_node("Inventory"));
                for row in 0..(MAIN_SLOTS / 9) {
                    panel.spawn(row_node()).with_children(|r| {
                        for col in 0..9 {
                            let index = HOTBAR_SLOTS + row * 9 + col;
                            spawn_slot(r, SlotRegion::Player(index), ingame, icons);
                        }
                    });
                }
                panel.spawn(Node {
                    height: px(6),
                    ..default()
                });
                panel.spawn(row_node()).with_children(|r| {
                    for index in 0..HOTBAR_SLOTS {
                        spawn_slot(r, SlotRegion::Player(index), ingame, icons);
                    }
                });
                panel.spawn(Node {
                    height: px(6),
                    ..default()
                });
                panel.spawn(label_node("Trash"));
                panel.spawn(row_node()).with_children(|r| {
                    spawn_slot(r, SlotRegion::Trash, ingame, icons);
                });
            });
            overlay
                .spawn((SidePanel, panel_node()))
                .with_children(|panel| {
                    build_side_panel(panel, ingame, icons);
                });
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
        GlobalZIndex(super::depth::INVENTORY_TOOLTIP),
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
        ImageNode::new(icons.missing()),
        Node {
            position_type: PositionType::Absolute,
            width: px(SLOT - 12.0),
            height: px(SLOT - 12.0),
            display: Display::None,
            ..default()
        },
        GlobalZIndex(super::depth::INVENTORY_CURSOR),
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

fn build_side_panel(panel: &mut ChildSpawnerCommands, ingame: &InGame, icons: &ItemIcons) {
    if ingame.you.mode == GameMode::Creative {
        panel.spawn(label_node("Items"));
        let all: Vec<ItemId> = content::items().map(|(id, _)| id).collect();
        for chunk in all.chunks(9) {
            panel.spawn(row_node()).with_children(|r| {
                for &id in chunk {
                    spawn_slot(r, SlotRegion::Palette(id), ingame, icons);
                }
            });
        }
    } else {
        panel.spawn(label_node("Crafting"));
        let craftable = craftable_flags(ingame);
        for (i, recipe) in content::recipes().iter().enumerate() {
            let ok = craftable.get(i).copied().unwrap_or(false);
            let (background, text_color) = craft_colors(ok);
            panel
                .spawn((
                    UiSlot(SlotRegion::Craft(i as u16)),
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
                    entry.spawn((
                        ImageNode::new(icons.get(recipe.output.0)),
                        Node {
                            width: px(20),
                            height: px(20),
                            ..default()
                        },
                        Pickable::IGNORE,
                    ));
                    let name = content::try_item(recipe.output.0)
                        .map(|info| info.display)
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
}

fn panel_node() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        row_gap: px(GAP),
        padding: UiRect::all(px(8)),
        ..default()
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

fn spawn_slot(
    parent: &mut ChildSpawnerCommands,
    region: SlotRegion,
    ingame: &InGame,
    icons: &ItemIcons,
) {
    let stack = region_stack(region, &ingame.inventory);
    parent
        .spawn((
            UiSlot(region),
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
        .with_children(|slot| spawn_slot_widgets(slot, SLOT, 6.0, stack, icons));
}

fn cursor_ui_pos(cursor: Option<Vec2>, ui_scale: &UiScale) -> Option<Vec2> {
    cursor.map(|p| p / ui_scale.0.max(f32::EPSILON))
}

pub fn update_cursor_follow(
    game: Res<Game>,
    icons: Res<ItemIcons>,
    ui_scale: Res<UiScale>,
    window: Single<&Window>,
    mut follow: Query<(&mut Node, &mut ImageNode), With<CursorFollow>>,
    mut count: Query<&mut Text, With<CursorFollowCount>>,
) {
    let Ok((mut node, mut image)) = follow.single_mut() else {
        return;
    };
    let cursor = cursor_ui_pos(window.cursor_position(), &ui_scale);
    let inventory = game.0.playing().map(|ingame| &ingame.inventory);
    let open = game
        .0
        .playing()
        .is_some_and(|ingame| ingame.inventory_open());
    match (
        open,
        inventory.and_then(|inventory| inventory.cursor),
        cursor,
    ) {
        (true, Some(stack), Some(pos)) => {
            node.display = Display::Flex;
            node.left = px(pos.x - (SLOT - 12.0) / 2.0);
            node.top = px(pos.y - (SLOT - 12.0) / 2.0);
            image.image = icons.get(stack.item);
            if let Ok(mut text) = count.single_mut() {
                **text = if stack.count > 1 {
                    stack.count.to_string()
                } else {
                    String::new()
                };
            }
        }
        _ => node.display = Display::None,
    }
}

pub fn update_tooltip(
    game: Res<Game>,
    ui_scale: Res<UiScale>,
    window: Single<&Window>,
    slots: Query<(&UiSlot, &Interaction)>,
    mut tooltip: Query<&mut Node, With<Tooltip>>,
    mut text: Query<&mut Text, With<TooltipText>>,
) {
    let Ok(mut node) = tooltip.single_mut() else {
        return;
    };
    let ingame = game.0.playing();
    let open = ingame.is_some_and(|ingame| ingame.inventory_open());
    let hovered = if open {
        slots
            .iter()
            .find(|(_, interaction)| {
                matches!(interaction, Interaction::Hovered | Interaction::Pressed)
            })
            .map(|(slot, _)| slot.0)
    } else {
        None
    };
    let item = hovered.and_then(|region| region_stack(region, &ingame?.inventory));
    match (item, cursor_ui_pos(window.cursor_position(), &ui_scale)) {
        (Some(stack), Some(pos)) => {
            if let Some(info) = content::try_item(stack.item)
                && let Ok(mut text) = text.single_mut()
            {
                **text = info.display.to_string();
            }
            node.display = Display::Flex;
            node.left = px(pos.x + 14.0);
            node.top = px(pos.y + 14.0);
        }
        _ => node.display = Display::None,
    }
}
