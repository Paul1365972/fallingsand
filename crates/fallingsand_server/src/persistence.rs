mod player_record;
mod region_codec;
mod store;
mod worker;

pub use player_record::{PlayerRecord, restore_player, snapshot_player};

use crate::WorldInfo;
use crate::player::Players;
use crate::regions::{RegionMap, RegionSave, collect_region_saves, mark_changed_regions};
use fallingsand_core::{Calendar, Chunk, Region, RegionPos};
use fallingsand_protocol::PlayerUuid;
use fallingsand_sim::CellWorld;
use fallingsand_worldgen::WorldGenerator;
use redb::TableDefinition;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::TryRecvError;
use store::WorldStore;
use worker::{PersistenceWorker, WorkerCommand, WorkerCompletion};

pub const REGION_FORMAT_VERSION: u8 = 21;
pub const WORLD_FORMAT_VERSION: u16 = 24;
const AUTOSAVE_INTERVAL_TICKS: u64 = fallingsand_core::ticks_from_secs(10.0);

const REGIONS: TableDefinition<u64, &[u8]> = TableDefinition::new("regions");
const PLAYERS: TableDefinition<u128, &[u8]> = TableDefinition::new("players");
const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldMeta {
    pub format_version: u16,
    pub seed: u64,
    pub name: String,
    pub world_age: u64,
    pub tick: u64,
}

pub struct RegionLoad {
    pub region: Region,
    pub revision: u64,
    pub persisted_revision: u64,
}

pub struct RegionReady {
    pub request: u64,
    pub pos: RegionPos,
    pub result: Result<RegionLoad, StoreError>,
}

pub struct PersistenceCompletions {
    pub regions: Vec<RegionReady>,
    pub saved_regions: Vec<(RegionPos, u64)>,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("redb: {0}")]
    Redb(#[from] redb::Error),
    #[error("corrupt region blob: {0}")]
    CorruptRegion(String),
    #[error("corrupt record: {0}")]
    CorruptRecord(#[from] postcard::Error),
    #[error("corrupt player record: {0}")]
    CorruptPlayer(String),
    #[error("failed to load player {uuid}: {source}")]
    PlayerLoad {
        uuid: PlayerUuid,
        #[source]
        source: Box<StoreError>,
    },
    #[error(
        "unsupported world format {0} (current {WORLD_FORMAT_VERSION}); pre-release worlds carry no migrations — delete the world and create a new one"
    )]
    UnsupportedWorld(u16),
    #[error("unsupported region format {0} (server supports {REGION_FORMAT_VERSION})")]
    UnsupportedRegion(u8),
    #[error("failed to load region {pos:?}: {source}")]
    RegionLoad {
        pos: RegionPos,
        #[source]
        source: Box<StoreError>,
    },
    #[error("persistence worker disconnected")]
    WorkerDisconnected,
    #[error("persistence worker panicked")]
    WorkerPanicked,
    #[error("failed to start persistence worker: {0}")]
    WorkerStart(String),
}

impl From<redb::DatabaseError> for StoreError {
    fn from(err: redb::DatabaseError) -> Self {
        Self::Redb(err.into())
    }
}
impl From<redb::TransactionError> for StoreError {
    fn from(err: redb::TransactionError) -> Self {
        Self::Redb(err.into())
    }
}
impl From<redb::TableError> for StoreError {
    fn from(err: redb::TableError) -> Self {
        Self::Redb(err.into())
    }
}
impl From<redb::StorageError> for StoreError {
    fn from(err: redb::StorageError) -> Self {
        Self::Redb(err.into())
    }
}
impl From<redb::CommitError> for StoreError {
    fn from(err: redb::CommitError) -> Self {
        Self::Redb(err.into())
    }
}

#[derive(Clone)]
struct RegionSnapshot {
    revision: u64,
    persisted_revision: u64,
    base: Option<Arc<Region>>,
    chunks: Vec<(fallingsand_core::ChunkOffset, Arc<Chunk>)>,
}

impl RegionSnapshot {
    fn merge(&mut self, newer: Self) {
        self.revision = self.revision.max(newer.revision);
        self.persisted_revision = self.persisted_revision.max(newer.persisted_revision);
        if newer.base.is_some() {
            self.base = newer.base;
            self.chunks = newer.chunks;
            return;
        }
        for (offset, chunk) in newer.chunks {
            if let Some((_, current)) = self
                .chunks
                .iter_mut()
                .find(|(current, _)| *current == offset)
            {
                *current = chunk;
            } else {
                self.chunks.push((offset, chunk));
            }
        }
        self.chunks
            .sort_unstable_by_key(|(offset, _)| offset.index());
    }

    fn materialize(
        &self,
        store: Option<&WorldStore>,
        generator: &WorldGenerator,
        pos: RegionPos,
    ) -> Result<Region, StoreError> {
        let mut region = match &self.base {
            Some(region) => (**region).clone(),
            None => store
                .map(|store| store.load_region(pos))
                .transpose()?
                .flatten()
                .unwrap_or_else(|| generator.generate_region(pos)),
        };
        for &(offset, ref chunk) in &self.chunks {
            *region.chunk_mut(offset) = (**chunk).clone();
        }
        Ok(region)
    }
}

#[derive(Clone)]
struct SaveBatch {
    regions: Vec<(RegionPos, RegionSnapshot)>,
    players: Vec<(PlayerUuid, PlayerRecord)>,
    meta: Option<WorldMeta>,
}

pub struct Persistence {
    store: Option<Arc<WorldStore>>,
    worker: Option<PersistenceWorker>,
    pending_regions: BTreeMap<RegionPos, RegionSnapshot>,
    pending_players: BTreeMap<PlayerUuid, PlayerRecord>,
    pending_meta: Option<WorldMeta>,
    next_request: u64,
    in_flight: Option<Arc<SaveBatch>>,
}

impl Persistence {
    pub fn open(path: Option<&Path>) -> Result<Self, StoreError> {
        Ok(Self {
            store: path.map(WorldStore::open).transpose()?.map(Arc::new),
            worker: None,
            pending_regions: BTreeMap::new(),
            pending_players: BTreeMap::new(),
            pending_meta: None,
            next_request: 1,
            in_flight: None,
        })
    }

    pub fn start_worker(&mut self, seed: u64) -> Result<(), StoreError> {
        self.worker = Some(PersistenceWorker::start(self.store.clone(), seed)?);
        Ok(())
    }

    pub fn load_meta(&self) -> Result<Option<WorldMeta>, StoreError> {
        match &self.pending_meta {
            Some(meta) => Ok(Some(meta.clone())),
            None => self
                .store
                .as_ref()
                .map_or(Ok(None), |store| store.load_meta()),
        }
    }

    pub fn stage_meta(&mut self, meta: WorldMeta) {
        self.pending_meta = Some(meta);
    }

    pub fn flush_startup_meta(&mut self) -> Result<(), StoreError> {
        let Some(store) = &self.store else {
            return Ok(());
        };
        let Some(meta) = &self.pending_meta else {
            return Ok(());
        };
        store.save_meta(meta)?;
        self.pending_meta = None;
        Ok(())
    }

    pub fn request_region(
        &mut self,
        pos: RegionPos,
    ) -> Result<(u64, Option<RegionLoad>), StoreError> {
        let request = self.next_request;
        self.next_request = self.next_request.wrapping_add(1).max(1);
        let retained = self.pending_regions.get(&pos).cloned().or_else(|| {
            self.in_flight
                .as_ref()?
                .regions
                .iter()
                .find_map(|(saved_pos, snapshot)| (*saved_pos == pos).then(|| snapshot.clone()))
        });
        if let Some(pending) = retained {
            return Ok((
                request,
                Some(RegionLoad {
                    region: pending
                        .base
                        .as_deref()
                        .map(|region| {
                            let mut region = region.clone();
                            for &(offset, ref chunk) in &pending.chunks {
                                *region.chunk_mut(offset) = (**chunk).clone();
                            }
                            region
                        })
                        .expect("unloaded pending region has a full snapshot"),
                    revision: pending.revision,
                    persisted_revision: pending.persisted_revision,
                }),
            ));
        }
        let worker = self.worker.as_ref().ok_or(StoreError::WorkerDisconnected)?;
        worker
            .commands
            .send(WorkerCommand::LoadRegion { request, pos })
            .map_err(|_| StoreError::WorkerDisconnected)?;
        Ok((request, None))
    }

    pub fn stage_region(&mut self, save: RegionSave) {
        let snapshot = RegionSnapshot {
            revision: save.revision,
            persisted_revision: save.persisted_revision,
            base: save.base,
            chunks: save.chunks,
        };
        match self.pending_regions.entry(save.pos) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(snapshot);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                entry.get_mut().merge(snapshot);
            }
        }
    }

    pub fn stage_regions(&mut self, regions: impl IntoIterator<Item = RegionSave>) {
        for region in regions {
            self.stage_region(region);
        }
    }

    pub fn load_player(&mut self, uuid: PlayerUuid) -> Result<Option<PlayerRecord>, StoreError> {
        if let Some(record) = self.pending_players.get(&uuid) {
            return Ok(Some(record.clone()));
        }
        if let Some(record) = self.in_flight.as_ref().and_then(|batch| {
            batch
                .players
                .iter()
                .find_map(|(id, record)| (*id == uuid).then_some(record))
        }) {
            return Ok(Some(record.clone()));
        }
        self.store
            .as_ref()
            .map_or(Ok(None), |store| store.load_player(uuid))
    }

    pub fn stage_player(&mut self, uuid: PlayerUuid, record: PlayerRecord) {
        self.pending_players.insert(uuid, record);
    }

    pub fn pump(&mut self) -> Result<(), StoreError> {
        if self.store.is_none() || self.in_flight.is_some() || !self.has_pending() {
            return Ok(());
        }
        let batch = Arc::new(self.take_batch());
        let worker = self.worker.as_ref().ok_or(StoreError::WorkerDisconnected)?;
        if worker
            .commands
            .send(WorkerCommand::SaveBatch(Arc::clone(&batch)))
            .is_err()
        {
            self.restore_batch(batch);
            return Err(StoreError::WorkerDisconnected);
        }
        self.in_flight = Some(batch);
        Ok(())
    }

    pub fn drain_completions(&mut self) -> Result<PersistenceCompletions, StoreError> {
        let mut completed = PersistenceCompletions {
            regions: Vec::new(),
            saved_regions: Vec::new(),
        };
        let mut messages = Vec::new();
        let worker = self.worker.as_ref().ok_or(StoreError::WorkerDisconnected)?;
        loop {
            match worker.completions.try_recv() {
                Ok(message) => messages.push(message),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Err(StoreError::WorkerDisconnected),
            }
        }
        for message in messages {
            self.apply_completion(message, &mut completed, false)?;
        }
        Ok(completed)
    }

    pub fn flush_durable(&mut self) -> Result<(usize, usize), StoreError> {
        let region_count = self.pending_regions.len();
        let player_count = self.pending_players.len();
        if self.store.is_none() {
            self.shutdown_worker()?;
            return Ok((region_count, player_count));
        }
        loop {
            self.pump()?;
            if !self.has_pending() && self.in_flight.is_none() {
                break;
            }
            let message = self
                .worker
                .as_ref()
                .ok_or(StoreError::WorkerDisconnected)?
                .completions
                .recv()
                .map_err(|_| StoreError::WorkerDisconnected)?;
            let mut completed = PersistenceCompletions {
                regions: Vec::new(),
                saved_regions: Vec::new(),
            };
            if let Err(error) = self.apply_completion(message, &mut completed, true) {
                self.shutdown_worker()?;
                return Err(error);
            }
        }
        self.shutdown_worker()?;
        Ok((region_count, player_count))
    }

    fn has_pending(&self) -> bool {
        !self.pending_regions.is_empty()
            || !self.pending_players.is_empty()
            || self.pending_meta.is_some()
    }

    fn take_batch(&mut self) -> SaveBatch {
        SaveBatch {
            regions: std::mem::take(&mut self.pending_regions)
                .into_iter()
                .collect(),
            players: std::mem::take(&mut self.pending_players)
                .into_iter()
                .collect(),
            meta: self.pending_meta.take(),
        }
    }

    fn restore_batch(&mut self, batch: Arc<SaveBatch>) {
        let batch = Arc::try_unwrap(batch).unwrap_or_else(|batch| (*batch).clone());
        for (pos, mut snapshot) in batch.regions {
            if let Some(newer) = self.pending_regions.remove(&pos) {
                snapshot.merge(newer);
            }
            self.pending_regions.insert(pos, snapshot);
        }
        for (uuid, record) in batch.players {
            self.pending_players.entry(uuid).or_insert(record);
        }
        if self.pending_meta.is_none() {
            self.pending_meta = batch.meta;
        }
    }

    fn apply_completion(
        &mut self,
        message: WorkerCompletion,
        completed: &mut PersistenceCompletions,
        durable: bool,
    ) -> Result<(), StoreError> {
        match message {
            WorkerCompletion::RegionReady(ready) => completed.regions.push(ready),
            WorkerCompletion::SaveComplete => {
                let batch = self.take_in_flight()?;
                let SaveBatch {
                    regions,
                    players,
                    meta,
                } = Arc::try_unwrap(batch).unwrap_or_else(|batch| (*batch).clone());
                for (pos, saved) in regions {
                    let remove_pending =
                        self.pending_regions.get_mut(&pos).is_some_and(|pending| {
                            pending.persisted_revision = pending
                                .persisted_revision
                                .max(saved.revision.min(pending.revision));
                            pending.revision <= pending.persisted_revision
                        });
                    if remove_pending {
                        self.pending_regions.remove(&pos);
                    }
                    completed.saved_regions.push((pos, saved.revision));
                }
                for (uuid, saved) in players {
                    if self.pending_players.get(&uuid) == Some(&saved) {
                        self.pending_players.remove(&uuid);
                    }
                }
                if self.pending_meta == meta {
                    self.pending_meta = None;
                }
            }
            WorkerCompletion::SaveFailed(error) => {
                let batch = self.take_in_flight()?;
                self.restore_batch(batch);
                if durable {
                    return Err(error);
                }
                tracing::error!("persistence batch failed: {error}");
            }
            WorkerCompletion::Fatal(error) => {
                let batch = self.take_in_flight()?;
                self.restore_batch(batch);
                self.shutdown_worker()?;
                return Err(error);
            }
        }
        Ok(())
    }

    fn take_in_flight(&mut self) -> Result<Arc<SaveBatch>, StoreError> {
        self.in_flight.take().ok_or(StoreError::WorkerDisconnected)
    }

    fn shutdown_worker(&mut self) -> Result<(), StoreError> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        drop(worker.completions);
        let _ = worker.commands.send(WorkerCommand::Shutdown);
        drop(worker.commands);
        worker.thread.join().map_err(|_| StoreError::WorkerPanicked)
    }
}

pub fn autosave(
    sim: &CellWorld,
    regions: &mut RegionMap,
    info: &WorldInfo,
    clock: &Calendar,
    players: &Players,
    persistence: &mut Persistence,
) -> Result<(), StoreError> {
    let tick = sim.tick();
    if tick == 0 || !tick.is_multiple_of(AUTOSAVE_INTERVAL_TICKS) {
        persistence.pump()?;
        return Ok(());
    }

    mark_changed_regions(sim, regions);
    let player_records = snapshot_players(players)?;
    persistence.stage_regions(collect_region_saves(sim, regions));
    for (uuid, record) in player_records {
        persistence.stage_player(uuid, record);
    }
    persistence.stage_meta(world_meta(info, clock, tick));
    persistence.pump()
}

pub fn save_everything(
    sim: &mut CellWorld,
    regions: &mut RegionMap,
    players: &Players,
    persistence: &mut Persistence,
    info: &WorldInfo,
    clock: &Calendar,
) -> Result<(), StoreError> {
    let started = std::time::Instant::now();
    let player_records = snapshot_players(players)?;

    mark_changed_regions(sim, regions);
    persistence.stage_regions(collect_region_saves(sim, regions));

    for (uuid, record) in player_records {
        persistence.stage_player(uuid, record);
    }
    persistence.stage_meta(world_meta(info, clock, sim.tick()));
    let (region_count, player_count) = persistence.flush_durable()?;
    regions.mark_all_saved();
    tracing::info!(
        "world saved: {} regions, {} players in {:.1?}",
        region_count,
        player_count,
        started.elapsed(),
    );
    Ok(())
}

fn snapshot_players(players: &Players) -> Result<Vec<(PlayerUuid, PlayerRecord)>, StoreError> {
    players
        .iter()
        .map(|(_, player)| Ok((player.uuid, snapshot_player(player)?)))
        .collect()
}

fn world_meta(info: &WorldInfo, clock: &Calendar, tick: u64) -> WorldMeta {
    WorldMeta {
        format_version: WORLD_FORMAT_VERSION,
        seed: info.seed,
        name: info.name.clone(),
        world_age: clock.age,
        tick,
    }
}

fn parse_meta(bytes: &[u8]) -> Result<WorldMeta, StoreError> {
    let (version, _) = postcard::take_from_bytes::<u16>(bytes)?;
    if version != WORLD_FORMAT_VERSION {
        return Err(StoreError::UnsupportedWorld(version));
    }
    Ok(postcard::from_bytes(bytes)?)
}
