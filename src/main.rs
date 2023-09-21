use std::{time::Duration, mem::ManuallyDrop};

use bevy::{
    asset::ChangeWatcher,
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    window::PresentMode,
};

use bevy_pancam::{PanCam, PanCamPlugin};
use bevy_pixel_buffer::{prelude::*, query::QueryPixelBuffer};
use fallingsand_sim::{
    cell::tile::{MyTile, MyTileVariant},
    chunk::{TileChunk, UnloadedRegion},
    util::coords::{WorldCoords, WorldRegionCoords, TILES_PER_CHUNK, CHUNKS_PER_REGION},
    world::World,
};

fn create_world() -> World {
    let mut world = World::default();
    for y in -1..=1 {
        for x in -1..=1 {
            let mut chunks = Vec::with_capacity(CHUNKS_PER_REGION * CHUNKS_PER_REGION);
            for _ in 0..(CHUNKS_PER_REGION * CHUNKS_PER_REGION) {
                chunks.push(TileChunk::new(std::array::from_fn(|_| MyTile {
                    variant: MyTileVariant::AIR,
                    ..Default::default()
                })));
            }
            let chunks = unsafe {
                Box::from_raw(ManuallyDrop::new(chunks).as_mut_ptr() as *mut [TileChunk; CHUNKS_PER_REGION * CHUNKS_PER_REGION])
            };
            world.load_region(
                WorldRegionCoords::new(x, y),
                UnloadedRegion {
                    tile_chunk: chunks,
                    entities: vec![],
                },
            );
        }
    }
    let region = world.unsafe_get_mut(&WorldRegionCoords::new(0, 0)).unwrap();
    //for i in 0..CHUNKS_PER_REGION {
    //    for j in 0..TILES_PER_CHUNK {
    //        region.chunks[i + i * CHUNKS_PER_REGION].tiles[j + j * TILES_PER_CHUNK].variant = MyTileVariant::STONE;
    //    }
    //}
    let chunk = &mut region.chunks[0];
    chunk.tiles[2 + (TILES_PER_CHUNK - 2) * TILES_PER_CHUNK].variant = MyTileVariant::SAND;
    chunk.tiles[2 + 1 * TILES_PER_CHUNK].variant = MyTileVariant::STONE;
    chunk.tiles[1 + 0 * TILES_PER_CHUNK].variant = MyTileVariant::STONE;
    world
}

fn main() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    println!("Hello, world!");
    let mut app = App::new();

    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    canvas: Some("#bevy".to_owned()),
                    title: "Fallingsand-Sim Game".to_owned(),
                    present_mode: PresentMode::AutoNoVsync,
                    //present_mode: PresentMode::AutoVsync,
                    ..Default::default()
                }),
                ..default()
            })
            .set(AssetPlugin {
                watch_for_changes: ChangeWatcher::with_delay(Duration::from_secs(1)),
                ..Default::default()
            }),
    );

    app.add_plugins((LogDiagnosticsPlugin::default(), FrameTimeDiagnosticsPlugin));

    app.add_plugins(PixelBufferPlugin);
    app.add_systems(
        Startup,
        PixelBufferBuilder::new()
            .with_size(PixelBufferSize::pixel_size((2, 2))) // only set pixel_size as size will be dynamically updated
            .with_fill(Fill::window())
            .with_render(RenderConfig::sprite()) // set fill to the window
            .setup(),
    );

    app.add_plugins(PanCamPlugin);

    app.insert_resource(GameState {
        field: create_world(),
    });
    app.insert_resource(FixedTime::new_from_secs(1.0 / 10.0));

    app.add_systems(Startup, setup);
    app.add_systems(Update, update_pixels);
    app.add_systems(FixedUpdate, update_simulation);
    app.run();
}

#[derive(Resource)]
struct GameState {
    field: World,
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    let texture: Handle<Image> = asset_server.load("player.png");

    // Create Camera
    commands.spawn(Camera2dBundle::default()).insert(PanCam {
        grab_buttons: vec![MouseButton::Right],
        min_scale: 0.1,
        max_scale: Some(10.),
        ..Default::default()
    });

    // commands
    //     .spawn(Camera2dComponents::default())
    //     .spawn(UiCameraComponents::default())
    //     .insert_resource(Grid::default())
    //     .insert_resource(InputState::default())
    //     .spawn(SpriteComponents {
    //         material: materials.add(th.into()),
    //         transform: Transform {
    //             scale,
    //             ..Default::default()
    //         },
    //         ..Default::default()
    //     })
    //     .with(GridTexture);
}

fn update_pixels(mut pb: QueryPixelBuffer, state: Res<GameState>) {
    let state = state.as_ref();
    pb.frame().per_pixel(|p, _| {
        let coords = WorldCoords::new(p.x as i32 - 48, p.y as i32 - 48);
        let region_coords = coords.to_region_coords();
        let world_region_coords = coords.to_world_region_coords();
        let region = state.field.unsafe_get(&world_region_coords);
        if let Some(region) = region {
            if region_coords.to_chunk_coords().to_tile_index() == 0 {
                return Pixel::RED;
            }
            let tile = region.chunks[region_coords.to_chunk_index()].tiles
                [region_coords.to_chunk_coords().to_tile_index()];

            if world_region_coords != WorldRegionCoords::new(0, 0)
                && matches!(tile.variant, MyTileVariant::AIR)
            {
                return Pixel {
                    r: 196,
                    g: 196,
                    b: 196,
                    a: 255,
                };
            }
            match tile.variant {
                MyTileVariant::NIL => todo!(),
                MyTileVariant::AIR => Pixel {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
                MyTileVariant::SAND => Pixel {
                    r: 255,
                    g: 255,
                    b: 0,
                    a: 255,
                },
                MyTileVariant::STONE => Pixel {
                    r: 64,
                    g: 64,
                    b: 64,
                    a: 255,
                },
                MyTileVariant::WATER => Pixel {
                    r: 0,
                    g: 0,
                    b: 255,
                    a: 255,
                },
            }
        } else {
            Pixel::BLACK
        }
    });
}

fn update_simulation(mut state: ResMut<GameState>) {
    let state = state.as_mut();
    state.field.step_context();
    state.field.step_tiles();
}
