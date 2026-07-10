use crate::camera::SkyCamera;
use crate::chat::ChatOpen;
use crate::inventory::{BrushRadius, InventoryOpen, SelectedSlot};
use crate::net::{NetSet, ServerMsg, Session, SessionEnded};
use crate::player::LocalPlayerState;
use crate::{AppState, GameState, PauseState};
use bevy::input::InputSystems;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use fallingsand_core::{CellPos, HOTBAR_SLOTS, MAX_BRUSH, TICK_RATE};
use fallingsand_protocol::{
    ClientMessage, GameMode, InputAction, InputFrame, InputState, ServerMessage,
};

pub struct InputPlugin;

const MOVE_LEFT_KEYS: [KeyCode; 2] = [KeyCode::KeyA, KeyCode::ArrowLeft];
const MOVE_RIGHT_KEYS: [KeyCode; 2] = [KeyCode::KeyD, KeyCode::ArrowRight];
const JUMP_KEYS: [KeyCode; 3] = [KeyCode::Space, KeyCode::KeyW, KeyCode::ArrowUp];
const DOWN_KEYS: [KeyCode; 2] = [KeyCode::KeyS, KeyCode::ArrowDown];
const PRIMARY_BUTTON: MouseButton = MouseButton::Left;
const SECONDARY_BUTTON: MouseButton = MouseButton::Right;
const FLY_TAP_KEY: KeyCode = KeyCode::Space;
const INVENTORY_KEY: KeyCode = KeyCode::KeyE;
const BACK_KEY: KeyCode = KeyCode::Escape;
const CHAT_KEY: KeyCode = KeyCode::Enter;
const SCREENSHOT_KEY: KeyCode = KeyCode::F2;
const DEBUG_KEY: KeyCode = KeyCode::F3;
const DEBUG_BORDERS_KEY: KeyCode = KeyCode::KeyG;
const DEBUG_GAMEMODE_KEY: KeyCode = KeyCode::KeyN;
const FULLSCREEN_KEY: KeyCode = KeyCode::F11;
const SLOT_KEYS: [KeyCode; 9] = [
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
const BRUSH_SHRINK_KEYS: [KeyCode; 2] = [KeyCode::BracketLeft, KeyCode::Minus];
const BRUSH_GROW_KEYS: [KeyCode; 2] = [KeyCode::BracketRight, KeyCode::Equal];
const DOUBLE_TAP_SECS: f32 = 0.3;
const SCROLL_EPSILON: f32 = 0.01;

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub enum InputContext {
    #[default]
    Menu,
    Connecting,
    Chat,
    Paused,
    Inventory,
    Gameplay,
}

#[derive(Message, Clone, Copy, PartialEq)]
pub enum LocalAction {
    ToggleInventory,
    TogglePause,
    OpenChat,
    SubmitChat,
    CancelChat,
    ToggleDebugOverlay,
    ToggleDebugBorders,
    ToggleGameMode,
    Screenshot,
    ToggleFullscreen,
    Zoom(f32),
    CancelConnect,
}

#[derive(Resource, Default, Clone, Copy)]
pub struct InputHeld(pub InputState);

#[derive(Resource, Default, Clone, Copy)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
}

#[derive(Resource, Default, Clone, Copy)]
pub struct Pointer {
    pub primary_click: bool,
    pub secondary_click: bool,
}

#[derive(Resource, Default)]
pub struct InputAccumulator {
    latched: InputState,
    actions: Vec<InputAction>,
    blocked_primary: bool,
    blocked_secondary: bool,
    last_fly_tap: f32,
    f3_combo: bool,
}

impl InputAccumulator {
    pub fn queue(&mut self, action: InputAction) {
        self.actions.push(action);
    }
}

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputContext>()
            .init_resource::<InputHeld>()
            .init_resource::<Modifiers>()
            .init_resource::<Pointer>()
            .init_resource::<InputAccumulator>()
            .add_message::<LocalAction>()
            .insert_resource(Time::<Fixed>::from_hz(TICK_RATE as f64))
            .add_systems(
                PreUpdate,
                (
                    (resolve_context, sample).chain().after(InputSystems),
                    session_sync.after(NetSet),
                ),
            )
            .add_systems(FixedUpdate, flush.run_if(in_state(PauseState::Running)))
            .add_systems(OnEnter(PauseState::Paused), pause_release)
            .add_systems(Update, cleanup.run_if(on_message::<SessionEnded>))
            .add_systems(OnExit(AppState::InGame), cleanup);
    }
}

fn resolve_context(
    game_state: Option<Res<State<GameState>>>,
    pause: Option<Res<State<PauseState>>>,
    chat_open: Res<ChatOpen>,
    inv_open: Res<InventoryOpen>,
    mut context: ResMut<InputContext>,
) {
    let next = match game_state.as_ref().map(|state| *state.get()) {
        None => InputContext::Menu,
        Some(GameState::Connecting) => InputContext::Connecting,
        Some(GameState::Playing) => {
            if chat_open.0 {
                InputContext::Chat
            } else if pause
                .as_ref()
                .is_some_and(|state| *state.get() == PauseState::Paused)
            {
                InputContext::Paused
            } else if inv_open.0 {
                InputContext::Inventory
            } else {
                InputContext::Gameplay
            }
        }
    };
    if *context != next {
        *context = next;
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
fn sample(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<MouseWheel>,
    window: Option<Single<&Window>>,
    camera: Option<Single<(&Camera, &GlobalTransform), With<SkyCamera>>>,
    interactions: Query<&Interaction>,
    context: Res<InputContext>,
    player: Res<LocalPlayerState>,
    time: Res<Time>,
    mut selected: ResMut<SelectedSlot>,
    mut brush: ResMut<BrushRadius>,
    mut modifiers: ResMut<Modifiers>,
    mut pointer: ResMut<Pointer>,
    mut held: ResMut<InputHeld>,
    mut acc: ResMut<InputAccumulator>,
    mut actions: MessageWriter<LocalAction>,
) {
    modifiers.shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    modifiers.ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    pointer.primary_click = buttons.just_pressed(PRIMARY_BUTTON);
    pointer.secondary_click = buttons.just_pressed(SECONDARY_BUTTON);

    let mut aim = acc.latched.aim;
    if let (Some(window), Some(camera)) = (&window, &camera) {
        let (camera, camera_transform) = **camera;
        if let Some(cell) = cursor_cell(window, camera, camera_transform) {
            aim = cell;
        }
    }

    let gameplay = *context == InputContext::Gameplay;
    let over_ui = interactions
        .iter()
        .any(|interaction| !matches!(interaction, Interaction::None));
    let suppress = !gameplay || over_ui;
    if pointer.primary_click && suppress {
        acc.blocked_primary = true;
    }
    if !buttons.pressed(PRIMARY_BUTTON) {
        acc.blocked_primary = false;
    }
    if pointer.secondary_click && suppress {
        acc.blocked_secondary = true;
    }
    if !buttons.pressed(SECONDARY_BUTTON) {
        acc.blocked_secondary = false;
    }

    let scroll: f32 = wheel.read().map(|event| event.y).sum();
    if modifiers.ctrl && scroll.abs() > SCROLL_EPSILON {
        actions.write(LocalAction::Zoom(scroll));
    }

    if keys.just_pressed(SCREENSHOT_KEY) {
        actions.write(LocalAction::Screenshot);
    }
    if keys.just_pressed(FULLSCREEN_KEY) {
        actions.write(LocalAction::ToggleFullscreen);
    }
    if keys.pressed(DEBUG_KEY) && keys.just_pressed(DEBUG_BORDERS_KEY) {
        acc.f3_combo = true;
        actions.write(LocalAction::ToggleDebugBorders);
    }
    if keys.pressed(DEBUG_KEY) && keys.just_pressed(DEBUG_GAMEMODE_KEY) {
        acc.f3_combo = true;
        actions.write(LocalAction::ToggleGameMode);
    }
    if keys.just_released(DEBUG_KEY) {
        if !acc.f3_combo {
            actions.write(LocalAction::ToggleDebugOverlay);
        }
        acc.f3_combo = false;
    }

    match *context {
        InputContext::Menu => {}
        InputContext::Connecting => {
            if keys.just_pressed(BACK_KEY) {
                actions.write(LocalAction::CancelConnect);
            }
        }
        InputContext::Chat => {
            if keys.just_pressed(CHAT_KEY) {
                actions.write(LocalAction::SubmitChat);
            }
            if keys.just_pressed(BACK_KEY) {
                actions.write(LocalAction::CancelChat);
            }
        }
        InputContext::Paused => {
            if keys.just_pressed(BACK_KEY) {
                actions.write(LocalAction::TogglePause);
            }
        }
        InputContext::Inventory => {
            if keys.just_pressed(INVENTORY_KEY) || keys.just_pressed(BACK_KEY) {
                actions.write(LocalAction::ToggleInventory);
            }
        }
        InputContext::Gameplay => {
            if keys.just_pressed(BACK_KEY) {
                actions.write(LocalAction::TogglePause);
            }
            if keys.just_pressed(INVENTORY_KEY) {
                actions.write(LocalAction::ToggleInventory);
            }
            if keys.just_pressed(CHAT_KEY) {
                actions.write(LocalAction::OpenChat);
            }
        }
    }

    let mut state = InputState {
        aim,
        ..Default::default()
    };
    if gameplay {
        if keys.any_pressed(MOVE_LEFT_KEYS) {
            state.move_x -= 1;
        }
        if keys.any_pressed(MOVE_RIGHT_KEYS) {
            state.move_x += 1;
        }
        state.jump = keys.any_pressed(JUMP_KEYS);
        state.down = keys.any_pressed(DOWN_KEYS);
        state.primary = buttons.pressed(PRIMARY_BUTTON) && !acc.blocked_primary;
        state.secondary = buttons.pressed(SECONDARY_BUTTON) && !acc.blocked_secondary;

        if keys.any_just_pressed(JUMP_KEYS) {
            acc.queue(InputAction::Jump);
        }
        if keys.just_pressed(FLY_TAP_KEY) {
            let now = time.elapsed_secs();
            if player.mode == GameMode::Creative && now - acc.last_fly_tap < DOUBLE_TAP_SECS {
                acc.queue(InputAction::ToggleFlight);
                acc.last_fly_tap = 0.0;
            } else {
                acc.last_fly_tap = now;
            }
        }

        for (index, key) in SLOT_KEYS.iter().enumerate() {
            if keys.just_pressed(*key) {
                selected.0 = index;
                acc.queue(InputAction::SelectSlot(index as u8));
            }
        }
        if !modifiers.ctrl && scroll.abs() > SCROLL_EPSILON {
            let step = if scroll > 0.0 { HOTBAR_SLOTS - 1 } else { 1 };
            selected.0 = (selected.0 + step) % HOTBAR_SLOTS;
            acc.queue(InputAction::SelectSlot(selected.0 as u8));
        }
        if keys.any_just_pressed(BRUSH_SHRINK_KEYS) {
            brush.0 = brush.0.saturating_sub(1);
            acc.queue(InputAction::SetBrush(brush.0));
        }
        if keys.any_just_pressed(BRUSH_GROW_KEYS) {
            brush.0 = (brush.0 + 1).min(MAX_BRUSH);
            acc.queue(InputAction::SetBrush(brush.0));
        }
    }
    held.0 = state;
    acc.latched.merge_or(state);
}

fn flush(
    mut acc: ResMut<InputAccumulator>,
    held: Res<InputHeld>,
    session: Option<ResMut<Session>>,
) {
    let Some(mut session) = session else {
        return;
    };
    session.send(&ClientMessage::Input(InputFrame {
        state: acc.latched,
        actions: std::mem::take(&mut acc.actions),
    }));
    acc.latched = held.0;
}

fn pause_release(
    mut acc: ResMut<InputAccumulator>,
    mut held: ResMut<InputHeld>,
    session: Option<ResMut<Session>>,
) {
    let state = InputState {
        aim: acc.latched.aim,
        ..Default::default()
    };
    acc.latched = state;
    held.0 = state;
    if let Some(mut session) = session {
        session.send(&ClientMessage::Input(InputFrame {
            state,
            actions: std::mem::take(&mut acc.actions),
        }));
    }
}

fn session_sync(
    mut messages: MessageReader<ServerMsg>,
    selected: Res<SelectedSlot>,
    brush: Res<BrushRadius>,
    mut acc: ResMut<InputAccumulator>,
) {
    for ServerMsg(message) in messages.read() {
        if matches!(message, ServerMessage::HelloAck { .. }) {
            acc.queue(InputAction::SelectSlot(selected.0 as u8));
            acc.queue(InputAction::SetBrush(brush.0));
        }
    }
}

fn cleanup(mut acc: ResMut<InputAccumulator>, mut held: ResMut<InputHeld>) {
    *acc = InputAccumulator::default();
    held.0 = InputState::default();
}
