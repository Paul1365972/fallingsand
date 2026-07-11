pub(crate) mod bodies;
pub(crate) mod commands;
pub(crate) mod dig;
pub(crate) mod hazards;
pub(crate) mod inventory;
pub(crate) mod persistence;
pub(crate) mod physics;
pub(crate) mod player;
pub(crate) mod regions;
pub(crate) mod replication;
pub(crate) mod session;
pub(crate) mod sim;

use bevy_ecs::prelude::*;
use fallingsand_core::{Calendar, CellPos, DAY_UNITS};
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

pub(crate) use fallingsand_core::TICK_RATE;
pub(crate) const TICK_DURATION: Duration = Duration::from_nanos(1_000_000_000 / TICK_RATE as u64);
pub(crate) const INTEREST_RADIUS_X: i32 = 6;
pub(crate) const INTEREST_RADIUS_Y: i32 = 4;
pub(crate) use fallingsand_core::{MAX_AIR_SECS, MAX_HP};

#[derive(Resource)]
pub(crate) struct SimWorld(pub(crate) CellWorld);

#[derive(Resource, Default)]
pub(crate) struct PlayerImpulses(pub(crate) rustc_hash::FxHashMap<Entity, (f32, f32)>);

#[derive(Resource)]
pub(crate) struct NetListener(pub(crate) Box<dyn Listener>);

#[derive(Resource, Clone, Copy)]
pub(crate) struct SpawnPoint(pub(crate) CellPos);

#[derive(Resource, Default, Clone, Copy)]
pub(crate) struct WorldClock(pub(crate) Calendar);

#[derive(Resource, Clone)]
pub(crate) struct WorldInfo {
    pub(crate) seed: u64,
    pub(crate) name: String,
}

pub(crate) use fallingsand_protocol::Stats;

#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct TickStats(pub Stats);

impl std::ops::Deref for TickStats {
    type Target = Stats;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for TickStats {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct WorldConfig {
    pub name: String,
    pub seed: u64,
    pub save_path: Option<PathBuf>,
}

pub struct ServerConfig {
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

    pub(crate) fn stop_requested(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    pub(crate) fn paused(&self) -> bool {
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
                    world_age: DAY_UNITS / 2,
                    tick: 0,
                };
                if let Some(store) = &store {
                    store.save_meta(&meta)?;
                }
                tracing::info!("created world \"{}\" (seed {:#x})", meta.name, meta.seed);
                meta
            }
        };
        let seed = meta.seed;
        let generator = Arc::new(WorldGenerator::new(seed));
        let item_registry = Arc::new(fallingsand_core::content::item_registry());
        let recipes = Arc::new(fallingsand_core::content::recipe_registry(&item_registry));

        let spawn_x = 0;
        let spawn = CellPos::new(spawn_x, generator.surface_height(spawn_x) + 12);

        let mut cell_world = CellWorld::new();
        cell_world.set_tick(meta.tick);
        let mut world = World::new();
        world.insert_resource(SimWorld(cell_world));
        world.insert_resource(inventory::ItemReg(item_registry));
        world.insert_resource(inventory::Recipes(recipes));
        world.insert_resource(inventory::SlotActions::default());
        world.insert_resource(NetListener(config.listener));
        world.insert_resource(Sessions::default());
        world.insert_resource(replication::LastPlayers::default());
        world.insert_resource(TickStats::default());
        world.insert_resource(Generator(generator));
        world.insert_resource(Store(store));
        world.insert_resource(RegionMap::default());
        world.insert_resource(regions::ChunkTickets::default());
        world.insert_resource(SpawnPoint(spawn));
        world.insert_resource(bodies::PixelBodies::default());
        world.insert_resource(PlayerImpulses::default());
        world.insert_resource(commands::PendingCommands::default());
        world.insert_resource(hazards::CrushEvents::default());
        world.insert_resource(WorldClock(Calendar::new(meta.world_age)));
        world.insert_resource(WorldInfo {
            seed,
            name: meta.name.clone(),
        });

        let mut schedule = Schedule::default();
        schedule.add_systems(
            (
                (
                    session::drain_network,
                    commands::run_commands,
                    dig::apply_player_inputs,
                    inventory::apply_slot_actions,
                    regions::compute_tickets,
                    regions::manage_regions,
                    sim::step_simulation,
                )
                    .chain(),
                (
                    physics::step_physics,
                    bodies::step_bodies,
                    hazards::apply_hazards,
                    replication::advance_clock,
                    replication::replicate,
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

    pub(crate) fn tick(&mut self) {
        self.schedule.run(&mut self.world);
        if let Some(sink) = &self.stats_sink {
            *sink.lock().unwrap() = *self.world.resource::<TickStats>();
        }
    }

    pub(crate) fn stats(&self) -> TickStats {
        *self.world.resource::<TickStats>()
    }

    pub(crate) fn save_all(&mut self, final_save: bool) {
        regions::save_everything(&mut self.world, final_save);
    }

    pub fn run_blocking(&mut self, control: Arc<ServerControl>) {
        let mut timer = StepTimer::new(TICK_DURATION);
        while !control.stop_requested() {
            if control.take_save_request() {
                self.save_all(false);
            }
            if !control.paused() {
                {
                    let mut stats = self.world.resource_mut::<TickStats>();
                    stats.tps = timer.tps();
                    stats.slew_ms = timer.slew_ms();
                }
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

pub(crate) struct StepTimer {
    period: Duration,
    last_time: Instant,
    last_period: Duration,
    behind: Duration,
}

impl StepTimer {
    pub(crate) fn new(period: Duration) -> Self {
        Self {
            period,
            last_time: Instant::now(),
            last_period: period,
            behind: Duration::ZERO,
        }
    }

    pub(crate) fn tps(&self) -> f32 {
        let secs = self.last_period.as_secs_f32();
        if secs > 0.0 { 1.0 / secs } else { 0.0 }
    }

    pub(crate) fn slew_ms(&self) -> u32 {
        self.behind.as_millis() as u32
    }

    pub(crate) fn sleep(&mut self) {
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
        let now = Instant::now();
        self.last_period = now - self.last_time;
        self.last_time = now;
    }
}
