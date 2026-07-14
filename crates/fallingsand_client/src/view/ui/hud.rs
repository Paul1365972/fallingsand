use super::icons::ItemIcons;
use super::inventory::{SlotCount, SlotIcon, spawn_slot_widgets, sync_slots};
use crate::game::{ClientGame, InGame};
use crate::view::Game;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use fallingsand_core::{HOTBAR_SLOTS, ItemStack, MAX_AIR_SECS, MAX_HP};
use fallingsand_protocol::{GameMode, SelfLife};

const SLOT_SIZE: f32 = 42.0;
const HEALTH_WIDTH: f32 = 180.0;
const FLASH_SECS: f32 = 0.35;
const FLASH_MAX_ALPHA: f32 = 0.28;

#[derive(Component)]
pub(crate) struct HudRoot;

#[derive(Component)]
pub(crate) struct HealthFill;

#[derive(Component)]
pub(crate) struct HealthLabel;

#[derive(Component)]
pub(crate) struct AirBar;

#[derive(Component)]
pub(crate) struct AirFill;

#[derive(Component)]
pub(crate) struct DamageFlash;

#[derive(Component)]
pub(crate) struct DeathScreen;

#[derive(Component)]
pub(crate) struct DeathTitle;

#[derive(Component)]
pub(crate) struct DeathReviveButton;

#[derive(Component)]
pub(crate) struct HotbarSlot(usize);

#[derive(Component)]
pub(crate) struct CursorModeLabel;

fn cursor_mode_text(game: &ClientGame) -> String {
    format!("[Ctrl] Cursor: {}", game.settings.cursor_mode.label())
}

struct DeathPresentation {
    screen: Display,
    title: &'static str,
    revive: Display,
}

fn death_presentation(ingame: &InGame) -> DeathPresentation {
    if ingame.game_menu_open() {
        return DeathPresentation {
            screen: Display::None,
            title: "You died",
            revive: Display::None,
        };
    }
    match ingame.you.life {
        SelfLife::Dead if ingame.revive_request_pending => DeathPresentation {
            screen: Display::Flex,
            title: "Requesting revive...",
            revive: Display::None,
        },
        SelfLife::Dead => DeathPresentation {
            screen: Display::Flex,
            title: "You died",
            revive: Display::Flex,
        },
        SelfLife::Reviving => DeathPresentation {
            screen: Display::Flex,
            title: "Finding safe spawn...",
            revive: Display::None,
        },
        SelfLife::Entering | SelfLife::Alive(_) => DeathPresentation {
            screen: Display::None,
            title: "You died",
            revive: Display::None,
        },
    }
}

pub fn sync_hud(
    mut commands: Commands,
    game: Res<Game>,
    icons: Res<ItemIcons>,
    roots: Query<Entity, With<HudRoot>>,
) {
    let should_exist = game.0.playing().is_some();
    let exists = !roots.is_empty();
    if should_exist && !exists {
        spawn_hud(&mut commands, &game.0, &icons);
    } else if !should_exist && exists {
        for entity in &roots {
            commands.entity(entity).despawn();
        }
    }
}

pub fn patch_hud_slots(
    game: Res<Game>,
    icons: Res<ItemIcons>,
    slots: Query<&HotbarSlot>,
    mut slot_icons: Query<(&ChildOf, &mut ImageNode, &mut Node), With<SlotIcon>>,
    mut counts: Query<(&ChildOf, &mut Text), With<SlotCount>>,
) {
    if game.0.changes.slots.is_empty() {
        return;
    }
    let Some(ingame) = game.0.playing() else {
        return;
    };
    let changed: HashSet<usize> = game.0.changes.slots.iter().copied().collect();
    let stack_for = |entity: Entity| -> Option<Option<ItemStack>> {
        let slot = slots.get(entity).ok()?;
        changed
            .contains(&slot.0)
            .then(|| ingame.inventory.slot(slot.0))
    };
    sync_slots(stack_for, &icons, &mut slot_icons, &mut counts);
}

pub fn sync_cursor_hud(game: Res<Game>, mut label: Query<&mut Text, With<CursorModeLabel>>) {
    if !game.0.changes.settings {
        return;
    }
    let value = cursor_mode_text(&game.0);
    for mut text in &mut label {
        if **text != value {
            **text = value.clone();
        }
    }
}

pub fn sync_death_screen(
    game: Res<Game>,
    mut death: Query<&mut Node, (With<DeathScreen>, Without<DeathReviveButton>)>,
    mut titles: Query<&mut Text, With<DeathTitle>>,
    mut buttons: Query<&mut Node, (With<DeathReviveButton>, Without<DeathScreen>)>,
) {
    let Some(ingame) = game.0.playing() else {
        return;
    };
    let presentation = death_presentation(ingame);
    for mut node in &mut death {
        if node.display != presentation.screen {
            node.display = presentation.screen;
        }
    }
    for mut title in &mut titles {
        if **title != presentation.title {
            **title = presentation.title.into();
        }
    }
    for mut node in &mut buttons {
        node.display = presentation.revive;
    }
}

#[allow(clippy::type_complexity)]
pub fn hud_status(
    game: Res<Game>,
    mut fill: Query<&mut Node, (With<HealthFill>, Without<AirBar>, Without<AirFill>)>,
    mut label: Query<&mut Text, With<HealthLabel>>,
    mut bar: Query<&mut Node, (With<AirBar>, Without<HealthFill>, Without<AirFill>)>,
    mut air_fill: Query<&mut Node, (With<AirFill>, Without<HealthFill>, Without<AirBar>)>,
    mut overlay: Query<&mut BackgroundColor, With<DamageFlash>>,
    mut hotbar: Query<(&HotbarSlot, &mut BorderColor)>,
) {
    let Some(ingame) = game.0.playing() else {
        return;
    };
    let you = &ingame.you;
    let avatar = you.life.avatar();
    let (hp, air) = avatar.map_or((0.0, 0.0), |avatar| (avatar.hp, avatar.air));

    let width = percent((hp / MAX_HP * 100.0).clamp(0.0, 100.0));
    for mut node in &mut fill {
        if node.width != width {
            node.width = width;
        }
    }
    if let Ok(mut text) = label.single_mut() {
        let value = format!("{:.0}", hp.max(0.0));
        if **text != value {
            **text = value;
        }
    }

    let show = you.mode == GameMode::Survival && avatar.is_some() && air < MAX_AIR_SECS - 0.05;
    let display = if show { Display::Flex } else { Display::None };
    for mut node in &mut bar {
        if node.display != display {
            node.display = display;
        }
    }
    let width = percent((air / MAX_AIR_SECS * 100.0).clamp(0.0, 100.0));
    for mut node in &mut air_fill {
        if node.width != width {
            node.width = width;
        }
    }

    let alpha = you.damage_flash / FLASH_SECS * FLASH_MAX_ALPHA;
    for mut color in &mut overlay {
        let target = BackgroundColor(Color::srgba(0.9, 0.1, 0.1, alpha));
        if *color != target {
            *color = target;
        }
    }

    for (slot, mut border) in &mut hotbar {
        let target = if slot.0 == ingame.inventory.selected {
            BorderColor::all(Color::srgb(1.0, 0.9, 0.4))
        } else {
            BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.6))
        };
        if *border != target {
            *border = target;
        }
    }
}

fn spawn_hud(commands: &mut Commands, game: &ClientGame, icons: &ItemIcons) {
    let ingame = game.playing().expect("HUD requires a playing game");
    let death = death_presentation(ingame);
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
                        BorderColor::all(if index == ingame.inventory.selected {
                            Color::srgb(1.0, 0.9, 0.4)
                        } else {
                            Color::srgba(0.0, 0.0, 0.0, 0.6)
                        }),
                    ))
                    .with_children(|slot| {
                        slot.spawn((
                            Text::new(format!("{}", (index + 1) % 10)),
                            TextFont {
                                font_size: FontSize::Px(10.0),
                                ..default()
                            },
                            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.6)),
                            GlobalZIndex(super::depth::HUD_LABEL),
                            Pickable::IGNORE,
                        ));
                        spawn_slot_widgets(
                            slot,
                            SLOT_SIZE,
                            9.0,
                            ingame.inventory.slot(index),
                            icons,
                        );
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
        CursorModeLabel,
        Text::new(cursor_mode_text(game)),
        TextFont {
            font_size: FontSize::Px(12.0),
            ..default()
        },
        TextColor(Color::srgba(0.9, 0.9, 0.95, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(10),
            right: px(12),
            ..default()
        },
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
        GlobalZIndex(super::depth::DAMAGE_FLASH),
        Pickable::IGNORE,
    ));

    commands
        .spawn((
            HudRoot,
            DeathScreen,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                display: death.screen,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(18),
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.0, 0.0, 0.78)),
            GlobalZIndex(super::depth::DEATH),
        ))
        .with_children(|screen| {
            screen.spawn((
                DeathTitle,
                Text::new(death.title),
                TextFont {
                    font_size: FontSize::Px(36.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            super::spawn_button_with(
                screen,
                DeathReviveButton,
                crate::view::io::Btn::Revive,
                "Revive",
                180.0,
                super::BUTTON_BG,
                death.revive,
            );
            super::spawn_button(
                screen,
                crate::view::io::Btn::OpenGameMenu,
                "Game Menu",
                180.0,
                super::BUTTON_BG,
            );
        });
}
