use crate::input::LocalAction;
use crate::menu::{BUTTON_BG, ButtonBase};
use crate::net::{ConnPhase, Session, Supervisor};
use crate::{AppState, GameState, PauseState};
use bevy::prelude::*;

pub struct ConnScreenPlugin;

#[derive(Component)]
struct ScreenRoot;

#[derive(Component)]
struct TitleText;

#[derive(Component)]
struct DotsText;

#[derive(Component)]
struct DetailText;

#[derive(Component)]
struct ErrorText;

#[derive(Component)]
struct CancelButton;

#[derive(Component)]
struct CancelLabel;

impl Plugin for ConnScreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_screen)
            .add_systems(OnExit(AppState::InGame), despawn_screen)
            .add_systems(
                Update,
                (update_screen, handle_cancel).run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                cancel_on_esc.run_if(in_state(GameState::Connecting)),
            );
    }
}

fn spawn_screen(mut commands: Commands) {
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

fn despawn_screen(mut commands: Commands, query: Query<Entity, With<ScreenRoot>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

fn set_text(text: &mut Text, value: String) {
    if **text != value {
        **text = value;
    }
}

#[allow(clippy::too_many_arguments)]
fn update_screen(
    supervisor: Res<Supervisor>,
    session: Option<Res<Session>>,
    game_state: Res<State<GameState>>,
    pause: Option<Res<State<PauseState>>>,
    time: Res<Time>,
    mut root: Single<(&mut Visibility, &mut BackgroundColor), With<ScreenRoot>>,
    title: Single<Entity, With<TitleText>>,
    dots: Single<Entity, With<DotsText>>,
    detail: Single<Entity, With<DetailText>>,
    error: Single<Entity, With<ErrorText>>,
    label: Single<Entity, With<CancelLabel>>,
    mut button: Single<&mut Visibility, (With<CancelButton>, Without<ScreenRoot>)>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    let (visibility, backdrop) = &mut *root;
    let connecting = *game_state.get() == GameState::Connecting;
    let paused = pause.is_some_and(|state| *state.get() == PauseState::Paused);
    let phase = supervisor.phase(session.as_deref(), paused);
    let server = supervisor
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
                **visibility = Visibility::Hidden;
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
    **visibility = Visibility::Inherited;
    backdrop.0 = if connecting {
        Color::srgba(0.05, 0.06, 0.09, 1.0)
    } else {
        Color::srgba(0.02, 0.03, 0.06, alpha)
    };
    if let Ok((mut text, mut color)) = texts.get_mut(*title) {
        set_text(&mut text, title_str);
        color.0 = title_color;
    }
    if let Ok((mut text, _)) = texts.get_mut(*dots) {
        let count = if animate {
            1 + (time.elapsed_secs() * 2.0) as usize % 3
        } else {
            0
        };
        set_text(&mut text, "● ".repeat(count).trim_end().to_string());
    }
    if let Ok((mut text, _)) = texts.get_mut(*detail) {
        set_text(&mut text, detail_str);
    }
    if let Ok((mut text, _)) = texts.get_mut(*error) {
        let error_str = match (&phase, &supervisor.last_error) {
            (ConnPhase::Lost { .. }, _) | (_, None) => String::new(),
            (_, Some(err)) if connecting => format!("last error: {err}"),
            _ => String::new(),
        };
        set_text(&mut text, error_str);
    }
    **button = if button_str.is_some() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if let (Ok((mut text, _)), Some(button_str)) = (texts.get_mut(*label), button_str) {
        set_text(&mut text, button_str.to_string());
    }
}

fn handle_cancel(
    buttons: Query<&Interaction, (Changed<Interaction>, With<CancelButton>)>,
    mut next: ResMut<NextState<AppState>>,
) {
    for interaction in &buttons {
        if *interaction == Interaction::Pressed {
            next.set(AppState::MainMenu);
        }
    }
}

fn cancel_on_esc(mut actions: MessageReader<LocalAction>, mut next: ResMut<NextState<AppState>>) {
    for action in actions.read() {
        if *action == LocalAction::CancelConnect {
            next.set(AppState::MainMenu);
        }
    }
}
