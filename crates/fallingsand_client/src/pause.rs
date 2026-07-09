use crate::input::LocalAction;
use crate::menu::{BUTTON_BG, spawn_button};
#[cfg(not(target_family = "wasm"))]
use crate::net::embedded::EmbeddedServer;
use crate::{AppState, PauseState};
use bevy::prelude::*;

pub struct PausePlugin;

#[derive(Component)]
struct PauseRoot;

#[cfg_attr(target_family = "wasm", allow(dead_code))]
#[derive(Component, Clone, Copy)]
enum PauseButton {
    Resume,
    Save,
    QuitToMenu,
    QuitGame,
}

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, toggle_pause)
            .add_systems(OnEnter(PauseState::Paused), spawn_pause_menu)
            .add_systems(OnExit(PauseState::Paused), despawn_pause_menu)
            .add_systems(Update, handle_buttons.run_if(in_state(PauseState::Paused)));
        #[cfg(not(target_family = "wasm"))]
        app.add_systems(OnEnter(PauseState::Paused), pause_embedded)
            .add_systems(OnExit(PauseState::Paused), resume_embedded);
    }
}

fn toggle_pause(
    mut actions: MessageReader<LocalAction>,
    state: Option<Res<State<PauseState>>>,
    next: Option<ResMut<NextState<PauseState>>>,
) {
    let (Some(state), Some(mut next)) = (state, next) else {
        return;
    };
    for action in actions.read() {
        if *action != LocalAction::TogglePause {
            continue;
        }
        next.set(match state.get() {
            PauseState::Running => PauseState::Paused,
            PauseState::Paused => PauseState::Running,
        });
    }
}

#[cfg(not(target_family = "wasm"))]
fn pause_embedded(server: Option<Res<EmbeddedServer>>) {
    if let Some(server) = server {
        server.control.set_paused(true);
    }
}

#[cfg(not(target_family = "wasm"))]
fn resume_embedded(server: Option<Res<EmbeddedServer>>) {
    if let Some(server) = server {
        server.control.set_paused(false);
    }
}

fn spawn_pause_menu(
    mut commands: Commands,
    #[cfg(not(target_family = "wasm"))] server: Option<Res<EmbeddedServer>>,
) {
    #[cfg(not(target_family = "wasm"))]
    let singleplayer = server.is_some();
    #[cfg(target_family = "wasm")]
    let singleplayer = false;

    commands
        .spawn((
            PauseRoot,
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(10),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            GlobalZIndex(10),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("paused"),
                TextFont {
                    font_size: FontSize::Px(40.0),
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.8, 0.5)),
                Node {
                    margin: UiRect::bottom(px(16)),
                    ..default()
                },
            ));
            spawn_button(parent, PauseButton::Resume, "Resume", 240.0, BUTTON_BG);
            if singleplayer {
                spawn_button(parent, PauseButton::Save, "Save World", 240.0, BUTTON_BG);
            }
            let quit_label = if singleplayer {
                "Save & Quit to Menu"
            } else {
                "Quit to Menu"
            };
            spawn_button(
                parent,
                PauseButton::QuitToMenu,
                quit_label,
                240.0,
                BUTTON_BG,
            );
            #[cfg(not(target_family = "wasm"))]
            spawn_button(parent, PauseButton::QuitGame, "Quit Game", 240.0, BUTTON_BG);
        });
}

fn handle_buttons(
    query: Query<(&Interaction, &PauseButton), Changed<Interaction>>,
    mut next_pause: ResMut<NextState<PauseState>>,
    mut next_app: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
    #[cfg(not(target_family = "wasm"))] server: Option<Res<EmbeddedServer>>,
) {
    for (interaction, button) in &query {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button {
            PauseButton::Resume => next_pause.set(PauseState::Running),
            PauseButton::Save =>
            {
                #[cfg(not(target_family = "wasm"))]
                if let Some(server) = &server {
                    server.control.request_save();
                }
            }
            PauseButton::QuitToMenu => next_app.set(AppState::MainMenu),
            PauseButton::QuitGame => {
                exit.write(AppExit::Success);
            }
        }
    }
}

fn despawn_pause_menu(mut commands: Commands, query: Query<Entity, With<PauseRoot>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}
