use super::{BUTTON_BG, ButtonBase, set_text};
use crate::game::Phase;
use crate::game::net::ConnPhase;
use crate::view::Game;
use crate::view::io::Btn;
use bevy::prelude::*;

#[derive(Component)]
pub(crate) struct ScreenRoot;

#[derive(Component)]
pub(crate) struct TitleText;

#[derive(Component)]
pub(crate) struct DotsText;

#[derive(Component)]
pub(crate) struct DetailText;

#[derive(Component)]
pub(crate) struct ErrorText;

#[derive(Component)]
pub(crate) struct CancelButton;

#[derive(Component)]
pub(crate) struct CancelLabel;

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
pub fn sync_connscreen(
    mut commands: Commands,
    game: Res<Game>,
    time: Res<Time>,
    roots: Query<Entity, With<ScreenRoot>>,
    mut root: Query<(&mut Visibility, &mut BackgroundColor), With<ScreenRoot>>,
    title: Query<Entity, With<TitleText>>,
    dots: Query<Entity, With<DotsText>>,
    detail: Query<Entity, With<DetailText>>,
    error: Query<Entity, With<ErrorText>>,
    label: Query<Entity, With<CancelLabel>>,
    mut button: Query<&mut Visibility, (With<CancelButton>, Without<ScreenRoot>)>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    let Some(ingame) = game.0.ingame() else {
        for entity in &roots {
            commands.entity(entity).despawn();
        }
        return;
    };
    if roots.is_empty() {
        spawn_screen(&mut commands);
        return;
    }

    let Ok((mut visibility, mut backdrop)) = root.single_mut() else {
        return;
    };
    let connecting = ingame.phase == Phase::Connecting;
    let phase = ingame
        .net
        .supervisor
        .phase(ingame.net.session.as_ref(), ingame.paused);
    let server = ingame
        .net
        .supervisor
        .target
        .as_ref()
        .map(|target| target.url.clone())
        .unwrap_or_else(|| {
            if connecting {
                "starting local server".into()
            } else {
                "local server".into()
            }
        });

    let (title_str, title_color, detail_str, animate, alpha, button_str) = if connecting {
        match &phase {
            ConnPhase::Lost { reason } => (
                "connection failed".to_string(),
                Color::srgb(0.9, 0.3, 0.3),
                reason.clone(),
                false,
                1.0,
                Some("Cancel"),
            ),
            ConnPhase::Reconnecting { attempt } => (
                "connecting".to_string(),
                Color::srgb(0.9, 0.8, 0.5),
                format!("{server} — attempt {attempt}"),
                true,
                1.0,
                Some("Cancel"),
            ),
            _ => (
                "connecting".to_string(),
                Color::srgb(0.9, 0.8, 0.5),
                server,
                true,
                1.0,
                Some("Cancel"),
            ),
        }
    } else {
        match &phase {
            ConnPhase::Online => {
                *visibility = Visibility::Hidden;
                return;
            }
            ConnPhase::Connecting => (
                "connecting".to_string(),
                Color::srgb(0.9, 0.8, 0.5),
                server,
                true,
                0.85,
                None,
            ),
            ConnPhase::Reconnecting { attempt } => (
                "reconnecting".to_string(),
                Color::srgb(0.95, 0.6, 0.3),
                format!("{server} — attempt {attempt}"),
                true,
                0.85,
                None,
            ),
            ConnPhase::Stalled { seconds } => (
                "connection unstable".to_string(),
                Color::srgb(0.95, 0.6, 0.3),
                format!("no data from {server} for {seconds:.1}s"),
                true,
                0.4,
                None,
            ),
            ConnPhase::Lost { reason } => (
                "connection lost".to_string(),
                Color::srgb(0.9, 0.3, 0.3),
                format!("{reason} — press Esc for menu"),
                false,
                0.85,
                Some("Back to Menu"),
            ),
        }
    };
    *visibility = Visibility::Inherited;
    backdrop.0 = if connecting {
        Color::srgba(0.05, 0.06, 0.09, 1.0)
    } else {
        Color::srgba(0.02, 0.03, 0.06, alpha)
    };
    if let Ok(entity) = title.single()
        && let Ok((mut text, mut color)) = texts.get_mut(entity)
    {
        set_text(&mut text, title_str);
        color.0 = title_color;
    }
    if let Ok(entity) = dots.single()
        && let Ok((mut text, _)) = texts.get_mut(entity)
    {
        let count = if animate {
            1 + (time.elapsed_secs() * 2.0) as usize % 3
        } else {
            0
        };
        set_text(&mut text, "● ".repeat(count).trim_end().to_string());
    }
    if let Ok(entity) = detail.single()
        && let Ok((mut text, _)) = texts.get_mut(entity)
    {
        set_text(&mut text, detail_str);
    }
    if let Ok(entity) = error.single()
        && let Ok((mut text, _)) = texts.get_mut(entity)
    {
        let error_str = match (&phase, &ingame.net.supervisor.last_error) {
            (ConnPhase::Lost { .. }, _) | (_, None) => String::new(),
            (_, Some(err)) if connecting => format!("last error: {err}"),
            _ => String::new(),
        };
        set_text(&mut text, error_str);
    }
    if let Ok(mut button_visibility) = button.single_mut() {
        *button_visibility = if button_str.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if let Ok(entity) = label.single()
        && let (Ok((mut text, _)), Some(button_str)) = (texts.get_mut(entity), button_str)
    {
        set_text(&mut text, button_str.to_string());
    }
}

fn spawn_screen(commands: &mut Commands) {
    commands
        .spawn((
            ScreenRoot,
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(12),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.03, 0.06, 0.85)),
            GlobalZIndex(5),
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            parent.spawn((
                TitleText,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(40.0),
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.8, 0.5)),
                Pickable::IGNORE,
            ));
            parent.spawn((
                DotsText,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(24.0),
                    ..default()
                },
                TextColor(Color::srgba(0.9, 0.9, 0.9, 0.8)),
                Pickable::IGNORE,
            ));
            parent.spawn((
                DetailText,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgba(0.75, 0.78, 0.85, 0.9)),
                Pickable::IGNORE,
            ));
            parent.spawn((
                ErrorText,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgba(0.85, 0.45, 0.45, 0.9)),
                Pickable::IGNORE,
            ));
            parent
                .spawn((
                    CancelButton,
                    Btn::CancelConnect,
                    Button,
                    ButtonBase(BUTTON_BG),
                    Node {
                        width: px(160),
                        height: px(30),
                        margin: UiRect::top(px(18)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(BUTTON_BG),
                ))
                .with_child((
                    CancelLabel,
                    Text::new("Cancel"),
                    TextFont {
                        font_size: FontSize::Px(15.0),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
        });
}
