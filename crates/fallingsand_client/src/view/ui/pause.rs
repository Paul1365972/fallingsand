use super::{BUTTON_BG, spawn_button};
use crate::game::Phase;
use crate::view::Game;
use crate::view::io::Btn;
use bevy::prelude::*;

#[derive(Component)]
pub(crate) struct PauseRoot;

pub fn sync_pause(mut commands: Commands, game: Res<Game>, roots: Query<Entity, With<PauseRoot>>) {
    let should_exist = game
        .0
        .ingame()
        .is_some_and(|ingame| ingame.phase == Phase::Playing && ingame.paused());
    let exists = !roots.is_empty();
    if should_exist && !exists {
        let singleplayer = game
            .0
            .ingame()
            .is_some_and(|ingame| ingame.net.is_embedded());
        spawn_pause_menu(&mut commands, singleplayer);
    } else if !should_exist && exists {
        for entity in &roots {
            commands.entity(entity).despawn();
        }
    }
}

fn spawn_pause_menu(commands: &mut Commands, singleplayer: bool) {
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
            spawn_button(parent, Btn::PauseResume, "Resume", 240.0, BUTTON_BG);
            spawn_button(parent, Btn::OpenSettings, "Settings", 240.0, BUTTON_BG);
            if singleplayer {
                spawn_button(parent, Btn::PauseSave, "Save World", 240.0, BUTTON_BG);
            }
            let quit_label = if singleplayer {
                "Save & Quit to Menu"
            } else {
                "Quit to Menu"
            };
            spawn_button(parent, Btn::PauseQuitToMenu, quit_label, 240.0, BUTTON_BG);
            #[cfg(not(target_family = "wasm"))]
            spawn_button(parent, Btn::QuitApp, "Quit Game", 240.0, BUTTON_BG);
        });
}
