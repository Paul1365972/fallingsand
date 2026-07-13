use super::Game;
use super::camera::{CameraState, cursor_to_world};
use super::ui::chat::ChatInput;
use super::ui::inventory::UiSlot;
use super::ui::menu::{CertField, NameField, PlayerNameField, UrlField};
use crate::game::input::{Button as InputButton, RawInput};
use crate::game::{Effect, IoFrame, UiEvent};
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use bevy::text::EditableText;
use bevy::window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode};
use fallingsand_core::CellPos;

#[derive(Resource, Default)]
pub struct UiInbox(pub Vec<UiEvent>);

const SCROLL_EPSILON: f32 = 0.01;

fn translate_input(
    keys: &ButtonInput<KeyCode>,
    buttons: &ButtonInput<MouseButton>,
    scroll: f32,
) -> RawInput {
    fn gather<'a>(
        keys: impl Iterator<Item = &'a KeyCode>,
        buttons: impl Iterator<Item = &'a MouseButton>,
    ) -> Vec<InputButton> {
        keys.map(|&key| InputButton::Key(key))
            .chain(buttons.map(|&button| InputButton::Mouse(button)))
            .collect()
    }

    let mut raw = RawInput {
        pressed: gather(keys.get_pressed(), buttons.get_pressed()),
        just_pressed: gather(keys.get_just_pressed(), buttons.get_just_pressed()),
        just_released: gather(keys.get_just_released(), buttons.get_just_released()),
    };
    if scroll > SCROLL_EPSILON {
        raw.just_pressed.push(InputButton::ScrollUp);
    } else if scroll < -SCROLL_EPSILON {
        raw.just_pressed.push(InputButton::ScrollDown);
    }
    raw
}

#[cfg_attr(target_family = "wasm", allow(dead_code))]
#[derive(Component, Clone)]
pub enum Btn {
    Play(String),
    Delete(String),
    Create,
    Connect,
    ToggleFullscreen,
    ToggleVsync,
    CycleRenderMode,
    CycleUiScale,
    CycleCursorMode,
    OpenSettings,
    SettingsBack,
    QuitApp,
    OpenGameMenu,
    CloseGameMenu,
    QuitToMenu,
    CancelConnect,
    Revive,
}

#[allow(clippy::too_many_arguments)]
pub fn collect_ui_events(
    buttons: Query<(&Interaction, &Btn), Changed<Interaction>>,
    slots: Query<(&UiSlot, &Interaction)>,
    mouse: Res<ButtonInput<MouseButton>>,
    player_name: Query<&EditableText, With<PlayerNameField>>,
    world_name: Query<&EditableText, With<NameField>>,
    url: Query<&EditableText, With<UrlField>>,
    cert: Query<&EditableText, With<CertField>>,
    mut inbox: ResMut<UiInbox>,
) {
    fn field<F: bevy::ecs::query::QueryFilter>(query: &Query<&EditableText, F>) -> String {
        query
            .single()
            .map(|editable| editable.value().to_string())
            .unwrap_or_default()
    }

    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button {
            Btn::Play(world) => {
                inbox.0.push(UiEvent::NameEdited(field(&player_name)));
                inbox.0.push(UiEvent::Play(world.clone()));
            }
            Btn::Create => {
                inbox.0.push(UiEvent::NameEdited(field(&player_name)));
                inbox.0.push(UiEvent::CreateWorld(field(&world_name)));
            }
            Btn::Connect => {
                inbox.0.push(UiEvent::NameEdited(field(&player_name)));
                inbox.0.push(UiEvent::Connect {
                    url: field(&url),
                    cert_hex: field(&cert),
                });
            }
            Btn::Delete(world) => inbox.0.push(UiEvent::DeleteWorld(world.clone())),
            Btn::ToggleFullscreen => inbox.0.push(UiEvent::ToggleFullscreen),
            Btn::ToggleVsync => inbox.0.push(UiEvent::ToggleVsync),
            Btn::CycleRenderMode => inbox.0.push(UiEvent::CycleRenderMode),
            Btn::CycleUiScale => inbox.0.push(UiEvent::CycleUiScale),
            Btn::CycleCursorMode => inbox.0.push(UiEvent::CycleCursorMode),
            Btn::OpenSettings => inbox.0.push(UiEvent::OpenSettings),
            Btn::SettingsBack => inbox.0.push(UiEvent::CloseSettings),
            Btn::QuitApp => inbox.0.push(UiEvent::QuitApp),
            Btn::OpenGameMenu => inbox.0.push(UiEvent::OpenGameMenu),
            Btn::CloseGameMenu => inbox.0.push(UiEvent::CloseGameMenu),
            Btn::QuitToMenu => inbox.0.push(UiEvent::QuitToMenu),
            Btn::CancelConnect => inbox.0.push(UiEvent::CancelConnect),
            Btn::Revive => inbox.0.push(UiEvent::Revive),
        }
    }

    let left = mouse.just_pressed(MouseButton::Left);
    let right = mouse.just_pressed(MouseButton::Right);
    if left || right {
        let hovered = slots
            .iter()
            .find(|(_, interaction)| !matches!(interaction, Interaction::None))
            .map(|(slot, _)| slot.0);
        if let Some(region) = hovered {
            inbox.0.push(UiEvent::Slot {
                region,
                right: !left,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn drive_game(
    mut game: ResMut<Game>,
    mut inbox: ResMut<UiInbox>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<MouseWheel>,
    camera_state: Res<CameraState>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut ui_scale: ResMut<UiScale>,
    interactions: Query<&Interaction>,
    chat_input: Query<&EditableText, With<ChatInput>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let cursor_cell = cursor_to_world(&window, &camera_state)
        .map(|world| CellPos::new(world.x.floor() as i32, world.y.floor() as i32));
    let over_ui = interactions
        .iter()
        .any(|interaction| !matches!(interaction, Interaction::None));
    let scroll: f32 = wheel.read().map(|event| event.y).sum();
    let chat_text = chat_input
        .single()
        .ok()
        .map(|editable| editable.value().to_string());

    let window_px = UVec2::new(
        window.physical_width().max(1),
        window.physical_height().max(1),
    );
    let mut io = IoFrame {
        dt: time.delta_secs().min(0.25),
        now: time.elapsed_secs(),
        raw: translate_input(&keys, &buttons, scroll),
        zoom_base: crate::view::camera::base_scale(window_px),
        cursor_cell,
        over_ui,
        chat_text,
        ui_events: std::mem::take(&mut inbox.0),
    };
    game.0.update(&mut io);

    for effect in std::mem::take(&mut game.0.effects) {
        match effect {
            Effect::Screenshot => {
                let path = chrono::Local::now()
                    .format("screenshot-%Y-%m-%d_%H-%M-%S.png")
                    .to_string();
                commands
                    .spawn(Screenshot::primary_window())
                    .observe(save_to_disk(path));
            }
            Effect::ApplyWindow => {
                let settings = &game.0.settings;
                window.mode = if settings.fullscreen {
                    WindowMode::BorderlessFullscreen(MonitorSelection::Current)
                } else {
                    WindowMode::Windowed
                };
                window.present_mode = if settings.vsync {
                    PresentMode::AutoVsync
                } else {
                    PresentMode::AutoNoVsync
                };
                ui_scale.0 = f32::from(settings.ui_scale.percent()) / 100.0;
            }
            Effect::Quit => {
                exit.write(AppExit::Success);
            }
        }
    }
}
