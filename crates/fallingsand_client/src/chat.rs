use crate::input::LocalAction;
use crate::net::{NetSet, ServerMsg, Session};
use crate::{AppState, PauseState};
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::text::EditableText;
use fallingsand_protocol::{ClientMessage, ServerMessage};

pub struct ChatPlugin;

const LOG_CAP: usize = 8;
const FADE_START: f32 = 6.0;
const FADE_END: f32 = 8.0;
const LINE_ALPHA: f32 = 0.9;

#[derive(Resource, Default)]
pub struct ChatOpen(pub bool);

#[derive(Resource, Default)]
struct ChatLog(Vec<(String, f32)>);

#[derive(Component)]
struct ChatRoot;

#[derive(Component)]
struct ChatLogPanel;

#[derive(Component)]
struct ChatLine(f32);

#[derive(Component)]
struct ChatInput;

impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatOpen>()
            .init_resource::<ChatLog>()
            .add_systems(OnEnter(AppState::InGame), spawn_chat_ui)
            .add_systems(OnExit(AppState::InGame), teardown)
            .add_systems(
                PreUpdate,
                collect_chat
                    .after(NetSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    toggle_chat.run_if(in_state(PauseState::Running)),
                    (render_chat, fade_chat)
                        .chain()
                        .run_if(in_state(AppState::InGame)),
                ),
            );
    }
}

fn spawn_chat_ui(mut commands: Commands) {
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
            ));
        });
}

fn teardown(
    mut commands: Commands,
    root: Query<Entity, With<ChatRoot>>,
    mut log: ResMut<ChatLog>,
    mut open: ResMut<ChatOpen>,
) {
    for entity in &root {
        commands.entity(entity).despawn();
    }
    log.0.clear();
    open.0 = false;
}

fn collect_chat(mut log: ResMut<ChatLog>, mut messages: MessageReader<ServerMsg>, time: Res<Time>) {
    for ServerMsg(message) in messages.read() {
        let line = match message {
            ServerMessage::Chat { name, text, .. } => format!("{name}: {text}"),
            ServerMessage::System { text } => text.clone(),
            _ => continue,
        };
        log.0.push((line, time.elapsed_secs()));
        if log.0.len() > LOG_CAP {
            let excess = log.0.len() - LOG_CAP;
            log.0.drain(..excess);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn toggle_chat(
    mut commands: Commands,
    mut actions: MessageReader<LocalAction>,
    mut open: ResMut<ChatOpen>,
    mut focus: ResMut<InputFocus>,
    mut session: Option<ResMut<Session>>,
    input: Query<(Entity, &EditableText), With<ChatInput>>,
    root: Query<Entity, With<ChatRoot>>,
) {
    for action in actions.read() {
        match action {
            LocalAction::SubmitChat | LocalAction::CancelChat if open.0 => {
                if let Ok((entity, editable)) = input.single() {
                    if *action == LocalAction::SubmitChat {
                        let text = editable.value().to_string();
                        let text = text.trim().to_string();
                        if !text.is_empty()
                            && let Some(session) = session.as_mut()
                        {
                            session.send(&ClientMessage::Chat { text });
                        }
                    }
                    commands.entity(entity).despawn();
                }
                open.0 = false;
                focus.clear();
            }
            LocalAction::OpenChat if !open.0 => {
                let Ok(root) = root.single() else {
                    continue;
                };
                let mut field = Entity::PLACEHOLDER;
                commands.entity(root).with_children(|parent| {
                    field = parent
                        .spawn((
                            ChatInput,
                            EditableText::new(""),
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
                        .id();
                });
                open.0 = true;
                focus.set(field, FocusCause::Navigated);
            }
            _ => {}
        }
    }
}

fn render_chat(
    mut commands: Commands,
    log: Res<ChatLog>,
    panel: Query<Entity, With<ChatLogPanel>>,
    rows: Query<Entity, With<ChatLine>>,
) {
    if !log.is_changed() {
        return;
    }
    let Ok(panel) = panel.single() else {
        return;
    };
    for row in &rows {
        commands.entity(row).despawn();
    }
    commands.entity(panel).with_children(|parent| {
        for (line, at) in &log.0 {
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
    });
}

fn fade_chat(open: Res<ChatOpen>, time: Res<Time>, mut rows: Query<(&ChatLine, &mut TextColor)>) {
    let now = time.elapsed_secs();
    for (line, mut color) in &mut rows {
        let alpha = if open.0 {
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
