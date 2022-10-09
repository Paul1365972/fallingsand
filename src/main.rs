use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    window::PresentMode,
};

use bevy_pixel_buffer::{prelude::*, query::QueryPixelBuffer};
use fallingsand_sim::{
    cell::cell::SimulationCell,
    chunk::{EntityChunk, TileChunk},
    coords::{ChunkCoords, WorldChunkCoords, WorldCoords},
    myimpl::{
        tile::{Tile, Variant},
        tilesimulator::Context,
    },
    region::{Chunk, DisjointRegion},
};

fn create_field() -> DisjointRegion<Tile, ()> {
    let mut field = DisjointRegion::new_unchecked();
    for y in 0..(4 + 2 * 2) {
        for x in 0..(8 + 2 * 2) {
            field.insert(
                WorldChunkCoords::new(x, y),
                Chunk::new(TileChunk::new_air(), EntityChunk::default()),
            );
        }
    }
    field
        .get_mut(WorldChunkCoords::new(2, 2))
        .unwrap()
        .tile_chunk_mut()
        .get_mut(ChunkCoords::new(60, 63))
        .variant = Variant::SAND;
    field
        .get_mut(WorldChunkCoords::new(2, 2))
        .unwrap()
        .tile_chunk_mut()
        .get_mut(ChunkCoords::new(60, 11))
        .variant = Variant::STONE;
    field
        .get_mut(WorldChunkCoords::new(2, 2))
        .unwrap()
        .tile_chunk_mut()
        .get_mut(ChunkCoords::new(59, 10))
        .variant = Variant::STONE;
    field
}

fn main() {
    println!("Hello, world!");
    App::new()
        .insert_resource(WindowDescriptor {
            title: "Fallingsand-Sim Game".to_string(),
            width: 1280.,
            height: 720.,
            present_mode: PresentMode::Immediate,
            ..Default::default()
        })
        .insert_resource(GameState {
            ctx: Context::default(),
            field: create_field(),
        })
        .add_plugins(DefaultPlugins)
        .add_plugin(LogDiagnosticsPlugin::default())
        .add_plugin(FrameTimeDiagnosticsPlugin::default())
        .add_plugin(PixelBufferPlugin)
        .add_startup_system(
            PixelBufferBuilder::new()
                .with_size(PixelBufferSize::pixel_size((5, 5))) // only set pixel_size as size will be dynamically updated
                .with_fill(Fill::window()) // set fill to the window
                .setup(),
        )
        .add_startup_system(setup)
        .add_system(update_pixels)
        .add_system(update_simulation)
        .run();
}

struct GameState {
    ctx: Context,
    field: DisjointRegion<Tile, ()>,
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    asset_server.watch_for_changes().unwrap();
    let texture: Handle<Image> = asset_server.load("player.png");
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
    // pb.frame().per_pixel(|_, _| Pixel::GREEN);
    pb.frame().per_pixel(|p, _| {
        let coords = WorldCoords::new(p.x as i32 + 48, p.y as i32 + 48);
        let chunk = state.field.get(coords.to_world_chunk_coords());
        if chunk.is_none() {
            return Pixel::BLACK;
        }
        let tile = chunk.unwrap().tile_chunk().get(coords.to_chunk_coords());
        match tile.variant {
            Variant::NIL => todo!(),
            Variant::AIR => Pixel {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
            Variant::SAND => Pixel {
                r: 255,
                g: 255,
                b: 0,
                a: 255,
            },
            Variant::STONE => Pixel {
                r: 64,
                g: 64,
                b: 64,
                a: 255,
            },
            Variant::WATER => Pixel {
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
    state
        .field
        .step_tiles(|x: &mut SimulationCell<Tile>| x.step(&state.ctx));
}
