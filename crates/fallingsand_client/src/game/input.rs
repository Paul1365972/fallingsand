use super::{ClientGame, Effect, Flow, IoFrame, Phase};
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;
use fallingsand_core::{HOTBAR_SLOTS, MAX_BRUSH, TICK_RATE};
use fallingsand_protocol::{ClientMessage, GameMode, InputAction, InputFrame, InputState};

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
const DEBUG_RENDERMODE_KEY: KeyCode = KeyCode::KeyR;
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
const TICK_DT: f32 = 1.0 / TICK_RATE as f32;
const MAX_CATCHUP_TICKS: f32 = 4.0;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum InputContext {
    #[default]
    Menu,
    Connecting,
    Chat,
    Paused,
    Inventory,
    Gameplay,
}

#[derive(Default)]
pub struct InputCore {
    pub context: InputContext,
    pub held: InputState,
    latched: InputState,
    actions: Vec<InputAction>,
    blocked_primary: bool,
    blocked_secondary: bool,
    last_fly_tap: f32,
    f3_combo: bool,
    acc: f32,
}

impl InputCore {
    pub fn queue(&mut self, action: InputAction) {
        self.actions.push(action);
    }

    pub(super) fn reset(&mut self) {
        let context = self.context;
        *self = Self::default();
        self.context = context;
    }

    pub(super) fn release_held(&mut self, session: Option<&mut super::net::Session>) {
        let state = InputState {
            aim: self.latched.aim,
            ..Default::default()
        };
        self.latched = state;
        self.held = state;
        if let Some(session) = session {
            session.send(&ClientMessage::Input(InputFrame {
                state,
                actions: std::mem::take(&mut self.actions),
            }));
        }
    }
}

fn compute_context(flow: &Flow) -> InputContext {
    match flow {
        Flow::Menu => InputContext::Menu,
        Flow::InGame(ingame) => match ingame.phase {
            Phase::Connecting => InputContext::Connecting,
            Phase::Playing if ingame.chat.open => InputContext::Chat,
            Phase::Playing if ingame.paused => InputContext::Paused,
            Phase::Playing if ingame.inventory.open => InputContext::Inventory,
            Phase::Playing => InputContext::Gameplay,
        },
    }
}

pub(super) fn resolve(game: &mut ClientGame, io: &IoFrame) {
    let ctrl = io
        .keys
        .any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);

    let context = compute_context(&game.flow);
    game.input.context = context;
    let gameplay = context == InputContext::Gameplay;

    {
        let input = &mut game.input;
        let suppress = !gameplay || io.over_ui;
        if io.buttons.just_pressed(PRIMARY_BUTTON) && suppress {
            input.blocked_primary = true;
        }
        if !io.buttons.pressed(PRIMARY_BUTTON) {
            input.blocked_primary = false;
        }
        if io.buttons.just_pressed(SECONDARY_BUTTON) && suppress {
            input.blocked_secondary = true;
        }
        if !io.buttons.pressed(SECONDARY_BUTTON) {
            input.blocked_secondary = false;
        }
    }

    sample(game, io, gameplay, ctrl);
    global_hotkeys(game, io, ctrl);
    context_hotkeys(game, io, context);
}

fn sample(game: &mut ClientGame, io: &IoFrame, gameplay: bool, ctrl: bool) {
    let aim = io.cursor_cell.unwrap_or(game.input.latched.aim);
    let mut state = InputState {
        aim,
        ..Default::default()
    };
    if gameplay && let Flow::InGame(ingame) = &mut game.flow {
        let input = &mut game.input;
        if io.keys.any_pressed(MOVE_LEFT_KEYS) {
            state.move_x -= 1;
        }
        if io.keys.any_pressed(MOVE_RIGHT_KEYS) {
            state.move_x += 1;
        }
        state.jump = io.keys.any_pressed(JUMP_KEYS);
        state.down = io.keys.any_pressed(DOWN_KEYS);
        state.primary = io.buttons.pressed(PRIMARY_BUTTON) && !input.blocked_primary;
        state.secondary = io.buttons.pressed(SECONDARY_BUTTON) && !input.blocked_secondary;

        if io.keys.any_just_pressed(JUMP_KEYS) {
            input.queue(InputAction::Jump);
        }
        if io.keys.just_pressed(FLY_TAP_KEY) {
            if ingame.you.mode == GameMode::Creative
                && io.now - input.last_fly_tap < DOUBLE_TAP_SECS
            {
                input.queue(InputAction::ToggleFlight);
                input.last_fly_tap = 0.0;
            } else {
                input.last_fly_tap = io.now;
            }
        }

        for (index, key) in SLOT_KEYS.iter().enumerate() {
            if io.keys.just_pressed(*key) {
                ingame.inventory.selected = index;
                input.queue(InputAction::SelectSlot(index as u8));
            }
        }
        if !ctrl && io.scroll.abs() > SCROLL_EPSILON {
            let step = if io.scroll > 0.0 { HOTBAR_SLOTS - 1 } else { 1 };
            ingame.inventory.selected = (ingame.inventory.selected + step) % HOTBAR_SLOTS;
            input.queue(InputAction::SelectSlot(ingame.inventory.selected as u8));
        }
        if io.keys.any_just_pressed(BRUSH_SHRINK_KEYS) {
            ingame.inventory.brush = ingame.inventory.brush.saturating_sub(1);
            input.queue(InputAction::SetBrush(ingame.inventory.brush));
        }
        if io.keys.any_just_pressed(BRUSH_GROW_KEYS) {
            ingame.inventory.brush = (ingame.inventory.brush + 1).min(MAX_BRUSH);
            input.queue(InputAction::SetBrush(ingame.inventory.brush));
        }
    }
    game.input.held = state;
    game.input.latched.merge_or(state);
}

pub fn clamp_zoom(base: u32, index: i32) -> i32 {
    let base = base as i32;
    index.clamp((base / 2).max(1) - base, base)
}

fn global_hotkeys(game: &mut ClientGame, io: &IoFrame, ctrl: bool) {
    let mut zoom = game.view_prefs.zoom_index;
    if ctrl && io.scroll.abs() > SCROLL_EPSILON && zoom_allowed(game.input.context) {
        zoom += io.scroll.signum() as i32;
    }
    game.view_prefs.zoom_index = clamp_zoom(io.zoom_base, zoom);
    if io.keys.just_pressed(SCREENSHOT_KEY) {
        game.effects.push(Effect::Screenshot);
    }
    if io.keys.just_pressed(FULLSCREEN_KEY) {
        game.toggle_fullscreen();
    }
    if io.keys.pressed(DEBUG_KEY) && io.keys.just_pressed(DEBUG_BORDERS_KEY) {
        game.input.f3_combo = true;
        game.view_prefs.debug_borders = !game.view_prefs.debug_borders;
    }
    if io.keys.pressed(DEBUG_KEY) && io.keys.just_pressed(DEBUG_GAMEMODE_KEY) {
        game.input.f3_combo = true;
        if let Flow::InGame(ingame) = &mut game.flow
            && let Some(session) = ingame.net.session.as_mut()
            && session.player().is_some()
        {
            let target = match ingame.you.mode {
                GameMode::Creative => "s",
                GameMode::Survival => "c",
            };
            session.send(&ClientMessage::Chat {
                text: format!("/gm {target}"),
            });
        }
    }
    if io.keys.pressed(DEBUG_KEY) && io.keys.just_pressed(DEBUG_RENDERMODE_KEY) {
        game.input.f3_combo = true;
        game.cycle_render_mode();
    }
    if io.keys.just_released(DEBUG_KEY) {
        if !game.input.f3_combo {
            game.view_prefs.debug_overlay = !game.view_prefs.debug_overlay;
        }
        game.input.f3_combo = false;
    }
}

fn zoom_allowed(context: InputContext) -> bool {
    matches!(
        context,
        InputContext::Gameplay | InputContext::Inventory | InputContext::Chat
    )
}

fn context_hotkeys(game: &mut ClientGame, io: &IoFrame, context: InputContext) {
    match context {
        InputContext::Menu => {}
        InputContext::Connecting => {
            if io.keys.just_pressed(BACK_KEY) {
                game.leave_game();
            }
        }
        InputContext::Chat => {
            let Flow::InGame(ingame) = &mut game.flow else {
                return;
            };
            if io.keys.just_pressed(CHAT_KEY) {
                let text = io
                    .chat_text
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or_default()
                    .to_string();
                if !text.is_empty()
                    && let Some(session) = ingame.net.session.as_mut()
                {
                    session.send(&ClientMessage::Chat { text });
                }
                ingame.chat.open = false;
            }
            if io.keys.just_pressed(BACK_KEY) {
                ingame.chat.open = false;
            }
        }
        InputContext::Paused => {
            if io.keys.just_pressed(BACK_KEY)
                && let Flow::InGame(ingame) = &mut game.flow
            {
                ingame.set_paused(false, &mut game.input);
            }
        }
        InputContext::Inventory => {
            if (io.keys.just_pressed(INVENTORY_KEY) || io.keys.just_pressed(BACK_KEY))
                && let Flow::InGame(ingame) = &mut game.flow
            {
                ingame.inventory.open = false;
            }
        }
        InputContext::Gameplay => {
            let Flow::InGame(ingame) = &mut game.flow else {
                return;
            };
            if io.keys.just_pressed(BACK_KEY) {
                ingame.set_paused(true, &mut game.input);
                return;
            }
            if io.keys.just_pressed(INVENTORY_KEY) {
                ingame.inventory.open = true;
            }
            if io.keys.just_pressed(CHAT_KEY) {
                ingame.chat.open = true;
            }
        }
    }
}

pub(super) fn flush(game: &mut ClientGame, dt: f32) {
    let Flow::InGame(ingame) = &mut game.flow else {
        game.input.acc = 0.0;
        return;
    };
    if ingame.paused {
        game.input.acc = 0.0;
        return;
    }
    let input = &mut game.input;
    let session = ingame
        .net
        .session
        .as_mut()
        .filter(|session| session.player().is_some());
    let Some(session) = session else {
        input.acc = 0.0;
        input.actions.clear();
        input.latched = input.held;
        return;
    };
    input.acc = (input.acc + dt).min(MAX_CATCHUP_TICKS * TICK_DT);
    while input.acc >= TICK_DT {
        input.acc -= TICK_DT;
        session.send(&ClientMessage::Input(InputFrame {
            state: input.latched,
            actions: std::mem::take(&mut input.actions),
        }));
        input.latched = input.held;
    }
}
