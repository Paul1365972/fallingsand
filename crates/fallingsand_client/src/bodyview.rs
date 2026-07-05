use crate::ClientRegistry;
use crate::interpolation::Interpolated;
use crate::net::{NetSet, ServerMsg, SessionEnded};
use bevy::asset::RenderAssetUsages;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use fallingsand_protocol::{ServerMessage, cells_from_wire};

pub struct BodyViewPlugin;

#[derive(Component)]
pub struct BodyVisual {
    pub pivot: Vec2,
}

#[derive(Resource, Default)]
pub struct BodyVisuals(pub HashMap<u32, Entity>);

impl Plugin for BodyViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BodyVisuals>()
            .add_systems(PreUpdate, apply_body_messages.after(NetSet))
            .add_systems(Update, cleanup_bodies.run_if(on_message::<SessionEnded>))
            .add_systems(OnExit(crate::AppState::InGame), cleanup_bodies);
    }
}

fn cleanup_bodies(mut commands: Commands, mut visuals: ResMut<BodyVisuals>) {
    for (_, entity) in visuals.0.drain() {
        commands.entity(entity).despawn();
    }
}

fn body_image(
    registry: &fallingsand_core::MaterialRegistry,
    width: u8,
    height: u8,
    cells: &[fallingsand_core::Cell],
) -> Image {
    let mut data = vec![0u8; width as usize * height as usize * 4];
    for (index, cell) in cells.iter().enumerate() {
        let material = registry.get(cell.material);
        let color = material.colors[cell.shade() as usize % material.colors.len()];
        let x = index % width as usize;
        let y = index / width as usize;
        let flipped = (height as usize - 1 - y) * width as usize + x;
        data[flipped * 4..flipped * 4 + 4].copy_from_slice(&color);
    }
    Image::new(
        Extent3d {
            width: width as u32,
            height: height as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

#[allow(clippy::too_many_arguments)]
fn apply_body_messages(
    mut commands: Commands,
    mut visuals: ResMut<BodyVisuals>,
    mut messages: MessageReader<ServerMsg>,
    mut images: ResMut<Assets<Image>>,
    mut query: Query<(&BodyVisual, Option<&mut Interpolated>)>,
    registry: Res<ClientRegistry>,
) {
    for ServerMsg(message) in messages.read() {
        match message {
            ServerMessage::PixelBodySpawn {
                id,
                width,
                height,
                com_x,
                com_y,
                cells,
            } => {
                let Ok(decoded) = cells_from_wire(cells) else {
                    error!("bad pixel body payload for {id}");
                    continue;
                };
                if decoded.len() != *width as usize * *height as usize {
                    error!("pixel body size mismatch for {id}");
                    continue;
                }
                if let Some(entity) = visuals.0.remove(id) {
                    commands.entity(entity).despawn();
                }
                let image = images.add(body_image(&registry.0, *width, *height, &decoded));
                let size = Vec2::new(*width as f32, *height as f32);
                let com = Vec2::new(*com_x, *com_y);
                let entity = commands
                    .spawn((
                        BodyVisual {
                            pivot: size / 2.0 - com,
                        },
                        Sprite {
                            image,
                            custom_size: Some(size),
                            ..default()
                        },
                        Transform::from_xyz(0.0, 0.0, 5.0),
                        Visibility::Hidden,
                    ))
                    .id();
                visuals.0.insert(*id, entity);
            }
            ServerMessage::PixelBodyDespawn { id } => {
                if let Some(entity) = visuals.0.remove(id) {
                    commands.entity(entity).despawn();
                }
            }
            ServerMessage::PixelBodyStates { bodies } => {
                for state in bodies {
                    let Some(&entity) = visuals.0.get(&state.id) else {
                        continue;
                    };
                    let Ok((visual, interpolated)) = query.get_mut(entity) else {
                        continue;
                    };
                    let position = Vec2::new(state.x, state.y);
                    match interpolated {
                        Some(mut interpolated) => {
                            interpolated.record(position, state.angle, false);
                        }
                        None => {
                            commands.entity(entity).insert((
                                Interpolated::snapped(position, state.angle)
                                    .with_pivot(visual.pivot),
                                Visibility::Visible,
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
