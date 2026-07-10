use super::{BUTTON_BG, spawn_button};
use crate::game::{ClientGame, Flow, identity, net};
use crate::view::Game;
use crate::view::io::Btn;
use bevy::prelude::*;
use bevy::text::EditableText;

#[derive(Component)]
pub struct MenuRoot;

#[derive(Component)]
pub(crate) struct WorldsPanel;

#[derive(Component)]
pub(crate) struct WorldRow;

#[derive(Component)]
pub struct NameField;

#[derive(Component)]
pub struct UrlField;

#[derive(Component)]
pub struct CertField;

#[derive(Component)]
pub struct PlayerNameField;

const DANGER_BG: Color = Color::srgb(0.35, 0.12, 0.12);
const FIELD_BG: Color = Color::srgb(0.09, 0.1, 0.15);
#[cfg_attr(target_family = "wasm", allow(dead_code))]
const PANEL_BG: Color = Color::srgb(0.07, 0.08, 0.12);

#[allow(clippy::too_many_arguments)]
pub fn sync_menu(
    mut commands: Commands,
    game: Res<Game>,
    roots: Query<Entity, With<MenuRoot>>,
    panel: Query<Entity, With<WorldsPanel>>,
    rows: Query<Entity, With<WorldRow>>,
    toggles: Query<(&Btn, &Children)>,
    mut texts: Query<&mut Text>,
) {
    let active = matches!(game.0.flow, Flow::Menu);
    if !active {
        for entity in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }
    if roots.is_empty() {
        spawn_menu(&mut commands, &game.0);
        return;
    }

    if game.0.changes.worlds
        && let Ok(panel) = panel.single()
    {
        for row in &rows {
            commands.entity(row).despawn();
        }
        commands.entity(panel).with_children(|panel| {
            spawn_world_rows(panel, &game.0);
        });
    }

    if game.0.changes.settings {
        for (button, children) in &toggles {
            let label = match button {
                Btn::ToggleFullscreen => fullscreen_label(game.0.settings.fullscreen),
                Btn::ToggleVsync => vsync_label(game.0.settings.vsync),
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

fn spawn_menu(commands: &mut Commands, game: &ClientGame) {
    let player_name = identity::load_or_create().name;
    let settings = &game.settings;
    #[cfg(target_family = "wasm")]
    let _ = settings;
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

            spawn_header(parent, "player");
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(6),
                    ..default()
                })
                .with_children(|column| {
                    spawn_field(column, PlayerNameField, "name", 220.0, &player_name);
                    #[cfg(not(target_family = "wasm"))]
                    column
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: px(6),
                            ..default()
                        })
                        .with_children(|row| {
                            spawn_button(
                                row,
                                Btn::ToggleFullscreen,
                                &fullscreen_label(settings.fullscreen),
                                160.0,
                                BUTTON_BG,
                            );
                            spawn_button(
                                row,
                                Btn::ToggleVsync,
                                &vsync_label(settings.vsync),
                                160.0,
                                BUTTON_BG,
                            );
                        });
                });

            #[cfg(not(target_family = "wasm"))]
            {
                spawn_header(parent, "worlds");
                parent
                    .spawn((
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
                    ))
                    .with_children(|panel| {
                        spawn_world_rows(panel, game);
                    });
                parent
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: px(6),
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        spawn_field(row, NameField, "new world", 220.0, "");
                        spawn_button(row, Btn::Create, "Create", 90.0, BUTTON_BG);
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
                    spawn_field(
                        column,
                        UrlField,
                        "host[:port]",
                        320.0,
                        &net::default_server(),
                    );
                    spawn_field(column, CertField, "cert sha256 (optional)", 320.0, "");
                    spawn_button(column, Btn::Connect, "Connect", 120.0, BUTTON_BG);
                });

            #[cfg(not(target_family = "wasm"))]
            spawn_button(parent, Btn::QuitApp, "Quit", 220.0, BUTTON_BG);
        });
}

fn spawn_world_rows(panel: &mut ChildSpawnerCommands, game: &ClientGame) {
    if game.menu.worlds.is_empty() {
        panel.spawn((
            WorldRow,
            Text::new("no worlds yet"),
            TextFont {
                font_size: FontSize::Px(13.0),
                ..default()
            },
            TextColor(Color::srgba(0.7, 0.75, 0.8, 0.6)),
        ));
        return;
    }
    for name in &game.menu.worlds {
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
                    spawn_button(buttons, Btn::Play(name.clone()), "Play", 80.0, BUTTON_BG);
                    let confirming = game.menu.pending_delete.as_deref() == Some(name);
                    spawn_button(
                        buttons,
                        Btn::Delete(name.clone()),
                        if confirming { "sure?" } else { "Delete" },
                        80.0,
                        if confirming { DANGER_BG } else { BUTTON_BG },
                    );
                });
            });
    }
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

fn spawn_field(
    parent: &mut ChildSpawnerCommands,
    marker: impl Component,
    label: &str,
    width: f32,
    initial: &str,
) {
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
                EditableText::new(initial),
                super::field_cursor_style(),
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
