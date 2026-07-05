use crate::net::{ConnPhase, Session, Supervisor};
use crate::{AppState, PauseState};
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

impl Plugin for ConnScreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_screen)
            .add_systems(OnExit(AppState::InGame), despawn_screen)
            .add_systems(Update, update_screen.run_if(in_state(AppState::InGame)));
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
        });
}

fn despawn_screen(mut commands: Commands, query: Query<Entity, With<ScreenRoot>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

type TitleQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Text, &'static mut TextColor),
    (With<TitleText>, Without<DotsText>, Without<DetailText>),
>;
type DotsQuery<'w, 's> =
    Query<'w, 's, &'static mut Text, (With<DotsText>, Without<TitleText>, Without<DetailText>)>;
type DetailQuery<'w, 's> =
    Query<'w, 's, &'static mut Text, (With<DetailText>, Without<TitleText>, Without<DotsText>)>;

#[allow(clippy::too_many_arguments)]
fn update_screen(
    supervisor: Res<Supervisor>,
    session: Option<Res<Session>>,
    pause: Option<Res<State<PauseState>>>,
    time: Res<Time>,
    mut root: Query<(&mut Visibility, &mut BackgroundColor), With<ScreenRoot>>,
    mut title: TitleQuery,
    mut dots: DotsQuery,
    mut detail: DetailQuery,
) {
    let Ok((mut visibility, mut backdrop)) = root.single_mut() else {
        return;
    };
    let paused = pause.is_some_and(|state| *state.get() == PauseState::Paused);
    let phase = supervisor.phase(session.as_deref(), paused);
    let server = supervisor
        .target
        .as_ref()
        .map(|target| target.url.clone())
        .unwrap_or_else(|| "local server".into());
    let (title_str, title_color, detail_str, animate, alpha) = match &phase {
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
        ),
        ConnPhase::Reconnecting { attempt } => (
            "reconnecting".to_string(),
            Color::srgb(0.95, 0.6, 0.3),
            format!("{server} — attempt {attempt}"),
            true,
            0.85,
        ),
        ConnPhase::Stalled { seconds } => (
            "connection unstable".to_string(),
            Color::srgb(0.95, 0.6, 0.3),
            format!("no data from {server} for {seconds:.1}s"),
            true,
            0.4,
        ),
        ConnPhase::Lost { reason } => (
            "connection lost".to_string(),
            Color::srgb(0.9, 0.3, 0.3),
            format!("{reason} — press Esc for menu"),
            false,
            0.85,
        ),
    };
    *visibility = Visibility::Inherited;
    backdrop.0 = Color::srgba(0.02, 0.03, 0.06, alpha);
    if let Ok((mut text, mut color)) = title.single_mut() {
        if **text != title_str {
            **text = title_str;
        }
        color.0 = title_color;
    }
    if let Ok(mut text) = dots.single_mut() {
        let count = if animate {
            1 + (time.elapsed_secs() * 2.0) as usize % 3
        } else {
            0
        };
        let dots_str = "● ".repeat(count).trim_end().to_string();
        if **text != dots_str {
            **text = dots_str;
        }
    }
    if let Ok(mut text) = detail.single_mut()
        && **text != detail_str
    {
        **text = detail_str;
    }
}
