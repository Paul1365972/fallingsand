use crate::game::InGame;
use crate::view::Game;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::text::{EditableText, TextEdit};
use fallingsand_protocol::{CHAT_MAX_CHARS, ChatEntry, ChatKind, PlayerId};

const FADE_START: f32 = 6.0;
const FADE_END: f32 = 8.0;
const WIDTH: f32 = 520.0;
const FONT: f32 = 14.0;
const ROW_BG: Color = Color::srgba(0.05, 0.06, 0.09, 0.62);
const HINT_BG: Color = Color::srgba(0.05, 0.06, 0.09, 0.88);
const DIM: Color = Color::srgb(0.52, 0.56, 0.64);
const ACCENT: Color = Color::srgb(0.98, 0.86, 0.45);
const ERROR: Color = Color::srgb(0.95, 0.52, 0.45);

#[derive(Component)]
pub(crate) struct ChatRoot;

#[derive(Component)]
pub(crate) struct ChatLogPanel;

#[derive(Component)]
pub(crate) struct ChatHintPanel;

#[derive(Component)]
pub(crate) struct ChatRow(f32);

#[derive(Component)]
pub(crate) struct ChatHintRow;

#[derive(Component)]
pub struct ChatInput;

#[allow(clippy::too_many_arguments)]
pub fn sync_chat(
    mut commands: Commands,
    game: Res<Game>,
    mut focus: ResMut<InputFocus>,
    roots: Query<Entity, With<ChatRoot>>,
    log_panel: Query<Entity, With<ChatLogPanel>>,
    hint_panel: Query<Entity, With<ChatHintPanel>>,
    rows: Query<Entity, With<ChatRow>>,
    hints: Query<Entity, With<ChatHintRow>>,
    mut input: Query<(Entity, &mut EditableText), With<ChatInput>>,
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
    let open = ingame.chat_open();
    if roots.is_empty() {
        spawn_shell(&mut commands, ingame, open);
        return;
    }

    let field = input.single().map(|(entity, _)| entity).ok();
    let toggled = open != field.is_some();

    if (game.0.changes.chat || toggled)
        && let Ok(panel) = log_panel.single()
    {
        for row in &rows {
            commands.entity(row).despawn();
        }
        commands
            .entity(panel)
            .with_children(|parent| spawn_rows(parent, ingame, open));
    }
    if (game.0.changes.draft || toggled)
        && let Ok(panel) = hint_panel.single()
    {
        for hint in &hints {
            commands.entity(hint).despawn();
        }
        commands
            .entity(panel)
            .with_children(|parent| spawn_hints(parent, ingame, open));
    }
    if game.0.changes.draft_set
        && let Ok((_, mut editable)) = input.single_mut()
    {
        editable.editor_mut().set_text(&ingame.chat.composer.draft);
        editable.queue_edit(TextEdit::TextEnd(false));
    }

    match (open, field) {
        (true, None) => {
            let Ok(root) = roots.single() else {
                return;
            };
            let mut field = Entity::PLACEHOLDER;
            commands.entity(root).with_children(|parent| {
                field = spawn_input(parent);
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

fn spawn_shell(commands: &mut Commands, ingame: &InGame, open: bool) {
    let column = |gap: f32| Node {
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::FlexStart,
        row_gap: px(gap),
        ..default()
    };
    commands
        .spawn((
            ChatRoot,
            Node {
                position_type: PositionType::Absolute,
                left: px(8),
                bottom: px(8),
                ..column(3.0)
            },
            GlobalZIndex(super::depth::CHAT),
        ))
        .with_children(|root| {
            root.spawn((ChatLogPanel, column(2.0)))
                .with_children(|panel| spawn_rows(panel, ingame, open));
            root.spawn((ChatHintPanel, column(2.0)))
                .with_children(|panel| spawn_hints(panel, ingame, open));
        });
}

fn spawn_rows(parent: &mut ChildSpawnerCommands, ingame: &InGame, open: bool) {
    let me = ingame
        .net
        .session
        .as_ref()
        .and_then(|session| session.player());
    for (entry, at) in ingame.chat.log.visible(open) {
        spawn_row(parent, entry, *at, me);
    }
}

fn spawn_row(parent: &mut ChildSpawnerCommands, entry: &ChatEntry, at: f32, me: Option<PlayerId>) {
    let mut row = parent.spawn((ChatRow(at), text_row(ROW_BG)));
    if let Some((player, name)) = &entry.author {
        row.with_child(span(format!("{name}: "), author_color(*player, me)));
    }
    row.with_child(span(entry.text.clone(), kind_color(entry.kind)));
}

fn spawn_hints(parent: &mut ChildSpawnerCommands, ingame: &InGame, open: bool) {
    if !open {
        let unread = ingame.chat.log.unread;
        if unread > 0 {
            spawn_hint(
                parent,
                vec![(format!("{unread} new - Enter to read"), true)],
                ACCENT,
                DIM,
            );
        }
        return;
    }
    let composer = &ingame.chat.composer;
    let used = composer.draft.chars().count();
    if used * 4 > CHAT_MAX_CHARS * 3 {
        let over = used >= CHAT_MAX_CHARS;
        spawn_hint(
            parent,
            vec![(format!("{used}/{CHAT_MAX_CHARS}"), over)],
            ERROR,
            DIM,
        );
    }
    let Some(suggestion) = composer.suggestion() else {
        return;
    };
    if !suggestion.candidates.is_empty() {
        spawn_hint(parent, suggestion.candidates, ACCENT, DIM);
    }
    if !suggestion.line.is_empty() {
        let active = if suggestion.error {
            ERROR
        } else {
            Color::WHITE
        };
        spawn_hint(parent, suggestion.line, active, DIM);
    }
}

fn spawn_hint(
    parent: &mut ChildSpawnerCommands,
    parts: Vec<(String, bool)>,
    active: Color,
    rest: Color,
) {
    let mut row = parent.spawn((ChatHintRow, text_row(HINT_BG)));
    for (index, (text, emphasized)) in parts.into_iter().enumerate() {
        let text = match index {
            0 => text,
            _ => format!("  {text}"),
        };
        row.with_child(span(text, if emphasized { active } else { rest }));
    }
}

fn spawn_input(parent: &mut ChildSpawnerCommands) -> Entity {
    parent
        .spawn((
            ChatInput,
            EditableText::new(""),
            super::field_cursor_style(),
            font(),
            TextColor(Color::WHITE),
            Node {
                width: px(WIDTH),
                height: px(22),
                padding: UiRect::axes(px(5), px(2)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(HINT_BG),
        ))
        .id()
}

fn text_row(background: Color) -> impl Bundle {
    (
        Node {
            max_width: px(WIDTH),
            padding: UiRect::axes(px(5), px(1)),
            ..default()
        },
        BackgroundColor(background),
        Text::default(),
        font(),
    )
}

fn span(text: String, color: Color) -> impl Bundle {
    (TextSpan::new(text), font(), TextColor(color))
}

fn font() -> TextFont {
    TextFont {
        font_size: FontSize::Px(FONT),
        ..default()
    }
}

fn kind_color(kind: ChatKind) -> Color {
    match kind {
        ChatKind::Say => Color::srgb(0.95, 0.95, 0.95),
        ChatKind::System => Color::srgb(0.62, 0.78, 0.92),
        ChatKind::Error => ERROR,
        ChatKind::Announce => Color::srgb(0.92, 0.82, 0.5),
    }
}

fn author_color(player: PlayerId, me: Option<PlayerId>) -> Color {
    if me == Some(player) {
        return ACCENT;
    }
    Color::hsl((player.0.wrapping_mul(97) % 360) as f32, 0.5, 0.72)
}

pub fn fade_chat(
    game: Res<Game>,
    time: Res<Time>,
    mut rows: Query<(&ChatRow, &Children, &mut BackgroundColor)>,
    mut colors: Query<&mut TextColor>,
) {
    let open = game.0.ingame().is_some_and(|ingame| ingame.chat_open());
    let now = time.elapsed_secs();
    for (row, children, mut background) in &mut rows {
        let age = now - row.0;
        let fade = match open || age <= FADE_START {
            true => 1.0,
            false => (1.0 - (age - FADE_START) / (FADE_END - FADE_START)).max(0.0),
        };
        background.0.set_alpha(ROW_BG.alpha() * fade);
        for &child in children {
            if let Ok(mut color) = colors.get_mut(child) {
                color.0.set_alpha(fade);
            }
        }
    }
}
