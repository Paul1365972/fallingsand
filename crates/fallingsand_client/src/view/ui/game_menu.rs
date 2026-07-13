use super::{BUTTON_BG, spawn_button};
use crate::game::Phase;
use crate::view::Game;
use crate::view::io::Btn;
use bevy::prelude::*;

#[derive(Component)]
pub(crate) struct GameMenuRoot {
    can_save: bool,
}

pub fn sync_game_menu(
    mut commands: Commands,
    game: Res<Game>,
    roots: Query<(Entity, &GameMenuRoot)>,
) {
    let config = game
        .0
        .ingame()
        .filter(|ingame| ingame.phase == Phase::Playing && ingame.game_menu_open())
        .map(|ingame| ingame.net.is_embedded() && ingame.session_ready());
    let current = roots.iter().next().map(|(_, root)| root.can_save);
    if current == config && roots.iter().count() == usize::from(config.is_some()) {
        return;
    }
    for (entity, _) in &roots {
        commands.entity(entity).despawn();
    }
    if let Some(can_save) = config {
        spawn_game_menu(&mut commands, can_save);
    }
}

fn spawn_game_menu(commands: &mut Commands, can_save: bool) {
    commands
        .spawn((
            GameMenuRoot { can_save },
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
            GlobalZIndex(super::depth::GAME_MENU),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Game Menu"),
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
            spawn_button(parent, Btn::CloseGameMenu, "Resume", 240.0, BUTTON_BG);
            spawn_button(parent, Btn::OpenSettings, "Settings", 240.0, BUTTON_BG);
            let quit_label = if can_save {
                "Save & Quit to Menu"
            } else {
                "Quit to Menu"
            };
            spawn_button(parent, Btn::QuitToMenu, quit_label, 240.0, BUTTON_BG);
        });
}
