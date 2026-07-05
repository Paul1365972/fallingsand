use crate::AppState;
use crate::ClientRegistry;
use crate::net::{NetSet, ServerMsg, Session};
use crate::player::Hotbar;
use bevy::prelude::*;
use fallingsand_protocol::ServerMessage;

pub struct HudPlugin;

const SLOT_SIZE: f32 = 42.0;
const HEALTH_WIDTH: f32 = 180.0;

#[derive(Component)]
struct HudRoot;

#[derive(Component)]
struct HealthFill;

#[derive(Component)]
struct HealthLabel;

#[derive(Component)]
struct HotbarSlot(usize);

#[derive(Resource, Default)]
pub struct LocalHealth(pub f32);

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalHealth>()
            .add_systems(OnEnter(AppState::InGame), spawn_hud)
            .add_systems(OnExit(AppState::InGame), despawn_hud)
            .add_systems(
                PreUpdate,
                track_health
                    .after(NetSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (update_health_bar, highlight_hotbar).run_if(in_state(AppState::InGame)),
            );
    }
}

fn spawn_hud(mut commands: Commands, registry: Res<ClientRegistry>, hotbar: Res<Hotbar>) {
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
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: px(4),
                        margin: UiRect::bottom(px(10)),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|row| {
                    for (index, material) in hotbar.materials.iter().enumerate() {
                        let definition = registry.0.get(*material);
                        let color = definition.colors[0];
                        row.spawn((
                            HotbarSlot(index),
                            Node {
                                width: px(SLOT_SIZE),
                                height: px(SLOT_SIZE),
                                border: UiRect::all(px(2)),
                                align_items: AlignItems::FlexEnd,
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            BackgroundColor(Color::srgba_u8(color[0], color[1], color[2], 200)),
                            BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                        ))
                        .with_child((
                            Text::new(format!("{}", index + 1)),
                            TextFont {
                                font_size: FontSize::Px(11.0),
                                ..default()
                            },
                            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
                        ));
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
}

fn despawn_hud(
    mut commands: Commands,
    query: Query<Entity, With<HudRoot>>,
    mut health: ResMut<LocalHealth>,
) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
    health.0 = 0.0;
}

fn track_health(
    mut messages: MessageReader<ServerMsg>,
    session: Option<Res<Session>>,
    mut health: ResMut<LocalHealth>,
) {
    let local = session.and_then(|session| session.player);
    for ServerMsg(message) in messages.read() {
        if let ServerMessage::EntityStates { entities } = message
            && let Some(id) = local
            && let Some(state) = entities.iter().find(|state| state.player == id)
        {
            health.0 = state.hp;
        }
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

fn highlight_hotbar(hotbar: Res<Hotbar>, mut slots: Query<(&HotbarSlot, &mut BorderColor)>) {
    if !hotbar.is_changed() {
        return;
    }
    for (slot, mut border) in &mut slots {
        *border = if slot.0 == hotbar.selected {
            BorderColor::all(Color::srgb(1.0, 0.9, 0.4))
        } else {
            BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.6))
        };
    }
}
