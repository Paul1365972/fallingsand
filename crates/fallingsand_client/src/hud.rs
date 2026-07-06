use crate::ClientRegistry;
use crate::net::{NetSet, ServerMsg, Session};
use crate::player::{Hotbar, LocalInventory, LocalMode};
use crate::{AppState, GameState};
use bevy::prelude::*;
use fallingsand_core::{MAX_AIR_SECS, MaterialId};
use fallingsand_protocol::{GameMode, ServerMessage};

pub struct HudPlugin;

const SLOT_SIZE: f32 = 42.0;
const HEALTH_WIDTH: f32 = 180.0;
const FLASH_SECS: f32 = 0.35;
const FLASH_MAX_ALPHA: f32 = 0.28;

#[derive(Component)]
struct HudRoot;

#[derive(Component)]
struct HotbarRow;

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
struct HotbarSlot(MaterialId);

#[derive(Resource, Default)]
pub struct LocalHealth(pub f32);

#[derive(Resource, Default)]
pub struct LocalAir(pub f32);

#[derive(Resource, Default)]
struct FlashTimer(f32);

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalHealth>()
            .init_resource::<LocalAir>()
            .init_resource::<FlashTimer>()
            .add_systems(OnEnter(GameState::Playing), spawn_hud)
            .add_systems(OnExit(GameState::Playing), despawn_hud)
            .add_systems(
                PreUpdate,
                track_vitals
                    .after(NetSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    rebuild_hotbar,
                    highlight_hotbar,
                    update_health_bar,
                    update_air_bar,
                    update_flash,
                )
                    .run_if(in_state(AppState::InGame)),
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
        .with_child((
            HotbarRow,
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(4),
                margin: UiRect::bottom(px(10)),
                ..default()
            },
            Pickable::IGNORE,
        ));

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
                Text::new("100"),
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
    mut health: ResMut<LocalHealth>,
    mut air: ResMut<LocalAir>,
    mut flash: ResMut<FlashTimer>,
) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
    health.0 = 0.0;
    air.0 = 0.0;
    flash.0 = 0.0;
}

fn track_vitals(
    mut messages: MessageReader<ServerMsg>,
    session: Option<Res<Session>>,
    mut health: ResMut<LocalHealth>,
    mut air: ResMut<LocalAir>,
    mut flash: ResMut<FlashTimer>,
) {
    let local = session.and_then(|session| session.player);
    for ServerMsg(message) in messages.read() {
        if let ServerMessage::EntityStates { entities } = message
            && let Some(id) = local
            && let Some(state) = entities.iter().find(|state| state.player == id)
        {
            if state.hp < health.0 - 0.01 && state.hp > 0.0 {
                flash.0 = FLASH_SECS;
            }
            health.0 = state.hp;
            air.0 = state.air;
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn rebuild_hotbar(
    mut commands: Commands,
    registry: Res<ClientRegistry>,
    hotbar: Res<Hotbar>,
    mode: Res<LocalMode>,
    inventory: Res<LocalInventory>,
    row: Query<Entity, With<HotbarRow>>,
    slots: Query<Entity, With<HotbarSlot>>,
    mut shown: Local<Option<(GameMode, Vec<(MaterialId, u64)>)>>,
) {
    let survival = mode.0 == GameMode::Survival;
    let visible = hotbar.visible(mode.0, &inventory);
    let entries: Vec<(MaterialId, u64)> = visible
        .iter()
        .map(|&id| (id, if survival { inventory.count(id) } else { 0 }))
        .collect();
    let key = (mode.0, entries.clone());
    if shown.as_ref() == Some(&key) && slots.iter().count() == entries.len() {
        return;
    }
    let Ok(row) = row.single() else {
        *shown = None;
        return;
    };
    *shown = Some(key);
    for slot in &slots {
        commands.entity(slot).despawn();
    }
    commands.entity(row).with_children(|parent| {
        for (index, &(material, count)) in entries.iter().enumerate() {
            let definition = registry.0.get(material);
            let color = definition.colors[0];
            parent
                .spawn((
                    HotbarSlot(material),
                    Node {
                        width: px(SLOT_SIZE),
                        height: px(SLOT_SIZE),
                        border: UiRect::all(px(2)),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::SpaceBetween,
                        ..default()
                    },
                    BackgroundColor(Color::srgba_u8(color[0], color[1], color[2], 200)),
                    BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                ))
                .with_children(|slot| {
                    let digit = if index < 9 {
                        format!("{}", index + 1)
                    } else if index == 9 {
                        "0".to_string()
                    } else {
                        String::new()
                    };
                    slot.spawn((
                        Text::new(digit),
                        TextFont {
                            font_size: FontSize::Px(11.0),
                            ..default()
                        },
                        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
                    ));
                    if survival {
                        slot.spawn((
                            Text::new(format_count(count)),
                            TextFont {
                                font_size: FontSize::Px(11.0),
                                ..default()
                            },
                            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.95)),
                        ));
                    }
                });
        }
    });
}

fn format_count(count: u64) -> String {
    if count >= 100_000 {
        format!("{}k", count / 1000)
    } else {
        format!("{count}")
    }
}

fn update_health_bar(
    health: Res<LocalHealth>,
    mut fill: Query<&mut Node, With<HealthFill>>,
    mut label: Query<&mut Text, With<HealthLabel>>,
) {
    if !health.is_changed() {
        return;
    }
    for mut node in &mut fill {
        node.width = percent(health.0.clamp(0.0, 100.0));
    }
    for mut text in &mut label {
        **text = format!("{:.0}", health.0.max(0.0));
    }
}

fn update_air_bar(
    air: Res<LocalAir>,
    mode: Res<LocalMode>,
    mut bar: Query<&mut Node, (With<AirBar>, Without<AirFill>)>,
    mut fill: Query<&mut Node, With<AirFill>>,
) {
    if !air.is_changed() && !mode.is_changed() {
        return;
    }
    let show = mode.0 == GameMode::Survival && air.0 < MAX_AIR_SECS - 0.05;
    for mut node in &mut bar {
        node.display = if show { Display::Flex } else { Display::None };
    }
    for mut node in &mut fill {
        node.width = percent((air.0 / MAX_AIR_SECS * 100.0).clamp(0.0, 100.0));
    }
}

fn update_flash(
    time: Res<Time>,
    mut flash: ResMut<FlashTimer>,
    mut overlay: Query<&mut BackgroundColor, With<DamageFlash>>,
) {
    if flash.0 <= 0.0 {
        return;
    }
    flash.0 = (flash.0 - time.delta_secs()).max(0.0);
    let alpha = flash.0 / FLASH_SECS * FLASH_MAX_ALPHA;
    for mut color in &mut overlay {
        *color = BackgroundColor(Color::srgba(0.9, 0.1, 0.1, alpha));
    }
}

fn highlight_hotbar(hotbar: Res<Hotbar>, mut slots: Query<(&HotbarSlot, &mut BorderColor)>) {
    for (slot, mut border) in &mut slots {
        *border = if slot.0 == hotbar.selected {
            BorderColor::all(Color::srgb(1.0, 0.9, 0.4))
        } else {
            BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.6))
        };
    }
}
