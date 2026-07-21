use super::Game;
use super::camera::CameraState;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use fallingsand_protocol::PlayerId;

const NAMETAG_RISE: f32 = 3.0;

#[derive(Component)]
pub struct NameTag(PlayerId);

#[derive(Resource, Default)]
pub struct NametagVisuals(HashMap<PlayerId, Entity>);

pub fn sync_nametags(
    mut commands: Commands,
    game: Res<Game>,
    state: Res<CameraState>,
    ui_scale: Res<UiScale>,
    window: Single<&Window>,
    mut visuals: ResMut<NametagVisuals>,
    mut tags: Query<(
        &NameTag,
        &mut Text,
        &mut TextFont,
        &mut Node,
        &mut Visibility,
    )>,
) {
    let ingame = game.0.ingame();
    let local = ingame.and_then(|ingame| ingame.net.session.as_ref()?.player());

    visuals.0.retain(|player, entity| {
        let live = ingame.is_some_and(|ingame| ingame.players.avatars.contains_key(player))
            && Some(*player) != local;
        if !live {
            commands.entity(*entity).despawn();
        }
        live
    });

    let Some(ingame) = ingame else {
        return;
    };
    for (&player, remote) in &ingame.players.avatars {
        if Some(player) == local {
            continue;
        }
        let world = Vec2::new(remote.pos.x, remote.top_y() + NAMETAG_RISE);
        let px = ((world - state.pos) * state.k as f32).round();
        let scale_factor = window.scale_factor();
        let ui_scale = ui_scale.0.max(f32::EPSILON);
        let left = (window.width() * 0.5 + px.x / scale_factor) / ui_scale;
        let top = (window.height() * 0.5 - px.y / scale_factor) / ui_scale;
        let font_size = FontSize::Px(16.0);
        if let Some(&entity) = visuals.0.get(&player) {
            if let Ok((tag, mut text, mut font, mut node, mut visibility)) = tags.get_mut(entity) {
                node.left = Val::Px(left);
                node.top = Val::Px(top);
                font.font_size = font_size;
                *visibility = Visibility::Inherited;
                if game.0.changes.roster {
                    let name = ingame
                        .players
                        .names
                        .get(&tag.0)
                        .map(String::as_str)
                        .unwrap_or("");
                    if **text != name {
                        **text = name.to_string();
                    }
                }
            }
        } else {
            let name = ingame
                .players
                .names
                .get(&player)
                .cloned()
                .unwrap_or_default();
            let entity = commands
                .spawn((
                    NameTag(player),
                    Text::new(name),
                    TextFont {
                        font_size,
                        ..default()
                    },
                    TextColor(Color::srgba(0.92, 0.95, 1.0, 0.9)),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(left),
                        top: Val::Px(top),
                        ..default()
                    },
                    UiTransform::from_translation(Val2::percent(-50.0, -100.0)),
                    GlobalZIndex(20),
                    Visibility::Hidden,
                ))
                .id();
            visuals.0.insert(player, entity);
        }
    }
}
