use crate::net::{Conn, LocalPlayer, NetSet, ServerMsg};
use crate::{AppState, ClientRegistry, PauseState};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use fallingsand_core::{CellPos, MaterialId, Phase};
use fallingsand_protocol::{ClientMessage, PlayerId, PlayerInput, ServerMessage, encode_message};

pub struct PlayerPlugin;

pub const PLAYER_SIZE: Vec2 = Vec2::new(3.8, 11.0);
const SNAP_DISTANCE: f32 = 64.0;

#[derive(Component)]
pub struct PlayerVisual {
    pub id: PlayerId,
    pub previous: Vec2,
    pub target: Vec2,
    pub blend: f32,
}

#[derive(Resource, Default)]
pub struct PlayerVisuals(pub HashMap<PlayerId, Entity>);

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
        .init_resource::<InputState>()
        .insert_resource(Time::<Fixed>::from_hz(60.0))
        .add_systems(PreUpdate, apply_entity_states.after(NetSet))
        .add_systems(
            FixedUpdate,
            send_input.run_if(in_state(PauseState::Running)),
        )
        .add_systems(
            Update,
            (
                select_material.run_if(in_state(PauseState::Running)),
                interpolate_players,
            ),
        )
        .add_systems(OnExit(AppState::InGame), cleanup_players);
    }
}

fn apply_entity_states(
    mut commands: Commands,
    mut visuals: ResMut<PlayerVisuals>,
    mut messages: MessageReader<ServerMsg>,
    mut query: Query<(&mut PlayerVisual, &Transform)>,
    local: Res<LocalPlayer>,
) {
    let mut latest: Option<&Vec<fallingsand_protocol::EntityState>> = None;
    for ServerMsg(message) in messages.read() {
        if let ServerMessage::EntityStates { entities, .. } = message {
            latest = Some(entities);
        }
    }
    let Some(entities) = latest else {
        return;
    };

    let mut seen: Vec<PlayerId> = Vec::with_capacity(entities.len());
    for state in entities {
        seen.push(state.player);
        let target = Vec2::new(state.x, state.y);
        if let Some(&entity) = visuals.0.get(&state.player) {
            if let Ok((mut visual, transform)) = query.get_mut(entity) {
                let current = transform.translation.truncate();
                visual.previous =
                    if current.distance_squared(target) > SNAP_DISTANCE * SNAP_DISTANCE {
                        target
                    } else {
                        current
                    };
                visual.target = target;
                visual.blend = 0.0;
            }
        } else {
            let is_local = local.id == Some(state.player);
            let color = if is_local {
                Color::srgb(0.95, 0.75, 0.35)
            } else {
                Color::srgb(0.55, 0.8, 0.95)
            };
            let entity = commands
                .spawn((
                    PlayerVisual {
                        id: state.player,
                        previous: target,
                        target,
                        blend: 1.0,
                    },
                    Sprite::from_color(color, PLAYER_SIZE),
                    Transform::from_xyz(target.x, target.y, 10.0),
                ))
                .id();
            visuals.0.insert(state.player, entity);
        }
    }

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
    mut input: ResMut<InputState>,
) {
    for (_, entity) in visuals.0.drain() {
        commands.entity(entity).despawn();
    }
    *input = InputState::default();
}

fn interpolate_players(time: Res<Time>, mut query: Query<(&mut PlayerVisual, &mut Transform)>) {
    for (mut visual, mut transform) in &mut query {
        visual.blend = (visual.blend + time.delta_secs() * 60.0).min(1.0);
        let position = visual.previous.lerp(visual.target, visual.blend);
        transform.translation.x = position.x;
        transform.translation.y = position.y;
    }
}

fn select_material(keys: Res<ButtonInput<KeyCode>>, mut hotbar: ResMut<Hotbar>) {
    const DIGITS: [KeyCode; 9] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    for (index, key) in DIGITS.iter().enumerate() {
        if keys.just_pressed(*key) && index < hotbar.materials.len() {
            hotbar.selected = index;
        }
    }
}

fn send_input(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform)>,
    hotbar: Res<Hotbar>,
    mut state: ResMut<InputState>,
    conn: Option<ResMut<Conn>>,
) {
    let Some(mut conn) = conn else {
        return;
    };

    let (camera, camera_transform) = *camera;
    if let Some(cursor) = window.cursor_position()
        && let Ok(world) = camera.viewport_to_world_2d(camera_transform, cursor)
    {
        state.aim = CellPos::new(world.x.floor() as i32, world.y.floor() as i32);
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
        primary: buttons.pressed(MouseButton::Left),
        secondary: buttons.pressed(MouseButton::Right),
        aim: state.aim,
        selected: hotbar
            .materials
            .get(hotbar.selected)
            .copied()
            .unwrap_or(MaterialId::AIR),
    };
    conn.0.send(encode_message(&ClientMessage::Input(input)));
}
