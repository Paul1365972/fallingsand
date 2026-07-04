use crate::AppState;
use bevy::prelude::*;
use bevy::text::EditableText;

pub struct MenuPlugin;

#[derive(Resource, Clone)]
pub struct SelectedWorld(pub String);

impl Default for SelectedWorld {
    fn default() -> Self {
        Self("local".into())
    }
}

#[derive(Component)]
struct MenuRoot;

#[derive(Component)]
struct WorldsPanel;

#[derive(Component)]
struct WorldRow;

#[cfg_attr(target_family = "wasm", allow(dead_code))]
#[derive(Component, Clone)]
enum MenuButton {
    Play(String),
    Delete(String),
    Create,
    Connect,
    Quit,
}

#[derive(Component)]
struct NameField;

#[derive(Component)]
struct UrlField;

#[derive(Component)]
struct CertField;

#[derive(Resource, Default)]
struct WorldList(Vec<String>);

#[derive(Resource, Default)]
struct PendingDelete(Option<String>);

pub(crate) const BUTTON_BG: Color = Color::srgb(0.14, 0.16, 0.22);
pub(crate) const BUTTON_HOVER: Color = Color::srgb(0.22, 0.25, 0.33);
const DANGER_BG: Color = Color::srgb(0.35, 0.12, 0.12);
const FIELD_BG: Color = Color::srgb(0.09, 0.1, 0.15);
#[cfg_attr(target_family = "wasm", allow(dead_code))]
const PANEL_BG: Color = Color::srgb(0.07, 0.08, 0.12);

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedWorld>()
            .init_resource::<WorldList>()
            .init_resource::<PendingDelete>()
            .add_systems(OnEnter(AppState::MainMenu), (scan_worlds, spawn_menu))
            .add_systems(OnExit(AppState::MainMenu), despawn_menu)
            .add_systems(
                Update,
                (handle_buttons, sync_world_rows).run_if(in_state(AppState::MainMenu)),
            );
    }
}

fn scan_worlds(mut worlds: ResMut<WorldList>, mut pending: ResMut<PendingDelete>) {
    worlds.0 = list_worlds();
    pending.0 = None;
}

#[cfg(not(target_family = "wasm"))]
fn list_worlds() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir("saves") else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().join("world.redb").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

#[cfg(target_family = "wasm")]
fn list_worlds() -> Vec<String> {
    Vec::new()
}

fn spawn_menu(mut commands: Commands) {
    commands
        .spawn((
            MenuRoot,
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(10),
                ..default()
            },
            BackgroundColor(Color::srgb(0.05, 0.06, 0.09)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("fallingsand"),
                TextFont {
                    font_size: FontSize::Px(64.0),
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.8, 0.5)),
                Node {
                    margin: UiRect::bottom(px(24)),
                    ..default()
                },
            ));

            #[cfg(not(target_family = "wasm"))]
            {
                spawn_header(parent, "worlds");
                parent.spawn((
                    WorldsPanel,
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: px(6),
                        padding: UiRect::all(px(8)),
                        min_width: px(420),
                        ..default()
                    },
                    BackgroundColor(PANEL_BG),
                ));
                parent
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: px(6),
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        spawn_field(row, NameField, "new world", 220.0);
                        spawn_button(row, MenuButton::Create, "Create", 90.0, BUTTON_BG);
                    });
            }

            spawn_header(parent, "direct connect");
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexEnd,
                    row_gap: px(6),
                    ..default()
                })
                .with_children(|column| {
                    spawn_field(column, UrlField, "https://host:port", 320.0);
                    spawn_field(column, CertField, "cert sha256 (optional)", 320.0);
                    spawn_button(column, MenuButton::Connect, "Connect", 120.0, BUTTON_BG);
                });

            #[cfg(not(target_family = "wasm"))]
            spawn_button(parent, MenuButton::Quit, "Quit", 220.0, BUTTON_BG);
        });
}

fn spawn_header(parent: &mut ChildSpawnerCommands, label: &str) {
    parent.spawn((
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::srgba(0.7, 0.75, 0.8, 0.8)),
        Node {
            margin: UiRect::top(px(14)),
            ..default()
        },
    ));
}

fn spawn_field(parent: &mut ChildSpawnerCommands, marker: impl Component, label: &str, width: f32) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: px(8),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(Color::srgba(0.7, 0.75, 0.8, 0.7)),
            ));
            row.spawn((
                marker,
                EditableText::new(""),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    width: px(width),
                    height: px(26),
                    padding: UiRect::axes(px(6), px(3)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(FIELD_BG),
            ));
        });
}

pub(crate) fn spawn_button(
    parent: &mut ChildSpawnerCommands,
    action: impl Component,
    label: &str,
    width: f32,
    background: Color,
) {
    parent
        .spawn((
            action,
            Button,
            Node {
                width: px(width),
                height: px(30),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(background),
        ))
        .with_child((
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(15.0),
                ..default()
            },
            TextColor(Color::WHITE),
        ));
}

fn sync_world_rows(
    mut commands: Commands,
    worlds: Res<WorldList>,
    pending: Res<PendingDelete>,
    panel: Single<Entity, With<WorldsPanel>>,
    rows: Query<Entity, With<WorldRow>>,
) {
    if !worlds.is_changed() && !pending.is_changed() {
        return;
    }
    for row in &rows {
        commands.entity(row).despawn();
    }
    commands.entity(*panel).with_children(|panel| {
        if worlds.0.is_empty() {
            panel
                .spawn((
                    WorldRow,
                    Text::new("no worlds yet"),
                    TextFont {
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(Color::srgba(0.7, 0.75, 0.8, 0.6)),
                ))
                .insert(Node::default());
            return;
        }
        for name in &worlds.0 {
            panel
                .spawn((
                    WorldRow,
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: px(6),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::SpaceBetween,
                        width: percent(100),
                        ..default()
                    },
                ))
                .with_children(|row| {
                    row.spawn((
                        Text::new(name),
                        TextFont {
                            font_size: FontSize::Px(16.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                    row.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: px(6),
                        ..default()
                    })
                    .with_children(|buttons| {
                        spawn_button(
                            buttons,
                            MenuButton::Play(name.clone()),
                            "Play",
                            80.0,
                            BUTTON_BG,
                        );
                        let confirming = pending.0.as_deref() == Some(name);
                        spawn_button(
                            buttons,
                            MenuButton::Delete(name.clone()),
                            if confirming { "sure?" } else { "Delete" },
                            80.0,
                            if confirming { DANGER_BG } else { BUTTON_BG },
                        );
                    });
                });
        }
    });
}

fn sanitize_world_name(raw: &str) -> Option<String> {
    let name: String = raw
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ' '))
        .collect();
    let name = name.trim().to_string();
    (!name.is_empty()).then_some(name)
}

type ChangedButton = (Changed<Interaction>, With<Button>);

#[allow(clippy::too_many_arguments)]
fn handle_buttons(
    mut query: Query<(&Interaction, &MenuButton, &mut BackgroundColor), ChangedButton>,
    name_field: Query<&EditableText, With<NameField>>,
    url_field: Query<&EditableText, With<UrlField>>,
    cert_field: Query<&EditableText, With<CertField>>,
    mut selected: ResMut<SelectedWorld>,
    mut worlds: ResMut<WorldList>,
    mut pending_delete: ResMut<PendingDelete>,
    mut pending_connect: ResMut<crate::net::PendingConnect>,
    mut next_state: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, button, mut background) in &mut query {
        match interaction {
            Interaction::Pressed => match button {
                MenuButton::Play(name) => {
                    selected.0 = name.clone();
                    next_state.set(AppState::InGame);
                }
                MenuButton::Create => {
                    let raw = name_field.single().map(|f| f.value().to_string());
                    let Some(name) = raw.ok().as_deref().and_then(sanitize_world_name) else {
                        continue;
                    };
                    selected.0 = name;
                    next_state.set(AppState::InGame);
                }
                MenuButton::Delete(name) => {
                    if pending_delete.0.as_deref() == Some(name) {
                        delete_world(name);
                        pending_delete.0 = None;
                        worlds.0 = list_worlds();
                    } else {
                        pending_delete.0 = Some(name.clone());
                    }
                }
                MenuButton::Connect => {
                    let url = url_field
                        .single()
                        .map(|f| f.value().to_string())
                        .unwrap_or_default();
                    let url = url.trim().to_string();
                    if url.is_empty() {
                        continue;
                    }
                    let cert = cert_field
                        .single()
                        .map(|f| f.value().to_string())
                        .unwrap_or_default();
                    pending_connect.0 = Some(crate::net::ConnectTarget {
                        url,
                        cert_hash: crate::net::parse_cert_hash(cert.trim()),
                    });
                    next_state.set(AppState::InGame);
                }
                MenuButton::Quit => {
                    exit.write(AppExit::Success);
                }
            },
            Interaction::Hovered => *background = BackgroundColor(BUTTON_HOVER),
            Interaction::None => {
                let danger = matches!(button, MenuButton::Delete(name) if pending_delete.0.as_deref() == Some(name));
                *background = BackgroundColor(if danger { DANGER_BG } else { BUTTON_BG });
            }
        }
    }
}

#[cfg(not(target_family = "wasm"))]
fn delete_world(name: &str) {
    if sanitize_world_name(name).as_deref() != Some(name) {
        return;
    }
    let path = std::path::Path::new("saves").join(name);
    if path.join("world.redb").is_file()
        && let Err(err) = std::fs::remove_dir_all(&path)
    {
        error!("failed to delete world {name}: {err}");
    }
}

#[cfg(target_family = "wasm")]
fn delete_world(_name: &str) {}

fn despawn_menu(mut commands: Commands, query: Query<Entity, With<MenuRoot>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}
