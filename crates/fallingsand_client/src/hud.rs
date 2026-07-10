use crate::inventory::{
    LocalInventory, SelectedSlot, SlotChanged, SlotCount, SlotSwatch, spawn_slot_widgets,
    sync_slots,
};
use crate::player::{LocalPlayerState, SelfDamaged};
use crate::{AppState, ClientItemRegistry, ClientRegistry, GameState};
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use fallingsand_core::{HOTBAR_SLOTS, ItemStack, MAX_AIR_SECS, MAX_HP};
use fallingsand_protocol::GameMode;

pub struct HudPlugin;

const SLOT_SIZE: f32 = 42.0;
const HEALTH_WIDTH: f32 = 180.0;
const FLASH_SECS: f32 = 0.35;
const FLASH_MAX_ALPHA: f32 = 0.28;

#[derive(Component)]
struct HudRoot;

#[derive(Component)]
struct HealthFill;

#[derive(Component)]
struct HealthLabel;

#[derive(Component)]
struct AirBar;

#[derive(Component)]
struct AirFill;

#[derive(Component)]
struct DamageFlash;

#[derive(Component)]
struct HotbarSlot(usize);

#[derive(Resource, Default)]
struct FlashTimer(f32);

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FlashTimer>()
            .add_systems(OnEnter(GameState::Playing), spawn_hud)
            .add_systems(OnExit(GameState::Playing), despawn_hud)
            .add_systems(
                Update,
                (
                    highlight_hotbar,
                    update_health_bar,
                    update_air_bar,
                    update_flash,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                sync_hotbar_slots.run_if(in_state(GameState::Playing)),
            );
    }
}

fn spawn_hud(mut commands: Commands) {
    commands
        .spawn((
            HudRoot,
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: px(4),
                    margin: UiRect::bottom(px(10)),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|row| {
                for index in 0..HOTBAR_SLOTS {
                    row.spawn((
                        HotbarSlot(index),
                        Node {
                            width: px(SLOT_SIZE),
                            height: px(SLOT_SIZE),
                            border: UiRect::all(px(2)),
                            padding: UiRect::all(px(3)),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::FlexStart,
                            justify_content: JustifyContent::SpaceBetween,
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.06, 0.07, 0.10, 0.85)),
                        BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                    ))
                    .with_children(|slot| {
                        slot.spawn((
                            Text::new(format!("{}", index + 1)),
                            TextFont {
                                font_size: FontSize::Px(10.0),
                                ..default()
                            },
                            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.6)),
                            GlobalZIndex(1),
                            Pickable::IGNORE,
                        ));
                        spawn_slot_widgets(slot, SLOT_SIZE, 9.0);
                    });
                }
            });
        });

    commands
        .spawn((
            HudRoot,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(62),
                left: percent(50),
                margin: UiRect::left(px(-HEALTH_WIDTH / 2.0)),
                width: px(HEALTH_WIDTH),
                height: px(14),
                border: UiRect::all(px(2)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.05, 0.05, 0.8)),
            BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            parent.spawn((
                HealthFill,
                Node {
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.8, 0.2, 0.2)),
                Pickable::IGNORE,
            ));
            parent.spawn((
                HealthLabel,
                Text::new(format!("{MAX_HP:.0}")),
                TextFont {
                    font_size: FontSize::Px(10.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(4),
                    top: px(-1),
                    ..default()
                },
            ));
        });

    commands
        .spawn((
            HudRoot,
            AirBar,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(80),
                left: percent(50),
                margin: UiRect::left(px(-HEALTH_WIDTH / 2.0)),
                width: px(HEALTH_WIDTH),
                height: px(8),
                border: UiRect::all(px(2)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.03, 0.06, 0.12, 0.8)),
            BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            Pickable::IGNORE,
        ))
        .with_child((
            AirFill,
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            BackgroundColor(Color::srgb(0.35, 0.65, 0.95)),
            Pickable::IGNORE,
        ));

    commands.spawn((
        HudRoot,
        DamageFlash,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(Color::srgba(0.9, 0.1, 0.1, 0.0)),
        GlobalZIndex(50),
        Pickable::IGNORE,
    ));
}

fn despawn_hud(
    mut commands: Commands,
    query: Query<Entity, With<HudRoot>>,
    mut flash: ResMut<FlashTimer>,
) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
    flash.0 = 0.0;
}

#[allow(clippy::too_many_arguments)]
fn sync_hotbar_slots(
    mut slot_changes: MessageReader<SlotChanged>,
    added: Query<Entity, Added<HotbarSlot>>,
    inventory: Res<LocalInventory>,
    registry: Res<ClientRegistry>,
    item_reg: Res<ClientItemRegistry>,
    slots: Query<&HotbarSlot>,
    mut swatches: Query<(&ChildOf, &mut Node, &mut BackgroundColor), With<SlotSwatch>>,
    mut counts: Query<(&ChildOf, &mut Text), With<SlotCount>>,
) {
    if slot_changes.is_empty() && added.is_empty() {
        return;
    }
    let changed: HashSet<usize> = slot_changes.read().map(|change| change.0).collect();
    let added: HashSet<Entity> = added.iter().collect();

    let stack_for = |entity: Entity| -> Option<Option<ItemStack>> {
        let slot = slots.get(entity).ok()?;
        (added.contains(&entity) || changed.contains(&slot.0))
            .then(|| inventory.slots.get(slot.0).copied().flatten())
    };
    sync_slots(
        stack_for,
        &item_reg.0,
        &registry.0,
        &mut swatches,
        &mut counts,
    );
}

fn update_health_bar(
    state: Res<LocalPlayerState>,
    mut fill: Query<&mut Node, With<HealthFill>>,
    mut label: Query<&mut Text, With<HealthLabel>>,
) {
    let width = percent((state.hp / MAX_HP * 100.0).clamp(0.0, 100.0));
    for mut node in &mut fill {
        if node.width != width {
            node.width = width;
        }
    }
    if let Ok(mut text) = label.single_mut() {
        let value = format!("{:.0}", state.hp.max(0.0));
        if **text != value {
            **text = value;
        }
    }
}

fn update_air_bar(
    state: Res<LocalPlayerState>,
    mut bar: Query<&mut Node, (With<AirBar>, Without<AirFill>)>,
    mut fill: Query<&mut Node, With<AirFill>>,
) {
    let show = state.mode == GameMode::Survival && state.air < MAX_AIR_SECS - 0.05;
    let display = if show { Display::Flex } else { Display::None };
    for mut node in &mut bar {
        if node.display != display {
            node.display = display;
        }
    }
    let width = percent((state.air / MAX_AIR_SECS * 100.0).clamp(0.0, 100.0));
    for mut node in &mut fill {
        if node.width != width {
            node.width = width;
        }
    }
}

fn update_flash(
    time: Res<Time>,
    mut damaged: MessageReader<SelfDamaged>,
    mut flash: ResMut<FlashTimer>,
    mut overlay: Query<&mut BackgroundColor, With<DamageFlash>>,
) {
    for _ in damaged.read() {
        flash.0 = FLASH_SECS;
    }
    if flash.0 <= 0.0 {
        return;
    }
    flash.0 = (flash.0 - time.delta_secs()).max(0.0);
    let alpha = flash.0 / FLASH_SECS * FLASH_MAX_ALPHA;
    for mut color in &mut overlay {
        *color = BackgroundColor(Color::srgba(0.9, 0.1, 0.1, alpha));
    }
}

fn highlight_hotbar(
    selected: Res<SelectedSlot>,
    mut slots: Query<(&HotbarSlot, &mut BorderColor)>,
) {
    for (slot, mut border) in &mut slots {
        let target = if slot.0 == selected.0 {
            BorderColor::all(Color::srgb(1.0, 0.9, 0.4))
        } else {
            BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.6))
        };
        if *border != target {
            *border = target;
        }
    }
}
