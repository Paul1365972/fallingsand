use super::{format_count, item_color, item_glyph};
use crate::game::inventory::{Inventory, SlotRegion};
use crate::game::{ClientGame, InGame};
use crate::view::Game;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use fallingsand_core::{HOTBAR_SLOTS, ItemId, ItemStack, MAIN_SLOTS};
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
pub struct SlotSwatch;

#[derive(Component)]
pub struct SlotCount;

#[derive(Component)]
pub struct SlotGlyph;

pub fn spawn_slot_widgets(
    slot: &mut ChildSpawnerCommands,
    size: f32,
    inset: f32,
    stack: Option<ItemStack>,
    game: &ClientGame,
) {
    let glyph = stack
        .map(|stack| item_glyph(&game.registries.items, stack.item))
        .unwrap_or("");
    let (display, color) = match stack {
        Some(stack) => {
            let c = item_color(&game.registries.items, stack.item);
            (
                if glyph.is_empty() {
                    Display::Flex
                } else {
                    Display::None
                },
                Color::srgba_u8(c[0], c[1], c[2], c[3]),
            )
        }
        None => (Display::None, Color::NONE),
    };
    let count = match stack {
        Some(stack) if stack.count > 1 => format_count(stack.count),
        _ => String::new(),
    };
    slot.spawn((
        SlotSwatch,
        Node {
            position_type: PositionType::Absolute,
            left: px(inset),
            top: px(inset),
            width: px(size - 2.0 * inset),
            height: px(size - 2.0 * inset),
            display,
            ..default()
        },
        BackgroundColor(color),
        Pickable::IGNORE,
    ));
    let glyph_color = stack
        .map(|stack| item_color(&game.registries.items, stack.item))
        .unwrap_or([255; 4]);
    slot.spawn((
        SlotGlyph,
        Text::new(glyph),
        TextFont {
            font_size: FontSize::Px(5.0),
            ..default()
        },
        TextColor(Color::srgba_u8(
            glyph_color[0],
            glyph_color[1],
            glyph_color[2],
            glyph_color[3],
        )),
        Node {
            position_type: PositionType::Absolute,
            left: px(inset + 2.0),
            top: px(inset),
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
        GlobalZIndex(2),
        Pickable::IGNORE,
    ));
}

#[allow(clippy::type_complexity)]
pub fn sync_slots(
    stack_for: impl Fn(Entity) -> Option<Option<ItemStack>>,
    items: &fallingsand_core::ItemRegistry,
    swatches: &mut Query<(&ChildOf, &mut Node, &mut BackgroundColor), With<SlotSwatch>>,
    counts: &mut Query<(&ChildOf, &mut Text), (With<SlotCount>, Without<SlotGlyph>)>,
    glyphs: &mut Query<
        (&ChildOf, &mut Text, &mut TextColor),
        (With<SlotGlyph>, Without<SlotCount>),
    >,
) {
    for (child_of, mut node, mut color) in swatches {
        if let Some(stack) = stack_for(child_of.parent()) {
            apply_swatch(stack, items, &mut node, &mut color);
        }
    }
    for (child_of, mut text) in counts {
        if let Some(stack) = stack_for(child_of.parent()) {
            apply_count(stack, &mut text);
        }
    }
    for (child_of, mut text, mut color) in glyphs {
        if let Some(stack) = stack_for(child_of.parent()) {
            let glyph = stack
                .map(|stack| item_glyph(items, stack.item))
                .unwrap_or("");
            text.0 = glyph.into();
            if let Some(stack) = stack {
                let rgba = item_color(items, stack.item);
                color.0 = Color::srgba_u8(rgba[0], rgba[1], rgba[2], rgba[3]);
            }
        }
    }
}

fn apply_swatch(
    stack: Option<ItemStack>,
    items: &fallingsand_core::ItemRegistry,
    node: &mut Mut<Node>,
    color: &mut Mut<BackgroundColor>,
) {
    match stack {
        Some(stack) => {
            let c = item_color(items, stack.item);
            let target = Color::srgba_u8(c[0], c[1], c[2], c[3]);
            let display = if item_glyph(items, stack.item).is_empty() {
                Display::Flex
            } else {
                Display::None
            };
            if node.display != display {
                node.display = display;
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

fn apply_count(stack: Option<ItemStack>, text: &mut Mut<Text>) {
    let target = match stack {
        Some(stack) if stack.count > 1 => format_count(stack.count),
        _ => String::new(),
    };
    if text.0 != target {
        text.0 = target;
    }
}

pub fn sync_overlay(
    mut commands: Commands,
    game: Res<Game>,
    roots: Query<Entity, With<OverlayRoot>>,
    panels: Query<Entity, With<SidePanel>>,
) {
    let should_exist = game
        .0
        .playing()
        .is_some_and(|ingame| ingame.inventory_open());
    let exists = !roots.is_empty();
    if should_exist && !exists {
        spawn_overlay(&mut commands, &game.0);
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
            build_side_panel(panel, &game.0, ingame);
        });
    }
}

#[allow(clippy::type_complexity)]
pub fn patch_overlay_slots(
    game: Res<Game>,
    slots: Query<&UiSlot>,
    mut swatches: Query<(&ChildOf, &mut Node, &mut BackgroundColor), With<SlotSwatch>>,
    mut counts: Query<(&ChildOf, &mut Text), (With<SlotCount>, Without<SlotGlyph>)>,
    mut glyphs: Query<(&ChildOf, &mut Text, &mut TextColor), (With<SlotGlyph>, Without<SlotCount>)>,
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
    sync_slots(
        stack_for,
        &game.0.registries.items,
        &mut swatches,
        &mut counts,
        &mut glyphs,
    );
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
    let craftable = craftable_flags(&game.0, ingame);

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

fn craftable_flags(game: &ClientGame, ingame: &InGame) -> Vec<bool> {
    let recipes = &game.registries.recipes;
    recipes
        .recipes()
        .iter()
        .map(|recipe| recipes.can_craft(recipe, ingame.inventory.store()))
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

fn spawn_overlay(commands: &mut Commands, game: &ClientGame) {
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
            GlobalZIndex(30),
        ))
        .with_children(|overlay| {
            overlay.spawn(panel_node()).with_children(|panel| {
                panel.spawn(label_node("Inventory"));
                for row in 0..(MAIN_SLOTS / 9) {
                    panel.spawn(row_node()).with_children(|r| {
                        for col in 0..9 {
                            let index = HOTBAR_SLOTS + row * 9 + col;
                            spawn_slot(r, SlotRegion::Player(index), game, ingame);
                        }
                    });
                }
                panel.spawn(Node {
                    height: px(6),
                    ..default()
                });
                panel.spawn(row_node()).with_children(|r| {
                    for index in 0..HOTBAR_SLOTS {
                        spawn_slot(r, SlotRegion::Player(index), game, ingame);
                    }
                });
                panel.spawn(Node {
                    height: px(6),
                    ..default()
                });
                panel.spawn(label_node("Trash"));
                panel.spawn(row_node()).with_children(|r| {
                    spawn_slot(r, SlotRegion::Trash, game, ingame);
                });
            });
            overlay
                .spawn((SidePanel, panel_node()))
                .with_children(|panel| {
                    build_side_panel(panel, game, ingame);
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

fn build_side_panel(panel: &mut ChildSpawnerCommands, game: &ClientGame, ingame: &InGame) {
    let items = &game.registries.items;

    if ingame.you.mode == GameMode::Creative {
        panel.spawn(label_node("Items"));
        let all: Vec<ItemId> = items.iter().map(|(id, _)| id).collect();
        for chunk in all.chunks(9) {
            panel.spawn(row_node()).with_children(|r| {
                for &id in chunk {
                    spawn_slot(r, SlotRegion::Palette(id), game, ingame);
                }
            });
        }
    } else {
        panel.spawn(label_node("Crafting"));
        let craftable = craftable_flags(game, ingame);
        for (i, recipe) in game.registries.recipes.recipes().iter().enumerate() {
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
                    let color = item_color(items, recipe.output.0);
                    entry.spawn((
                        Node {
                            width: px(20),
                            height: px(20),
                            ..default()
                        },
                        BackgroundColor(Color::srgba_u8(color[0], color[1], color[2], color[3])),
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
    game: &ClientGame,
    ingame: &InGame,
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
        .with_children(|slot| spawn_slot_widgets(slot, SLOT, 6.0, stack, game));
}

pub fn update_cursor_follow(
    game: Res<Game>,
    window: Single<&Window>,
    mut follow: Query<(&mut Node, &mut BackgroundColor), With<CursorFollow>>,
    mut count: Query<&mut Text, With<CursorFollowCount>>,
) {
    let Ok((mut node, mut color)) = follow.single_mut() else {
        return;
    };
    let cursor = window.cursor_position();
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
            let c = item_color(&game.0.registries.items, stack.item);
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

pub fn update_tooltip(
    game: Res<Game>,
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
    match (item, window.cursor_position()) {
        (Some(stack), Some(pos)) => {
            if let Some(def) = game.0.registries.items.try_get(stack.item)
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
