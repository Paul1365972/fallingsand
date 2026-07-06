pub mod bodies;
pub mod commands;
pub mod hazards;
pub mod persistence;
pub mod regions;
pub mod session;
pub mod systems;

use bevy_ecs::prelude::*;
use fallingsand_core::{CellPos, MaterialRegistry};
use fallingsand_net::Listener;
use fallingsand_sim::CellWorld;
use fallingsand_worldgen::WorldGenerator;
use persistence::{WorldMeta, WorldStore};
use regions::{Generator, RegionMap, Store};
use session::Sessions;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use fallingsand_core::TICK_RATE;
pub const TICK_DURATION: Duration = Duration::from_nanos(1_000_000_000 / TICK_RATE as u64);
pub const INTEREST_RADIUS_X: i32 = 6;
pub const INTEREST_RADIUS_Y: i32 = 4;
pub const MAX_HP: f32 = 100.0;
pub use fallingsand_core::{DAY_SECS, MAX_AIR_SECS};

#[derive(Resource)]
pub struct SimWorld(pub CellWorld);

#[derive(Resource, Default)]
pub struct SimObstacles(pub fallingsand_sim::Obstacles);

#[derive(Resource, Default)]
pub struct PlayerImpulses(pub rustc_hash::FxHashMap<Entity, (f32, f32)>);

#[derive(Resource, Clone)]
pub struct Registry(pub Arc<MaterialRegistry>);

#[derive(Resource)]
pub struct NetListener(pub Box<dyn Listener>);

#[derive(Resource, Clone, Copy)]
pub struct SpawnPoint(pub CellPos);

#[derive(Resource, Default, Clone, Copy)]
pub struct WorldClock {
    pub t: f32,
    pub day: u32,
}

impl WorldClock {
    pub fn moon_phase(&self) -> u32 {
        self.day % fallingsand_core::MOON_PHASES
    }
}

#[derive(Resource, Clone)]
pub struct WorldInfo {
    pub seed: u64,
    pub name: String,
}

#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct TickStats {
    pub tick: u64,
    pub sim_micros: u64,
    pub awake_chunks: usize,
    pub loaded_chunks: usize,
    pub active_chunks: usize,
    pub border_chunks: usize,
    pub players: usize,
    pub replicated_bytes: u64,
    pub pixel_bodies: usize,
}

pub struct WorldConfig {
    pub name: String,
    pub seed: u64,
    pub save_path: Option<PathBuf>,
}

pub struct ServerConfig {
    pub registry: Arc<MaterialRegistry>,
    pub listener: Box<dyn Listener>,
    pub stats_sink: Option<Arc<Mutex<TickStats>>>,
    pub world: WorldConfig,
}

pub struct Server {
    world: World,
    schedule: Schedule,
    stats_sink: Option<Arc<Mutex<TickStats>>>,
}

#[derive(Default)]
pub struct ServerControl {
    stop: AtomicBool,
    paused: AtomicBool,
    save: AtomicBool,
}

impl ServerControl {
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn request_save(&self) {
        self.save.store(true, Ordering::Relaxed);
    }

    pub fn stop_requested(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    pub fn paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    fn take_save_request(&self) -> bool {
        self.save.swap(false, Ordering::Relaxed)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error(transparent)]
    Store(#[from] persistence::StoreError),
    #[error(transparent)]
    Gen(#[from] fallingsand_worldgen::GenError),
}

impl Server {
    pub fn new(config: ServerConfig) -> Result<Self, ServerError> {
        let store = match &config.world.save_path {
            Some(path) => {
                tracing::debug!("world store: {}", path.display());
                Some(Arc::new(WorldStore::open(path)?))
            }
            None => None,
        };
        let meta = match store.as_ref().and_then(|s| s.load_meta().transpose()) {
            Some(meta) => {
                let meta = meta?;
                tracing::info!("loaded world \"{}\" (seed {:#x})", meta.name, meta.seed);
                meta
            }
            None => {
                let meta = WorldMeta {
                    format_version: persistence::WORLD_FORMAT_VERSION,
                    seed: config.world.seed,
                    name: config.world.name.clone(),
                    clock: 0.5,
                    day: 0,
                };
                if let Some(store) = &store {
                    store.save_meta(&meta)?;
                }
                tracing::info!("created world \"{}\" (seed {:#x})", meta.name, meta.seed);
                meta
            }
        };
        let seed = meta.seed;
        let generator = Arc::new(WorldGenerator::new(seed, &config.registry)?);

        let spawn_x = 0;
        let spawn = CellPos::new(spawn_x, generator.surface_height(spawn_x) + 12);

        let mut world = World::new();
        world.insert_resource(SimWorld(CellWorld::new()));
        world.insert_resource(Registry(config.registry));
        world.insert_resource(NetListener(config.listener));
        world.insert_resource(Sessions::default());
        world.insert_resource(TickStats::default());
        world.insert_resource(Generator(generator));
        world.insert_resource(Store(store));
        world.insert_resource(RegionMap::default());
        world.insert_resource(regions::ChunkTickets::default());
        world.insert_resource(SpawnPoint(spawn));
        world.insert_resource(bodies::PixelBodies::default());
        world.insert_resource(SimObstacles::default());
        world.insert_resource(PlayerImpulses::default());
        world.insert_resource(commands::PendingCommands::default());
        world.insert_resource(hazards::CrushEvents::default());
        world.insert_resource(WorldClock {
            t: meta.clock.rem_euclid(1.0),
            day: meta.day,
        });
        world.insert_resource(WorldInfo {
            seed,
            name: meta.name.clone(),
        });

        let mut schedule = Schedule::default();
        schedule.add_systems(
            (
                (
                    systems::drain_network,
                    commands::run_commands,
                    systems::apply_player_inputs,
                    regions::compute_tickets,
                    regions::manage_regions,
                    systems::build_obstacles,
                    systems::step_simulation,
                )
                    .chain(),
                (
                    systems::step_physics,
                    bodies::step_bodies,
                    bodies::react_bodies,
                    hazards::apply_hazards,
                    systems::advance_clock,
                    systems::sync_inventories,
                    systems::replicate,
                    bodies::replicate_bodies,
                    systems::finish_tick,
                    regions::autosave,
                )
                    .chain(),
            )
                .chain(),
        );
        Ok(Self {
            world,
            schedule,
            stats_sink: config.stats_sink,
        })
    }

    pub fn tick(&mut self) {
        self.schedule.run(&mut self.world);
        if let Some(sink) = &self.stats_sink {
            *sink.lock().unwrap() = *self.world.resource::<TickStats>();
        }
    }

    pub fn stats(&self) -> TickStats {
        *self.world.resource::<TickStats>()
    }

    pub fn save_all(&mut self, final_save: bool) {
        regions::save_everything(&mut self.world, final_save);
    }

    pub fn run_blocking(&mut self, control: Arc<ServerControl>) {
        let mut timer = StepTimer::new(TICK_DURATION);
        while !control.stop_requested() {
            if control.take_save_request() {
                self.save_all(false);
            }
            if !control.paused() {
                self.tick();
                let stats = self.stats();
                if stats.tick.is_multiple_of(10 * TICK_RATE as u64) {
                    tracing::debug!(
                        "tick {}: {} players, {}/{} chunks awake, {} bodies, sim {:.1}ms",
                        stats.tick,
                        stats.players,
                        stats.awake_chunks,
                        stats.loaded_chunks,
                        stats.pixel_bodies,
                        stats.sim_micros as f64 / 1000.0,
                    );
                }
            }
            timer.sleep();
        }
        tracing::info!("stopping server");
        self.save_all(true);
    }
}

pub struct StepTimer {
    period: Duration,
    last_time: Instant,
    behind: Duration,
}

impl StepTimer {
    pub fn new(period: Duration) -> Self {
        Self {
            period,
            last_time: Instant::now(),
            behind: Duration::ZERO,
        }
    }

    pub fn sleep(&mut self) {
        let passed = self.last_time.elapsed();
        if passed > self.period {
            self.behind += passed - self.period;
            if self.behind >= Duration::from_secs(2) {
                tracing::warn!(
                    "can't keep up, running {}ms behind",
                    self.behind.as_millis()
                );
                self.behind = Duration::ZERO;
            }
        } else {
            self.behind = Duration::ZERO;
        }
        spin_sleep::sleep(self.period.saturating_sub(passed));
        self.last_time = Instant::now();
    }
}
