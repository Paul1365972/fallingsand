use crate::camera::WORLD_LAYER;
use crate::interpolation::Interpolated;
use crate::inventory::{BrushRadius, InventoryOpen, SelectedSlot};
use crate::net::{NetSet, ServerMsg, Session, SessionEnded, TickMessage};
use crate::{AppState, GameState, PauseState};
use bevy::camera::visibility::RenderLayers;
use bevy::input::mouse::MouseWheel;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use fallingsand_core::{CellPos, HOTBAR_SLOTS, TICK_RATE};
use fallingsand_protocol::{
    ClientMessage, GameMode, MAX_BRUSH, PlayerId, PlayerInput, ServerMessage,
};

pub struct PlayerPlugin;

pub const PLAYER_SIZE: Vec2 = Vec2::new(3.8, 11.0);
pub const PLAYER_DUCK_SIZE: Vec2 = Vec2::new(3.8, 6.0);
const SNAP_DISTANCE: f32 = 64.0;
const DOUBLE_TAP_SECS: f32 = 0.3;

#[derive(Component)]
pub struct PlayerVisual {
    pub id: PlayerId,
    pub burning: bool,
}

#[derive(Component)]
struct NameTag(PlayerId);

#[derive(Resource, Default)]
pub struct PlayerVisuals(pub HashMap<PlayerId, Entity>);

#[derive(Resource, Default)]
pub struct PlayerNames(pub HashMap<PlayerId, String>);

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub struct LocalMode(pub GameMode);

#[derive(Resource, Default)]
pub struct PointerBlocked {
    primary: bool,
    secondary: bool,
}

#[derive(Resource, Default)]
pub struct InputState {
    pub aim: CellPos,
}

#[derive(Resource, Default, Clone, Copy)]
pub struct LocalPlayerState {
    pub present: bool,
    pub pos: Vec2,
    pub hp: f32,
    pub air: f32,
    pub burning: bool,
    pub ducking: bool,
    pub mode: GameMode,
}

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerVisuals>()
            .init_resource::<PlayerNames>()
            .init_resource::<InputState>()
            .init_resource::<LocalPlayerState>()
            .init_resource::<LocalMode>()
            .init_resource::<PointerBlocked>()
            .insert_resource(Time::<Fixed>::from_hz(TICK_RATE as f64))
            .add_systems(
                PreUpdate,
                (track_names, apply_players, apply_self_state)
                    .chain()
                    .after(NetSet),
            )
            .add_systems(
                FixedUpdate,
                send_input.run_if(in_state(PauseState::Running)),
            )
            .add_systems(
                Update,
                (
                    update_pointer_block.run_if(in_state(GameState::Playing)),
                    (select_slot, toggle_fly).run_if(in_state(PauseState::Running)),
                    update_nametags.run_if(resource_changed::<PlayerNames>),
                ),
            )
            .add_systems(Update, cleanup_players.run_if(on_message::<SessionEnded>))
            .add_systems(OnExit(AppState::InGame), cleanup_players);
    }
}

fn toggle_fly(
    keys: Res<ButtonInput<KeyCode>>,
    chat_open: Res<crate::chat::ChatOpen>,
    inv_open: Res<InventoryOpen>,
    time: Res<Time>,
    mode: Res<LocalMode>,
    session: Option<ResMut<Session>>,
    mut last_tap: Local<f32>,
) {
    let Some(mut session) = session else {
        return;
    };
    if mode.0 != GameMode::Creative || chat_open.0 || inv_open.0 {
        return;
    }
    if keys.just_pressed(KeyCode::Space) {
        let now = time.elapsed_secs();
        if now - *last_tap < DOUBLE_TAP_SECS {
            session.send(&ClientMessage::ToggleFly);
            *last_tap = 0.0;
        } else {
            *last_tap = now;
        }
    }
}

fn update_pointer_block(
    buttons: Res<ButtonInput<MouseButton>>,
    pause: Option<Res<State<PauseState>>>,
    chat_open: Res<crate::chat::ChatOpen>,
    inv_open: Res<InventoryOpen>,
    interactions: Query<&Interaction>,
    mut blocked: ResMut<PointerBlocked>,
) {
    let paused = pause.is_some_and(|state| *state.get() == PauseState::Paused);
    let over_ui = interactions
        .iter()
        .any(|interaction| !matches!(interaction, Interaction::None));
    let suppress = paused || chat_open.0 || inv_open.0 || over_ui;

    if buttons.just_pressed(MouseButton::Left) && suppress {
        blocked.primary = true;
    }
    if !buttons.pressed(MouseButton::Left) {
        blocked.primary = false;
    }
    if buttons.just_pressed(MouseButton::Right) && suppress {
        blocked.secondary = true;
    }
    if !buttons.pressed(MouseButton::Right) {
        blocked.secondary = false;
    }
}

fn track_names(
    mut commands: Commands,
    mut names: ResMut<PlayerNames>,
    mut visuals: ResMut<PlayerVisuals>,
    mut messages: MessageReader<ServerMsg>,
) {
    for ServerMsg(message) in messages.read() {
        match message {
            ServerMessage::PlayerJoined { player, name } => {
                names.0.insert(*player, name.clone());
            }
            ServerMessage::PlayerLeft { player } => {
                names.0.remove(player);
                if let Some(entity) = visuals.0.remove(player) {
                    commands.entity(entity).despawn();
                }
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

#[allow(clippy::too_many_arguments)]
fn apply_players(
    mut commands: Commands,
    mut visuals: ResMut<PlayerVisuals>,
    mut frames: MessageReader<TickMessage>,
    mut query: Query<(&mut Interpolated, &mut Sprite, &mut PlayerVisual)>,
    session: Option<Res<Session>>,
    names: Res<PlayerNames>,
    mut local_state: ResMut<LocalPlayerState>,
) {
    let local = session.and_then(|session| session.player);
    for TickMessage(tick) in frames.read() {
        for state in &tick.players {
            if local == Some(state.player) {
                local_state.pos = Vec2::new(state.x.to_f32(), state.y.to_f32());
                local_state.burning = state.burning;
                local_state.ducking = state.ducking;
                local_state.present = true;
            }
            let target = Vec2::new(state.x.to_f32(), state.y.to_f32());
            let size = if state.ducking {
                PLAYER_DUCK_SIZE
            } else {
                PLAYER_SIZE
            };
            if let Some(&entity) = visuals.0.get(&state.player) {
                if let Ok((mut visual, mut sprite, mut marker)) = query.get_mut(entity) {
                    let snap = visual.target_position().distance_squared(target)
                        > SNAP_DISTANCE * SNAP_DISTANCE;
                    visual.record(target, 0.0, snap);
                    if sprite.custom_size != Some(size) {
                        sprite.custom_size = Some(size);
                    }
                    if marker.burning != state.burning {
                        marker.burning = state.burning;
                    }
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
                        PlayerVisual {
                            id: state.player,
                            burning: state.burning,
                        },
                        Interpolated::snapped(target, 0.0),
                        Sprite::from_color(color, size),
                        Transform::from_xyz(target.x, target.y, 10.0),
                        RenderLayers::layer(WORLD_LAYER),
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
}

fn apply_self_state(
    mut frames: MessageReader<TickMessage>,
    mut mode: ResMut<LocalMode>,
    mut local_state: ResMut<LocalPlayerState>,
) {
    for TickMessage(tick) in frames.read() {
        if let Some(self_state) = tick.self_state {
            if mode.0 != self_state.mode {
                mode.0 = self_state.mode;
            }
            local_state.hp = self_state.hp;
            local_state.air = self_state.air;
            local_state.mode = self_state.mode;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn cleanup_players(
    mut commands: Commands,
    mut visuals: ResMut<PlayerVisuals>,
    mut names: ResMut<PlayerNames>,
    mut input: ResMut<InputState>,
    mut mode: ResMut<LocalMode>,
    mut selected: ResMut<SelectedSlot>,
    mut local_state: ResMut<LocalPlayerState>,
) {
    for (_, entity) in visuals.0.drain() {
        commands.entity(entity).despawn();
    }
    names.0.clear();
    *input = InputState::default();
    *mode = LocalMode::default();
    selected.0 = 0;
    *local_state = LocalPlayerState::default();
}

fn select_slot(
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: MessageReader<MouseWheel>,
    chat_open: Res<crate::chat::ChatOpen>,
    inv_open: Res<InventoryOpen>,
    mut selected: ResMut<SelectedSlot>,
    mut brush: ResMut<BrushRadius>,
) {
    if chat_open.0 || inv_open.0 {
        wheel.clear();
        return;
    }
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
        if keys.just_pressed(*key) {
            selected.0 = index;
        }
    }

    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let scroll: f32 = wheel.read().map(|event| event.y).sum();
    if !ctrl && scroll.abs() > 0.01 {
        let step = if scroll > 0.0 { HOTBAR_SLOTS - 1 } else { 1 };
        selected.0 = (selected.0 + step) % HOTBAR_SLOTS;
    }

    if keys.just_pressed(KeyCode::BracketLeft) || keys.just_pressed(KeyCode::Minus) {
        brush.0 = brush.0.saturating_sub(1);
    }
    if keys.just_pressed(KeyCode::BracketRight) || keys.just_pressed(KeyCode::Equal) {
        brush.0 = (brush.0 + 1).min(MAX_BRUSH);
    }
}

fn cursor_cell(
    window: &Window,
    camera: &Camera,
    camera_transform: &GlobalTransform,
) -> Option<CellPos> {
    let cursor = window.cursor_position()?;
    let world = camera.viewport_to_world_2d(camera_transform, cursor).ok()?;
    Some(CellPos::new(world.x.floor() as i32, world.y.floor() as i32))
}

#[allow(clippy::too_many_arguments)]
fn send_input(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform), With<crate::camera::SkyCamera>>,
    selected: Res<SelectedSlot>,
    brush: Res<BrushRadius>,
    chat_open: Res<crate::chat::ChatOpen>,
    inv_open: Res<InventoryOpen>,
    blocked: Res<PointerBlocked>,
    mut state: ResMut<InputState>,
    session: Option<ResMut<Session>>,
) {
    let Some(mut session) = session else {
        return;
    };

    let (camera, camera_transform) = *camera;
    if let Some(cell) = cursor_cell(&window, camera, camera_transform) {
        state.aim = cell;
    }

    let base = PlayerInput {
        aim: state.aim,
        selected_slot: selected.0 as u8,
        brush_radius: brush.0,
        ..default()
    };

    if chat_open.0 || inv_open.0 {
        session.send(&ClientMessage::Input(base));
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
        primary: buttons.pressed(MouseButton::Left) && !blocked.primary,
        secondary: buttons.pressed(MouseButton::Right) && !blocked.secondary,
        ..base
    };
    session.send(&ClientMessage::Input(input));
}
