use crate::game::ClientGame;
use crate::view::Game;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::text::EditableText;

const FADE_START: f32 = 6.0;
const FADE_END: f32 = 8.0;
const LINE_ALPHA: f32 = 0.9;

#[derive(Component)]
pub(crate) struct ChatRoot;

#[derive(Component)]
pub(crate) struct ChatLogPanel;

#[derive(Component)]
pub(crate) struct ChatLine(f32);

#[derive(Component)]
pub struct ChatInput;

#[allow(clippy::too_many_arguments)]
pub fn sync_chat(
    mut commands: Commands,
    game: Res<Game>,
    mut focus: ResMut<InputFocus>,
    roots: Query<Entity, With<ChatRoot>>,
    panel: Query<Entity, With<ChatLogPanel>>,
    rows: Query<Entity, With<ChatLine>>,
    input: Query<Entity, With<ChatInput>>,
) {
    let Some(ingame) = game.0.ingame() else {
        let had_input = !input.is_empty();
        for entity in &roots {
            commands.entity(entity).despawn();
        }
        if had_input {
            focus.clear();
        }
        return;
    };
    if roots.is_empty() {
        spawn_chat_ui(&mut commands, &game.0);
        return;
    }

    if game.0.changes.chat
        && let Ok(panel) = panel.single()
    {
        for row in &rows {
            commands.entity(row).despawn();
        }
        commands.entity(panel).with_children(|parent| {
            spawn_chat_rows(parent, &game.0);
        });
    }

    let field = input.single().ok();
    match (ingame.chat.open, field) {
        (true, None) => {
            let Ok(root) = roots.single() else {
                return;
            };
            let mut field = Entity::PLACEHOLDER;
            commands.entity(root).with_children(|parent| {
                field = spawn_chat_input(parent);
            });
            focus.set(field, FocusCause::Navigated);
        }
        (false, Some(entity)) => {
            commands.entity(entity).despawn();
            focus.clear();
        }
        _ => {}
    }
}

fn spawn_chat_ui(commands: &mut Commands, game: &ClientGame) {
    commands
        .spawn((
            ChatRoot,
            Node {
                position_type: PositionType::Absolute,
                left: px(8),
                bottom: px(8),
                flex_direction: FlexDirection::Column,
                row_gap: px(3),
                ..default()
            },
            GlobalZIndex(20),
        ))
        .with_children(|root| {
            root.spawn((
                ChatLogPanel,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(2),
                    ..default()
                },
            ))
            .with_children(|panel| {
                spawn_chat_rows(panel, game);
            });
        });
}

fn spawn_chat_rows(parent: &mut ChildSpawnerCommands, game: &ClientGame) {
    let Some(ingame) = game.ingame() else {
        return;
    };
    for (line, at) in &ingame.chat.log {
        parent.spawn((
            ChatLine(*at),
            Text::new(line.clone()),
            TextFont {
                font_size: FontSize::Px(14.0),
                ..default()
            },
            TextColor(Color::srgba(0.95, 0.95, 0.95, LINE_ALPHA)),
        ));
    }
}

fn spawn_chat_input(parent: &mut ChildSpawnerCommands) -> Entity {
    parent
        .spawn((
            ChatInput,
            EditableText::new(""),
            super::field_cursor_style(),
            TextFont {
                font_size: FontSize::Px(14.0),
                ..default()
            },
            TextColor(Color::WHITE),
            Node {
                width: px(360),
                height: px(22),
                padding: UiRect::axes(px(6), px(2)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.09, 0.1, 0.15, 0.9)),
        ))
        .id()
}

pub fn fade_chat(game: Res<Game>, time: Res<Time>, mut rows: Query<(&ChatLine, &mut TextColor)>) {
    let open = game.0.ingame().is_some_and(|ingame| ingame.chat.open);
    let now = time.elapsed_secs();
    for (line, mut color) in &mut rows {
        let alpha = if open {
            LINE_ALPHA
        } else {
            let age = now - line.0;
            if age <= FADE_START {
                LINE_ALPHA
            } else {
                (LINE_ALPHA * (1.0 - (age - FADE_START) / (FADE_END - FADE_START))).max(0.0)
            }
        };
        color.0.set_alpha(alpha);
    }
}
