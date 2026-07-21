use super::icons::ItemIcons;
use crate::game::inventory::SlotRegion;
use bevy::prelude::*;
use fallingsand_core::ItemStack;

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
