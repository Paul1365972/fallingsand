use super::{BUTTON_BG, spawn_button};
use crate::game::ClientGame;
use crate::view::Game;
use crate::view::io::Btn;
use bevy::prelude::*;

#[derive(Component)]
pub(crate) struct SettingsRoot;

pub fn sync_settings(
    mut commands: Commands,
    game: Res<Game>,
    roots: Query<Entity, With<SettingsRoot>>,
    buttons: Query<(&Btn, &Children)>,
    mut texts: Query<&mut Text>,
) {
    let should_exist = game.0.settings_open;
    let exists = !roots.is_empty();
    if should_exist && !exists {
        spawn_settings(&mut commands, &game.0);
        return;
    }
    if !should_exist && exists {
        for entity in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }

    if game.0.changes.settings {
        for (button, children) in &buttons {
            let label = match button {
                Btn::ToggleFullscreen => fullscreen_label(game.0.settings.fullscreen),
                Btn::ToggleVsync => vsync_label(game.0.settings.vsync),
                Btn::CycleRenderMode => render_mode_label(&game.0),
                Btn::CycleUiScale => game.0.settings.ui_scale_label(),
                _ => continue,
            };
            for &child in children {
                if let Ok(mut text) = texts.get_mut(child) {
                    **text = label.clone();
                }
            }
        }
    }
}

fn fullscreen_label(on: bool) -> String {
    format!("Fullscreen: {}", if on { "on" } else { "off" })
}

fn vsync_label(on: bool) -> String {
    format!("VSync: {}", if on { "on" } else { "off" })
}

fn render_mode_label(game: &ClientGame) -> String {
    format!("Render mode: {}", game.settings.render_mode.label())
}

fn spawn_settings(commands: &mut Commands, game: &ClientGame) {
    let settings = &game.settings;
    commands
        .spawn((
            SettingsRoot,
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(10),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.06, 0.09, 0.96)),
            GlobalZIndex(20),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("settings"),
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
            #[cfg(not(target_family = "wasm"))]
            {
                spawn_button(
                    parent,
                    Btn::ToggleFullscreen,
                    &fullscreen_label(settings.fullscreen),
                    260.0,
                    BUTTON_BG,
                );
                spawn_button(
                    parent,
                    Btn::ToggleVsync,
                    &vsync_label(settings.vsync),
                    260.0,
                    BUTTON_BG,
                );
            }
            spawn_button(
                parent,
                Btn::CycleRenderMode,
                &render_mode_label(game),
                260.0,
                BUTTON_BG,
            );
            spawn_button(
                parent,
                Btn::CycleUiScale,
                &settings.ui_scale_label(),
                260.0,
                BUTTON_BG,
            );
            spawn_button(parent, Btn::SettingsBack, "Back", 260.0, BUTTON_BG);
        });
}
