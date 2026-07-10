use super::Game;
use super::PLAYER_SIZE;
use super::camera::CameraState;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::sprite::Anchor;
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
    mut visuals: ResMut<NametagVisuals>,
    mut tags: Query<(&NameTag, &mut Text2d, &mut Transform, &mut Visibility)>,
) {
    let ingame = game.0.ingame();
    let local = ingame.and_then(|ingame| ingame.net.session.as_ref()?.player());

    visuals.0.retain(|player, entity| {
        let live = ingame.is_some_and(|ingame| ingame.players.roster.contains_key(player))
            && Some(*player) != local;
        if !live {
            commands.entity(*entity).despawn();
        }
        live
    });

    let Some(ingame) = ingame else {
        return;
    };
    for (&player, remote) in &ingame.players.roster {
        if Some(player) == local {
            continue;
        }
        let world = remote.pos + Vec2::new(0.0, PLAYER_SIZE.y / 2.0 + NAMETAG_RISE);
        let px = ((world - state.pos) * state.k as f32).round();
        let translation = Vec3::new(px.x, px.y, 20.0);
        if let Some(&entity) = visuals.0.get(&player) {
            if let Ok((tag, mut text, mut transform, mut visibility)) = tags.get_mut(entity) {
                transform.translation = translation;
                *visibility = Visibility::Inherited;
                if game.0.changes.roster {
                    let name = ingame
                        .players
                        .names
                        .get(&tag.0)
                        .map(String::as_str)
                        .unwrap_or("");
                    if text.0 != name {
                        text.0 = name.to_string();
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
                    Text2d::new(name),
                    TextFont {
                        font_size: FontSize::Px(16.0),
                        ..default()
                    },
                    TextColor(Color::srgba(0.92, 0.95, 1.0, 0.9)),
                    Anchor::BOTTOM_CENTER,
                    Transform::from_translation(translation),
                    Visibility::Hidden,
                ))
                .id();
            visuals.0.insert(player, entity);
        }
    }
}
