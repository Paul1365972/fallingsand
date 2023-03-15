use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    window::PresentMode,
};

use bevy_pancam::{PanCam, PanCamPlugin};
use bevy_pixel_buffer::{prelude::*, query::QueryPixelBuffer};
use fallingsand_sim::{
    cell::{
        cell::SimulationCell,
        tile::{MyTile, MyTileVariant},
    },
    chunk::{Chunk, EntityKeyChunk, TileChunk},
    region::DisjointRegion,
    util::coords::{ChunkCoords, WorldChunkCoords, WorldCoords, TILES_PER_CHUNK},
    world::GlobalContext,
};

fn create_field() -> DisjointRegion {
    let mut field = DisjointRegion::new_unchecked();
    for y in 0..(4 + 2 * 2) {
        for x in 0..(8 + 2 * 2) {
            field.insert_tile_chunk(
                WorldChunkCoords::new(x, y),
                TileChunk::new(
                    [MyTile {
                        variant: MyTileVariant::AIR,
                        ..Default::default()
                    }; TILES_PER_CHUNK as usize * TILES_PER_CHUNK as usize],
                ),
            );
        }
    }
    field
        .unsafe_get_mut(WorldChunkCoords::new(2, 2))
        .unwrap()
        .tile_chunk_mut()
        .get_mut(ChunkCoords::new(60, 63))
        .variant = MyTileVariant::SAND;
    field
        .unsafe_get_mut(WorldChunkCoords::new(2, 2))
        .unwrap()
        .tile_chunk_mut()
        .get_mut(ChunkCoords::new(60, 11))
        .variant = MyTileVariant::STONE;
    field
        .unsafe_get_mut(WorldChunkCoords::new(2, 2))
        .unwrap()
        .tile_chunk_mut()
        .get_mut(ChunkCoords::new(59, 10))
        .variant = MyTileVariant::STONE;
    field
}

fn main() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    println!("Hello, world!");
    let mut app = App::new();

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            canvas: Some("#bevy".to_owned()),
            title: "Fallingsand-Sim Game".to_string(),
            // present_mode: PresentMode::Immediate,
            present_mode: PresentMode::AutoVsync,
            ..Default::default()
        }),
        ..default()
    }).set(AssetPlugin {
        watch_for_changes: true,
        ..Default::default()
    }));

    app.add_plugin(LogDiagnosticsPlugin::default());
    app.add_plugin(FrameTimeDiagnosticsPlugin);

    app.add_plugin(PixelBufferPlugin);
    app.add_startup_system(
        PixelBufferBuilder::new()
            .with_size(PixelBufferSize::pixel_size((5, 5))) // only set pixel_size as size will be dynamically updated
            .with_fill(Fill::window())
            .with_render(RenderConfig::sprite()) // set fill to the window
            .setup(),
    );

    app.add_plugin(PanCamPlugin);


    app.insert_resource(GameState {
        ctx: GlobalContext::default(),
        field: create_field(),
    });

    
    app.add_startup_system(setup)
        .add_systems((update_pixels, update_simulation));
    app.run();
}

#[derive(Resource)]
struct GameState {
    ctx: GlobalContext,
    field: DisjointRegion,
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
    pb.frame().per_pixel(|_, _| Pixel::GREEN);
    pb.frame().per_pixel(|p, _| {
        let coords = WorldCoords::new(p.x as i32 + 48, p.y as i32 + 48);
        let chunk = state.field.unsafe_get(coords.to_world_chunk_coords());
        if chunk.is_none() {
            return Pixel::BLACK;
        }
        let tile = chunk.unwrap().tile_chunk().get(coords.to_chunk_coords());
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
    });
}

fn update_simulation(mut state: ResMut<GameState>) {
    let state = state.as_mut();
    state.ctx.tick += 1;
    state.field.step_tiles(&state.ctx);
}
