use crate::interpolation::Interpolated;
use crate::net::{NetSet, ServerMsg, Session, SessionEnded};
use crate::{AppState, ClientRegistry, PauseState};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use fallingsand_core::{CellPos, MaterialId, Phase, TICK_RATE};
use fallingsand_protocol::{ClientMessage, PlayerId, PlayerInput, ServerMessage};

pub struct PlayerPlugin;

pub const PLAYER_SIZE: Vec2 = Vec2::new(3.8, 11.0);
const SNAP_DISTANCE: f32 = 64.0;

#[derive(Component)]
pub struct PlayerVisual {
    pub id: PlayerId,
}

#[derive(Component)]
struct NameTag(PlayerId);

#[derive(Resource, Default)]
pub struct PlayerVisuals(pub HashMap<PlayerId, Entity>);

#[derive(Resource, Default)]
pub struct PlayerNames(pub HashMap<PlayerId, String>);

#[derive(Resource)]
pub struct Hotbar {
    pub materials: Vec<MaterialId>,
    pub selected: usize,
}

#[derive(Resource, Default)]
pub struct InputState {
    pub aim: CellPos,
}

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        let registry = &app.world().resource::<ClientRegistry>().0;
        let materials: Vec<MaterialId> = registry
            .iter()
            .filter(|(_, material)| material.phase != Phase::Empty)
            .map(|(id, _)| id)
            .collect();
        app.insert_resource(Hotbar {
            materials,
            selected: 0,
        })
        .init_resource::<PlayerVisuals>()
        .init_resource::<PlayerNames>()
        .init_resource::<InputState>()
        .insert_resource(Time::<Fixed>::from_hz(TICK_RATE as f64))
        .add_systems(
            PreUpdate,
            (track_names, apply_entity_states).chain().after(NetSet),
        )
        .add_systems(
            FixedUpdate,
            send_input.run_if(in_state(PauseState::Running)),
        )
        .add_systems(
            Update,
            (
                select_material.run_if(in_state(PauseState::Running)),
                update_nametags.run_if(resource_changed::<PlayerNames>),
            ),
        )
        .add_systems(Update, cleanup_players.run_if(on_message::<SessionEnded>))
        .add_systems(OnExit(AppState::InGame), cleanup_players);
    }
}

fn track_names(mut names: ResMut<PlayerNames>, mut messages: MessageReader<ServerMsg>) {
    for ServerMsg(message) in messages.read() {
        match message {
            ServerMessage::PlayerJoined { player, name } => {
                names.0.insert(*player, name.clone());
            }
            ServerMessage::PlayerLeft { player } => {
                names.0.remove(player);
            }
            _ => {}
        }
    }
}

fn update_nametags(names: Res<PlayerNames>, mut tags: Query<(&NameTag, &mut Text2d)>) {
    for (tag, mut text) in &mut tags {
        let name = names.0.get(&tag.0).map(String::as_str).unwrap_or("");
        if text.0 != name {
            text.0 = name.to_string();
        }
    }
}

fn apply_entity_states(
    mut commands: Commands,
    mut visuals: ResMut<PlayerVisuals>,
    mut messages: MessageReader<ServerMsg>,
    mut query: Query<&mut Interpolated, With<PlayerVisual>>,
    session: Option<Res<Session>>,
    names: Res<PlayerNames>,
) {
    let local = session.and_then(|session| session.player);
    let mut seen: Option<Vec<PlayerId>> = None;
    for ServerMsg(message) in messages.read() {
        let ServerMessage::EntityStates { entities } = message else {
            continue;
        };
        seen = Some(entities.iter().map(|state| state.player).collect());
        for state in entities {
            let target = Vec2::new(state.x, state.y);
            if let Some(&entity) = visuals.0.get(&state.player) {
                if let Ok(mut visual) = query.get_mut(entity) {
                    let snap = visual.target_position().distance_squared(target)
                        > SNAP_DISTANCE * SNAP_DISTANCE;
                    visual.record(target, 0.0, snap);
                }
            } else {
                let is_local = local == Some(state.player);
                let color = if is_local {
                    Color::srgb(0.95, 0.75, 0.35)
                } else {
                    Color::srgb(0.55, 0.8, 0.95)
                };
                let entity = commands
                    .spawn((
                        PlayerVisual { id: state.player },
                        Interpolated::snapped(target, 0.0),
                        Sprite::from_color(color, PLAYER_SIZE),
                        Transform::from_xyz(target.x, target.y, 10.0),
                    ))
                    .id();
                if !is_local {
                    let name = names.0.get(&state.player).cloned().unwrap_or_default();
                    commands.entity(entity).with_child((
                        NameTag(state.player),
                        Text2d::new(name),
                        TextFont {
                            font_size: FontSize::Px(24.0),
                            ..default()
                        },
                        TextColor(Color::srgba(0.92, 0.95, 1.0, 0.9)),
                        Transform::from_xyz(0.0, PLAYER_SIZE.y / 2.0 + 5.0, 1.0)
                            .with_scale(Vec3::splat(0.25)),
                    ));
                }
                visuals.0.insert(state.player, entity);
            }
        }
    }

    let Some(seen) = seen else {
        return;
    };
    let stale: Vec<PlayerId> = visuals
        .0
        .keys()
        .filter(|id| !seen.contains(id))
        .copied()
        .collect();
    for id in stale {
        if let Some(entity) = visuals.0.remove(&id) {
            commands.entity(entity).despawn();
        }
    }
}

fn cleanup_players(
    mut commands: Commands,
    mut visuals: ResMut<PlayerVisuals>,
    mut names: ResMut<PlayerNames>,
    mut input: ResMut<InputState>,
) {
    for (_, entity) in visuals.0.drain() {
        commands.entity(entity).despawn();
    }
    names.0.clear();
    *input = InputState::default();
}

fn select_material(
    keys: Res<ButtonInput<KeyCode>>,
    mut hotbar: ResMut<Hotbar>,
    chat_open: Res<crate::chat::ChatOpen>,
) {
    if chat_open.0 {
        return;
    }
    const DIGITS: [KeyCode; 10] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
        KeyCode::Digit0,
    ];
    for (index, key) in DIGITS.iter().enumerate() {
        if keys.just_pressed(*key) && index < hotbar.materials.len() {
            hotbar.selected = index;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn send_input(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform)>,
    hotbar: Res<Hotbar>,
    chat_open: Res<crate::chat::ChatOpen>,
    mut state: ResMut<InputState>,
    session: Option<ResMut<Session>>,
) {
    let Some(mut session) = session else {
        return;
    };

    let (camera, camera_transform) = *camera;
    if let Some(cursor) = window.cursor_position()
        && let Ok(world) = camera.viewport_to_world_2d(camera_transform, cursor)
    {
        state.aim = CellPos::new(world.x.floor() as i32, world.y.floor() as i32);
    }

    if chat_open.0 {
        session.send(&ClientMessage::Input(PlayerInput {
            aim: state.aim,
            selected: hotbar
                .materials
                .get(hotbar.selected)
                .copied()
                .unwrap_or(MaterialId::AIR),
            ..default()
        }));
        return;
    }

    let mut move_x = 0i8;
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        move_x -= 1;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        move_x += 1;
    }

    let input = PlayerInput {
        move_x,
        jump: keys.pressed(KeyCode::Space)
            || keys.pressed(KeyCode::KeyW)
            || keys.pressed(KeyCode::ArrowUp),
        down: keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown),
        primary: buttons.pressed(MouseButton::Left),
        secondary: buttons.pressed(MouseButton::Right),
        aim: state.aim,
        selected: hotbar
            .materials
            .get(hotbar.selected)
            .copied()
            .unwrap_or(MaterialId::AIR),
    };
    session.send(&ClientMessage::Input(input));
}
