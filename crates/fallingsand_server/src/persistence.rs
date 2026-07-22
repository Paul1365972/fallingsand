mod player_record;
mod region_codec;
mod store;
mod worker;

use crate::WorldInfo;
use crate::player::{Player, Players, RestoredPlayer};
use crate::regions::{RegionMap, snapshot_regions};
use fallingsand_core::{Calendar, Region, RegionPos};
use fallingsand_protocol::PlayerUuid;
use fallingsand_sim::CellWorld;
use player_record::{PlayerRecord, restore_player, snapshot_player};
use redb::TableDefinition;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use store::WorldStore;
use worker::{PersistenceWorker, WorkerCompletion};

pub const REGION_FORMAT_VERSION: u8 = 23;
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
}

pub struct RegionReady {
    pub request: u64,
    pub pos: RegionPos,
    pub result: Result<RegionLoad, StoreError>,
}

#[derive(Default)]
pub struct PersistenceCompletions {
    pub regions: Vec<RegionReady>,
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
struct SaveBatch {
    regions: BTreeMap<RegionPos, Arc<Region>>,
    players: BTreeMap<PlayerUuid, PlayerRecord>,
    meta: Option<WorldMeta>,
}

pub struct Persistence {
    store: Option<Arc<WorldStore>>,
    worker: Option<PersistenceWorker>,
    pending_regions: BTreeMap<RegionPos, Arc<Region>>,
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

    pub fn request_region(
        &mut self,
        pos: RegionPos,
    ) -> Result<(u64, Option<RegionLoad>), StoreError> {
        let request = self.next_request;
        self.next_request = self.next_request.wrapping_add(1).max(1);
        let retained = self
            .pending_regions
            .get(&pos)
            .cloned()
            .or_else(|| self.in_flight.as_ref()?.regions.get(&pos).cloned());
        if let Some(pending) = retained {
            return Ok((
                request,
                Some(RegionLoad {
                    region: (*pending).clone(),
                }),
            ));
        }
        self.worker
            .as_ref()
            .ok_or(StoreError::WorkerDisconnected)?
            .request_region(request, pos)?;
        Ok((request, None))
    }

    pub fn stage_region(&mut self, pos: RegionPos, region: impl Into<Arc<Region>>) {
        self.pending_regions.insert(pos, region.into());
    }

    fn stage_regions(&mut self, regions: impl IntoIterator<Item = (RegionPos, Arc<Region>)>) {
        self.pending_regions.extend(regions);
    }

    pub fn load_player(&mut self, uuid: PlayerUuid) -> Result<Option<RestoredPlayer>, StoreError> {
        self.load_player_record(uuid)
            .and_then(|record| record.map(restore_player).transpose())
            .map_err(|source| StoreError::PlayerLoad {
                uuid,
                source: Box::new(source),
            })
    }

    fn load_player_record(&mut self, uuid: PlayerUuid) -> Result<Option<PlayerRecord>, StoreError> {
        if let Some(record) = self.pending_players.get(&uuid) {
            return Ok(Some(record.clone()));
        }
        if let Some(record) = self
            .in_flight
            .as_ref()
            .and_then(|batch| batch.players.get(&uuid))
        {
            return Ok(Some(record.clone()));
        }
        self.store
            .as_ref()
            .map_or(Ok(None), |store| store.load_player(uuid))
    }

    pub fn stage_players<'a>(
        &mut self,
        players: impl IntoIterator<Item = &'a Player>,
    ) -> Result<(), StoreError> {
        let records = players
            .into_iter()
            .map(|player| Ok((player.uuid, snapshot_player(player)?)))
            .collect::<Result<Vec<_>, StoreError>>()?;
        self.pending_players.extend(records);
        Ok(())
    }

    fn start_save(&mut self) -> Result<(), StoreError> {
        if self.store.is_none() || self.in_flight.is_some() || !self.has_pending() {
            return Ok(());
        }
        let batch = Arc::new(self.take_batch());
        let worker = self.worker.as_ref().ok_or(StoreError::WorkerDisconnected)?;
        if worker.save(Arc::clone(&batch)).is_err() {
            self.restore_batch(batch);
            return Err(StoreError::WorkerDisconnected);
        }
        self.in_flight = Some(batch);
        Ok(())
    }

    pub fn drain_completions(&mut self) -> Result<PersistenceCompletions, StoreError> {
        let mut completed = PersistenceCompletions::default();
        let messages = self
            .worker
            .as_ref()
            .ok_or(StoreError::WorkerDisconnected)?
            .drain_completions()?;
        for message in messages {
            self.apply_completion(message, &mut completed, false)?;
        }
        Ok(completed)
    }

    pub fn shutdown(&mut self) -> Result<(), StoreError> {
        let save_result = self.finish_in_flight();
        let shutdown_result = self.shutdown_worker();
        save_result.and(shutdown_result)
    }

    fn finish_in_flight(&mut self) -> Result<(), StoreError> {
        while self.in_flight.is_some() {
            let message = self
                .worker
                .as_ref()
                .ok_or(StoreError::WorkerDisconnected)?
                .recv_completion()?;
            let mut completed = PersistenceCompletions::default();
            self.apply_completion(message, &mut completed, true)?;
        }
        Ok(())
    }

    fn has_pending(&self) -> bool {
        !self.pending_regions.is_empty()
            || !self.pending_players.is_empty()
            || self.pending_meta.is_some()
    }

    fn take_batch(&mut self) -> SaveBatch {
        SaveBatch {
            regions: std::mem::take(&mut self.pending_regions),
            players: std::mem::take(&mut self.pending_players),
            meta: self.pending_meta.take(),
        }
    }

    fn restore_batch(&mut self, batch: Arc<SaveBatch>) {
        let batch = Arc::try_unwrap(batch).unwrap_or_else(|batch| (*batch).clone());
        for (pos, region) in batch.regions {
            self.pending_regions.entry(pos).or_insert(region);
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
                self.take_in_flight()?;
            }
            WorkerCompletion::SaveFailed(error) => {
                let batch = self.take_in_flight()?;
                self.restore_batch(batch);
                if durable {
                    return Err(error);
                }
                tracing::error!("persistence batch failed: {error}");
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
        worker.shutdown()
    }
}

pub fn autosave(
    sim: &CellWorld,
    regions: &RegionMap,
    info: &WorldInfo,
    clock: &Calendar,
    players: &Players,
    persistence: &mut Persistence,
) -> Result<(), StoreError> {
    let tick = sim.tick();
    if persistence.store.is_none() || tick == 0 || !tick.is_multiple_of(AUTOSAVE_INTERVAL_TICKS) {
        return Ok(());
    }

    stage_world_snapshot(sim, regions, players, persistence, info, clock)?;
    persistence.start_save()
}

fn stage_world_snapshot(
    sim: &CellWorld,
    regions: &RegionMap,
    players: &Players,
    persistence: &mut Persistence,
    info: &WorldInfo,
    clock: &Calendar,
) -> Result<(), StoreError> {
    persistence.stage_players(players.iter().map(|(_, player)| player))?;
    persistence.stage_regions(snapshot_regions(sim, regions));
    persistence.stage_meta(world_meta(info, clock, sim.tick()));
    Ok(())
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
