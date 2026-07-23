pub(crate) mod bodies;
pub(crate) mod commands;
pub(crate) mod dig;
pub(crate) mod hazards;
pub(crate) mod inventory;
pub(crate) mod lifecycle;
pub(crate) mod particles;
pub(crate) mod persistence;
pub(crate) mod physics;
pub(crate) mod player;
pub(crate) mod regions;
pub(crate) mod replication;
pub(crate) mod session;
pub(crate) mod sim;

use fallingsand_core::{Calendar, CellPos, DAY_UNITS};
use fallingsand_net::Listener;
use fallingsand_protocol::{ServerStats, TickProfile};
use fallingsand_sim::{CellWorld, Simulator};
use fallingsand_worldgen::WorldGenerator;
use persistence::{Persistence, WorldMeta};
use player::Players;
use regions::{ChunkTickets, RegionMap};
use replication::ReplicationState;
use session::Sessions;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) use fallingsand_core::TICK_RATE;
pub(crate) const TICK_DURATION: Duration = Duration::from_nanos(1_000_000_000 / TICK_RATE as u64);
pub(crate) const INTEREST_RADIUS_X: i32 = 6;
pub(crate) const INTEREST_RADIUS_Y: i32 = 4;
pub(crate) use fallingsand_core::{MAX_AIR_SECONDS, MAX_HEALTH};

pub(crate) struct WorldInfo {
    pub(crate) seed: u64,
    pub(crate) name: String,
}

pub struct WorldConfig {
    pub name: String,
    pub seed: u64,
    pub save_path: Option<PathBuf>,
}

pub struct ServerConfig {
    pub listener: Box<dyn Listener>,
    pub stats_sink: Option<Arc<Mutex<ServerStats>>>,
    pub world: WorldConfig,
}

struct ServerState {
    listener: Box<dyn Listener>,
    sim: CellWorld,
    simulator: Simulator,
    players: Players,
    sessions: Sessions,
    bodies: bodies::BodyWorld,
    generator: WorldGenerator,
    regions: RegionMap,
    tickets: ChunkTickets,
    persistence: Persistence,
    replication: ReplicationState,
    emitter: particles::ParticleEmitter,
    spawn: CellPos,
    clock: Calendar,
    world: WorldInfo,
    stats: ServerStats,
}

pub struct Server {
    state: ServerState,
    stats_sink: Option<Arc<Mutex<ServerStats>>>,
}

#[derive(Default)]
pub struct ServerControl {
    stop: AtomicBool,
    paused: AtomicBool,
}

impl ServerControl {
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub(crate) fn stop_requested(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    pub(crate) fn paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error(transparent)]
    Store(#[from] persistence::StoreError),
}

impl Server {
    pub fn new(config: ServerConfig) -> Result<Self, ServerError> {
        if let Some(path) = &config.world.save_path {
            tracing::debug!("world store: {}", path.display());
        }
        let mut persistence = Persistence::open(config.world.save_path.as_deref())?;
        let meta = match persistence.load_meta()? {
            Some(meta) => {
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
                persistence.stage_meta(meta.clone());
                tracing::info!("created world \"{}\" (seed {:#x})", meta.name, meta.seed);
                meta
            }
        };
        let seed = meta.seed;
        persistence.start_worker(seed)?;
        let generator = WorldGenerator::new(seed);
        let spawn_x = 0;
        let spawn = CellPos::new(spawn_x, generator.surface_height(spawn_x) + 12);
        let mut sim = CellWorld::new();
        sim.set_tick(meta.tick);
        Ok(Self {
            state: ServerState {
                listener: config.listener,
                sim,
                simulator: Simulator::new(),
                players: Players::default(),
                sessions: Sessions::default(),
                bodies: bodies::BodyWorld::default(),
                generator,
                regions: RegionMap::default(),
                tickets: ChunkTickets::default(),
                persistence,
                replication: ReplicationState::default(),
                emitter: particles::ParticleEmitter::default(),
                spawn,
                clock: Calendar::new(meta.world_age),
                world: WorldInfo {
                    seed,
                    name: meta.name,
                },
                stats: ServerStats::default(),
            },
            stats_sink: config.stats_sink,
        })
    }

    pub(crate) fn tick(&mut self) -> Result<(), ServerError> {
        self.state.tick()?;
        if let Some(sink) = &self.stats_sink {
            *sink.lock().unwrap() = self.state.stats;
        }
        Ok(())
    }

    pub(crate) fn stats(&self) -> ServerStats {
        self.state.stats
    }

    pub fn run_blocking(&mut self, control: Arc<ServerControl>) -> Result<(), ServerError> {
        let mut timer = StepTimer::new(TICK_DURATION);
        while !control.stop_requested() {
            if !control.paused() {
                self.state.stats.tps = timer.tps();
                self.state.stats.slew_ms = timer.slew_ms();
                self.tick()?;
                let stats = self.stats();
                if stats.tick.is_multiple_of(10 * TICK_RATE as u64) {
                    tracing::debug!(
                        "tick {}: {} players, {}/{} chunks awake, {} bodies, sim {:.1}ms tick {:.1}ms",
                        stats.tick,
                        stats.players,
                        stats.awake_chunks,
                        stats.loaded_chunks,
                        stats.pixel_bodies,
                        stats.timing.sim() as f64 / 1000.0,
                        stats.timing.total as f64 / 1000.0,
                    );
                }
            }
            timer.sleep();
        }
        tracing::info!("stopping server");
        self.state.shutdown_persistence()?;
        Ok(())
    }
}

impl ServerState {
    fn timed<R>(
        &mut self,
        name: &'static str,
        slot: fn(&mut TickProfile) -> &mut u32,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let _span = tracing::info_span!("phase", name).entered();
        let start = Instant::now();
        let result = f(self);
        *slot(&mut self.stats.timing) = start.elapsed().as_micros() as u32;
        result
    }

    fn tick(&mut self) -> Result<(), ServerError> {
        let tick_start = Instant::now();

        self.timed(
            "network",
            |t| &mut t.network,
            |s| {
                let disconnected = session::drain_network(
                    &mut *s.listener,
                    &mut s.sessions,
                    &mut s.players,
                    s.spawn,
                    s.sim.tick(),
                    &mut s.persistence,
                )?;
                s.remove_disconnected_players(disconnected)?;
                for (player, text) in commands::run_commands(&mut s.players, &mut s.clock) {
                    s.sessions.send_to_player(
                        player,
                        &fallingsand_protocol::ServerMessage::System { text },
                    );
                }
                Ok::<(), persistence::StoreError>(())
            },
        )?;

        self.timed(
            "player_input",
            |t| &mut t.player_input,
            |s| {
                dig::apply_player_inputs(&mut s.sim, &mut s.players);
                inventory::apply_slot_actions(&mut s.players);
                lifecycle::begin_revives(&mut s.players, s.spawn, s.sim.tick());
            },
        );

        self.timed(
            "regions",
            |t| &mut t.regions,
            |s| {
                regions::compute_tickets(&mut s.tickets, &s.players);
                regions::manage_regions(
                    &mut s.sim,
                    &mut s.regions,
                    &mut s.persistence,
                    &s.tickets,
                    &mut s.bodies,
                )
            },
        )?;

        let sim_metrics = sim::step_simulation(&mut self.simulator, &mut self.sim, &self.tickets);
        self.stats.tick = sim_metrics.tick;
        self.stats.timing.sim_simulate = sim_metrics.timings.simulate_micros;
        self.stats.timing.sim_random_tick = sim_metrics.timings.random_tick_micros;
        self.stats.active_chunks = sim_metrics.active_chunks;
        self.stats.border_chunks = sim_metrics.border_chunks;

        self.timed(
            "physics",
            |t| &mut t.physics,
            |s| {
                physics::step_physics(&mut s.sim, &mut s.bodies, &mut s.players);
            },
        );

        self.timed(
            "bodies",
            |t| &mut t.bodies,
            |s| {
                let metrics = s.bodies.step(&mut s.sim, &s.tickets);
                s.stats.pixel_bodies = metrics.bodies;
            },
        );

        let tick = self.sim.tick();
        self.timed(
            "hazards",
            |t| &mut t.hazards,
            |s| {
                hazards::apply_hazards(&s.sim, &mut s.players);
            },
        );

        self.timed(
            "lifecycle",
            |t| &mut t.lifecycle,
            |s| {
                lifecycle::resolve_lethal(&mut s.sim, &mut s.players, tick);
                for (player, text) in
                    lifecycle::advance_materializations(&mut s.sim, &mut s.players, tick)
                {
                    s.sessions.send_to_player(
                        player,
                        &fallingsand_protocol::ServerMessage::System { text },
                    );
                }
            },
        );

        self.clock.advance();
        self.emitter.emit(&self.players, tick);

        self.timed(
            "replicate",
            |t| &mut t.replicate,
            |s| {
                let metrics = replication::replicate(
                    &mut s.sessions,
                    &s.players,
                    &s.sim,
                    &s.clock,
                    &s.regions,
                    &s.generator,
                    &s.emitter.spawns,
                    &s.bodies,
                    &mut s.replication,
                );
                s.stats.players = metrics.players;
                s.stats.awake_chunks = metrics.awake_chunks;
                s.stats.awake_cells = metrics.awake_cells;
                s.stats.loaded_chunks = metrics.loaded_chunks;
                s.stats.loaded_regions = metrics.loaded_regions;
                s.stats.replicated_bytes = metrics.replicated_bytes;
            },
        );

        self.timed(
            "persistence",
            |t| &mut t.persistence,
            |s| {
                persistence::autosave(
                    &s.sim,
                    &s.regions,
                    &s.world,
                    &s.clock,
                    &s.players,
                    &mut s.persistence,
                )
            },
        )?;

        let total = tick_start.elapsed().as_micros() as u32;
        self.stats.timing.finish(tick, total);
        Ok(())
    }

    fn shutdown_persistence(&mut self) -> Result<(), persistence::StoreError> {
        persistence::shutdown_world(
            &self.sim,
            &self.regions,
            &self.world,
            &self.clock,
            &self.players,
            &mut self.persistence,
        )
    }

    fn remove_disconnected_players(
        &mut self,
        disconnected: Vec<fallingsand_protocol::PlayerId>,
    ) -> Result<(), persistence::StoreError> {
        self.persistence
            .stage_players(disconnected.iter().filter_map(|&id| self.players.get(id)))?;
        for id in disconnected {
            let Some(mut player) = self.players.remove(id) else {
                continue;
            };
            if let Some(avatar) = player.avatar_mut() {
                physics::unstamp(&mut self.sim, &mut avatar.stamp);
            }
        }
        Ok(())
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
