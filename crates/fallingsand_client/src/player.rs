use crate::AppState;
use crate::camera::WORLD_LAYER;
use crate::interpolation::Interpolated;
use crate::inventory::SelectedSlot;
use crate::net::{NetSet, ServerMsg, Session, SessionEnded, TickMessage};
use bevy::camera::visibility::RenderLayers;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use fallingsand_protocol::{GameMode, PlayerId, ServerMessage};

pub struct PlayerPlugin;

pub const PLAYER_SIZE: Vec2 = Vec2::new(3.8, 11.0);
pub const PLAYER_DUCK_SIZE: Vec2 = Vec2::new(3.8, 6.0);
const SNAP_DISTANCE: f32 = 64.0;

#[derive(Component)]
pub struct PlayerVisual {
    pub id: PlayerId,
    pub burning: bool,
}

#[derive(Component)]
struct NameTag(PlayerId);

#[derive(Resource, Default)]
pub struct PlayerVisuals(pub HashMap<PlayerId, Entity>);

#[derive(Resource, Default)]
pub struct PlayerNames(pub HashMap<PlayerId, String>);

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub struct LocalMode(pub GameMode);

#[derive(Resource, Default, Clone, Copy)]
pub struct LocalPlayerState {
    pub present: bool,
    pub pos: Vec2,
    pub hp: f32,
    pub air: f32,
    pub burning: bool,
    pub ducking: bool,
    pub mode: GameMode,
}

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerVisuals>()
            .init_resource::<PlayerNames>()
            .init_resource::<LocalPlayerState>()
            .init_resource::<LocalMode>()
            .add_systems(
                PreUpdate,
                (track_names, apply_players, apply_self_state)
                    .chain()
                    .after(NetSet),
            )
            .add_systems(
                Update,
                update_nametags.run_if(resource_changed::<PlayerNames>),
            )
            .add_systems(Update, cleanup_players.run_if(on_message::<SessionEnded>))
            .add_systems(OnExit(AppState::InGame), cleanup_players);
    }
}

fn track_names(
    mut commands: Commands,
    mut names: ResMut<PlayerNames>,
    mut visuals: ResMut<PlayerVisuals>,
    mut messages: MessageReader<ServerMsg>,
) {
    for ServerMsg(message) in messages.read() {
        match message {
            ServerMessage::PlayerJoined { player, name } => {
                names.0.insert(*player, name.clone());
            }
            ServerMessage::PlayerLeft { player } => {
                names.0.remove(player);
                if let Some(entity) = visuals.0.remove(player) {
                    commands.entity(entity).despawn();
                }
            }
            _ => {}
        }
    }
}

fn update_nametags(names: Res<PlayerNames>, mut tags: Query<(&NameTag, &mut Text2d)>) {
    for (tag, mut text) in &mut tags {
        let name = names.0.get(&tag.0).map(String::as_str).unwrap_or("");
        if text.0 != name {
            text.0 = name.to_string();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_players(
    mut commands: Commands,
    mut visuals: ResMut<PlayerVisuals>,
    mut frames: MessageReader<TickMessage>,
    mut query: Query<(&mut Interpolated, &mut Sprite, &mut PlayerVisual)>,
    session: Option<Res<Session>>,
    names: Res<PlayerNames>,
    mut local_state: ResMut<LocalPlayerState>,
) {
    let local = session.and_then(|session| session.player);
    for TickMessage(tick) in frames.read() {
        for state in &tick.players {
            if local == Some(state.player) {
                local_state.pos = Vec2::new(state.x.to_f32(), state.y.to_f32());
                local_state.burning = state.burning;
                local_state.ducking = state.ducking;
                local_state.present = true;
            }
            let target = Vec2::new(state.x.to_f32(), state.y.to_f32());
            let size = if state.ducking {
                PLAYER_DUCK_SIZE
            } else {
                PLAYER_SIZE
            };
            if let Some(&entity) = visuals.0.get(&state.player) {
                if let Ok((mut visual, mut sprite, mut marker)) = query.get_mut(entity) {
                    let snap = visual.target_position().distance_squared(target)
                        > SNAP_DISTANCE * SNAP_DISTANCE;
                    visual.record(target, 0.0, snap);
                    if sprite.custom_size != Some(size) {
                        sprite.custom_size = Some(size);
                    }
                    if marker.burning != state.burning {
                        marker.burning = state.burning;
                    }
                }
            } else {
                let is_local = local == Some(state.player);
                let color = if is_local {
                    Color::srgb(0.95, 0.75, 0.35)
                } else {
                    Color::srgb(0.55, 0.8, 0.95)
                };
                let entity = commands
                    .spawn((
                        PlayerVisual {
                            id: state.player,
                            burning: state.burning,
                        },
                        Interpolated::snapped(target, 0.0),
                        Sprite::from_color(color, size),
                        Transform::from_xyz(target.x, target.y, 10.0),
                        RenderLayers::layer(WORLD_LAYER),
                    ))
                    .id();
                if !is_local {
                    let name = names.0.get(&state.player).cloned().unwrap_or_default();
                    commands.entity(entity).with_child((
                        NameTag(state.player),
                        Text2d::new(name),
                        TextFont {
                            font_size: FontSize::Px(24.0),
                            ..default()
                        },
                        TextColor(Color::srgba(0.92, 0.95, 1.0, 0.9)),
                        Transform::from_xyz(0.0, PLAYER_SIZE.y / 2.0 + 5.0, 1.0)
                            .with_scale(Vec3::splat(0.25)),
                    ));
                }
                visuals.0.insert(state.player, entity);
            }
        }
    }
}

fn apply_self_state(
    mut frames: MessageReader<TickMessage>,
    mut mode: ResMut<LocalMode>,
    mut local_state: ResMut<LocalPlayerState>,
) {
    for TickMessage(tick) in frames.read() {
        if let Some(self_state) = tick.self_state {
            if mode.0 != self_state.mode {
                mode.0 = self_state.mode;
            }
            local_state.hp = self_state.hp;
            local_state.air = self_state.air;
            local_state.mode = self_state.mode;
        }
    }
}

fn cleanup_players(
    mut commands: Commands,
    mut visuals: ResMut<PlayerVisuals>,
    mut names: ResMut<PlayerNames>,
    mut mode: ResMut<LocalMode>,
    mut selected: ResMut<SelectedSlot>,
    mut local_state: ResMut<LocalPlayerState>,
) {
    for (_, entity) in visuals.0.drain() {
        commands.entity(entity).despawn();
    }
    names.0.clear();
    *mode = LocalMode::default();
    selected.0 = 0;
    *local_state = LocalPlayerState::default();
}
