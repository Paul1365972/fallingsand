mod bindings;
mod keys;

pub use bindings::{Action, Binding, Bindings, Context, Gesture, Layer};
pub use keys::{Button, RawInput};

use super::{ClientGame, Effect, Flow, GamePanel, IoFrame};
use bevy::input::mouse::MouseButton;
use fallingsand_core::{CellPos, HOTBAR_SLOTS, TICK_RATE, ray_cells};
use fallingsand_protocol::{
    ClientMessage, GameMode, InputAction, InputFrame, InputState, MAX_INPUT_ACTIONS_PER_FRAME,
    UseButton,
};

const DOUBLE_TAP_SECS: f32 = 0.3;
const TICK_DT: f32 = 1.0 / TICK_RATE as f32;
const MAX_CATCHUP_TICKS: f32 = 4.0;
const USE_REPEAT_INTERVAL_SECS: f32 = 0.05;
const MAX_SWEEP_CELLS: usize = 64;

struct Track {
    button: Button,
    pressed_at: f32,
    last_tap: f32,
    chord_used: bool,
    hold_fired: bool,
}

impl Track {
    fn new(button: Button) -> Self {
        Self {
            button,
            pressed_at: f32::NEG_INFINITY,
            last_tap: f32::NEG_INFINITY,
            chord_used: false,
            hold_fired: false,
        }
    }
}

#[derive(Default)]
struct UsePacer {
    held: bool,
    last_emit_at: f32,
    last_aim: CellPos,
}

#[derive(Default)]
pub struct InputCore {
    pub held: InputState,
    actions: Vec<InputAction>,
    blocked_primary: bool,
    blocked_secondary: bool,
    tracks: Vec<Track>,
    pacers: [UsePacer; 2],
    acc: f32,
    neutral_pending: bool,
}

impl InputCore {
    pub fn queue(&mut self, action: InputAction) {
        self.actions.push(action);
    }

    fn pace_use(&mut self, button: UseButton, held: bool, aim: CellPos, now: f32) {
        let pacer = &mut self.pacers[button as usize];
        if !held {
            pacer.held = false;
            return;
        }
        if !pacer.held {
            *pacer = UsePacer {
                held: true,
                last_emit_at: now,
                last_aim: aim,
            };
            self.actions.push(InputAction::Use { button, cell: aim });
            return;
        }
        if aim != pacer.last_aim {
            let start = pacer.last_aim;
            pacer.last_aim = aim;
            pacer.last_emit_at = now;
            for cell in ray_cells(start, aim).take(MAX_SWEEP_CELLS) {
                self.actions.push(InputAction::Use { button, cell });
            }
        } else if now - pacer.last_emit_at >= USE_REPEAT_INTERVAL_SECS {
            pacer.last_emit_at = now;
            self.actions.push(InputAction::Use { button, cell: aim });
        }
    }

    fn take_actions(&mut self) -> Vec<InputAction> {
        let count = self.actions.len().min(MAX_INPUT_ACTIONS_PER_FRAME);
        self.actions.drain(..count).collect()
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(super) fn neutralize(&mut self) {
        self.held = InputState {
            aim: self.held.aim,
            ..Default::default()
        };
        self.neutral_pending = true;
    }

    fn track(&mut self, button: Button) -> &mut Track {
        let index = match self.tracks.iter().position(|track| track.button == button) {
            Some(index) => index,
            None => {
                self.tracks.push(Track::new(button));
                self.tracks.len() - 1
            }
        };
        &mut self.tracks[index]
    }
}

fn visible_layers<'a>(bindings: &'a Bindings, stack: &[Context]) -> Vec<&'a Layer> {
    let mut layers = vec![&bindings.global];
    for &context in stack.iter().rev() {
        let layer = bindings.layer(context);
        layers.push(layer);
        if layer.opaque {
            break;
        }
    }
    layers
}

fn modifier_ok(binding: &Binding, raw: &RawInput) -> bool {
    binding.modifier.is_none_or(|modifier| modifier.held(raw))
}

fn push_unique(fired: &mut Vec<Action>, action: Action) {
    if !fired.contains(&action) {
        fired.push(action);
    }
}

pub(super) fn resolve(game: &mut ClientGame, io: &IoFrame) {
    let stack = game.input_contexts();
    let gameplay = stack.last() == Some(&Context::Gameplay);

    {
        let input = &mut game.input;
        let suppress = !gameplay || io.over_ui;
        if io.raw.is_just_pressed(MouseButton::Left) && suppress {
            input.blocked_primary = true;
        }
        if !io.raw.is_pressed(MouseButton::Left) {
            input.blocked_primary = false;
        }
        if io.raw.is_just_pressed(MouseButton::Right) && suppress {
            input.blocked_secondary = true;
        }
        if !io.raw.is_pressed(MouseButton::Right) {
            input.blocked_secondary = false;
        }
    }

    sample(game, io, gameplay);
    {
        let input = &mut game.input;
        let held = input.held;
        input.pace_use(UseButton::Primary, held.primary, held.aim, io.now);
        input.pace_use(UseButton::Secondary, held.secondary, held.aim, io.now);
    }
    let fired = collect(&game.bindings, &mut game.input, &io.raw, &stack, io.now);
    for action in fired {
        apply(game, io, action);
    }
    game.view_prefs.zoom_index = clamp_zoom(io.zoom_base, game.view_prefs.zoom_index);
}

fn sample(game: &mut ClientGame, io: &IoFrame, gameplay: bool) {
    let aim = io.cursor_cell.unwrap_or(game.input.held.aim);
    let mut state = InputState {
        aim,
        ..Default::default()
    };
    if gameplay {
        let held = |action: Action| {
            game.bindings.gameplay.bindings.iter().any(|binding| {
                binding.action == action
                    && binding.gesture == Gesture::Press
                    && binding.modifier.is_none()
                    && io.raw.is_pressed(binding.button)
            })
        };
        state.left = held(Action::MoveLeft);
        state.right = held(Action::MoveRight);
        state.jump = held(Action::Jump);
        state.down = held(Action::Duck);
        state.primary = held(Action::Primary) && !game.input.blocked_primary;
        state.secondary = held(Action::Secondary) && !game.input.blocked_secondary;
        state.cursor_mode = game.settings.cursor_mode;
    }
    game.input.held = state;
}

fn collect(
    bindings: &Bindings,
    input: &mut InputCore,
    raw: &RawInput,
    stack: &[Context],
    now: f32,
) -> Vec<Action> {
    let layers = visible_layers(bindings, stack);
    let matching = |button: Button| {
        layers
            .iter()
            .flat_map(|layer| layer.bindings.iter())
            .filter(move |binding| binding.button == button)
    };

    let mut fired = Vec::new();

    for &button in &raw.just_pressed {
        {
            let track = input.track(button);
            track.pressed_at = now;
            track.chord_used = false;
            track.hold_fired = false;
        }

        let mut chord_modifiers: Vec<Button> = Vec::new();
        for binding in matching(button).filter(|binding| binding.gesture == Gesture::Press) {
            if let Some(modifier) = binding.modifier
                && modifier.held(raw)
            {
                push_unique(&mut fired, binding.action);
                chord_modifiers.extend(modifier.buttons());
            }
        }
        if chord_modifiers.is_empty() {
            for binding in matching(button)
                .filter(|binding| binding.gesture == Gesture::Press && binding.modifier.is_none())
            {
                push_unique(&mut fired, binding.action);
            }
        }
        for modifier in chord_modifiers {
            input.track(modifier).chord_used = true;
        }

        let double: Vec<Action> = matching(button)
            .filter(|binding| binding.gesture == Gesture::DoubleTap && modifier_ok(binding, raw))
            .map(|binding| binding.action)
            .collect();
        if !double.is_empty() {
            let track = input.track(button);
            if now - track.last_tap < DOUBLE_TAP_SECS {
                track.last_tap = f32::NEG_INFINITY;
                for action in double {
                    push_unique(&mut fired, action);
                }
            } else {
                track.last_tap = now;
            }
        }
    }

    for &button in &raw.just_released {
        let track = input.track(button);
        let chord_used = track.chord_used;
        track.chord_used = false;
        if !chord_used {
            for binding in matching(button)
                .filter(|binding| binding.gesture == Gesture::Tap && modifier_ok(binding, raw))
            {
                push_unique(&mut fired, binding.action);
            }
        }
    }

    for track in &mut input.tracks {
        if track.hold_fired || !raw.is_pressed(track.button) {
            continue;
        }
        let held_secs = now - track.pressed_at;
        for binding in matching(track.button) {
            if let Gesture::Hold { secs } = binding.gesture
                && held_secs >= secs
                && modifier_ok(binding, raw)
            {
                push_unique(&mut fired, binding.action);
                track.hold_fired = true;
            }
        }
    }

    fired
}

fn apply(game: &mut ClientGame, io: &IoFrame, action: Action) {
    match action {
        Action::MoveLeft
        | Action::MoveRight
        | Action::Duck
        | Action::Primary
        | Action::Secondary => {}
        Action::Jump => game.input.queue(InputAction::Jump),
        Action::ToggleFlight => {
            if let Flow::InGame(ingame) = &game.flow
                && ingame.you.mode == GameMode::Creative
            {
                game.input.queue(InputAction::ToggleFlight);
            }
        }
        Action::SelectSlot(index) => {
            if let Flow::InGame(ingame) = &mut game.flow {
                ingame.inventory.selected = index as usize;
                game.input.queue(InputAction::SelectSlot(index));
            }
        }
        Action::SlotPrev | Action::SlotNext => {
            if let Flow::InGame(ingame) = &mut game.flow {
                let step = if action == Action::SlotPrev {
                    HOTBAR_SLOTS - 1
                } else {
                    1
                };
                ingame.inventory.selected = (ingame.inventory.selected + step) % HOTBAR_SLOTS;
                game.input
                    .queue(InputAction::SelectSlot(ingame.inventory.selected as u8));
            }
        }
        Action::OpenInventory => game.open_panel(GamePanel::Inventory),
        Action::OpenChat => game.open_panel(GamePanel::Chat),
        Action::OpenGameMenu => game.open_panel(GamePanel::GameMenu),
        Action::CloseOverlay => {
            if let Flow::InGame(ingame) = &mut game.flow
                && let Some(panel) = ingame.active_panel()
            {
                ingame.close_panel(panel);
            }
        }
        Action::SubmitChat => {
            if let Flow::InGame(ingame) = &mut game.flow {
                let text = io
                    .chat_text
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or_default()
                    .to_string();
                if !text.is_empty()
                    && let Some(session) = ingame.net.session.as_mut()
                {
                    ingame.chat.record(&text);
                    session.send(&ClientMessage::Chat { text });
                }
                ingame.close_panel(GamePanel::Chat);
            }
        }
        Action::HistoryPrev => {
            if let Flow::InGame(ingame) = &mut game.flow
                && ingame
                    .chat
                    .previous(io.chat_text.as_deref().unwrap_or_default())
            {
                game.changes.chat_draft = true;
            }
        }
        Action::HistoryNext => {
            if let Flow::InGame(ingame) = &mut game.flow
                && ingame.chat.next()
            {
                game.changes.chat_draft = true;
            }
        }
        Action::CloseGameMenu => {
            if let Flow::InGame(ingame) = &mut game.flow {
                ingame.close_panel(GamePanel::GameMenu);
            }
        }
        Action::CancelConnect => game.leave_game(),
        Action::CloseSettings => game.settings_open = false,
        Action::Screenshot => game.effects.push(Effect::Screenshot),
        Action::ToggleFullscreen => game.toggle_fullscreen(),
        Action::ZoomIn => game.view_prefs.zoom_index += 1,
        Action::ZoomOut => game.view_prefs.zoom_index -= 1,
        Action::ToggleDebugOverlay => {
            game.view_prefs.debug_overlay = !game.view_prefs.debug_overlay;
        }
        Action::ToggleDebugBorders => {
            game.view_prefs.debug_borders = !game.view_prefs.debug_borders;
        }
        Action::ToggleCursorMode => {
            game.settings.cycle_cursor_mode();
            super::settings::save(&game.settings);
            game.changes.settings = true;
        }
        Action::CycleGameMode => {
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
    }
}

pub fn clamp_zoom(base: u32, index: i32) -> i32 {
    let base = base as i32;
    index.clamp((base / 2).max(1) - base, base * 3)
}

pub(super) fn flush(game: &mut ClientGame, dt: f32) {
    let Flow::InGame(ingame) = &mut game.flow else {
        game.input.acc = 0.0;
        return;
    };
    let game_menu_open = ingame.game_menu_open();
    let input = &mut game.input;
    let session = ingame
        .net
        .session
        .as_mut()
        .filter(|session| session.player().is_some());
    let Some(session) = session else {
        input.acc = 0.0;
        input.actions.clear();
        input.neutral_pending = false;
        return;
    };
    if input.neutral_pending {
        session.send(&ClientMessage::Input(InputFrame {
            state: input.held,
            actions: input.take_actions(),
        }));
        input.neutral_pending = false;
        input.acc = 0.0;
    }
    if game_menu_open {
        input.acc = 0.0;
        return;
    }
    input.acc = (input.acc + dt).min(MAX_CATCHUP_TICKS * TICK_DT);
    while input.acc >= TICK_DT {
        input.acc -= TICK_DT;
        session.send(&ClientMessage::Input(InputFrame {
            state: input.held,
            actions: input.take_actions(),
        }));
    }
}
