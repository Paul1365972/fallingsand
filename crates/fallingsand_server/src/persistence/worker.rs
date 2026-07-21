use super::store::WorldStore;
use super::{RegionLoad, RegionReady, SaveBatch, StoreError};
use fallingsand_core::RegionPos;
use fallingsand_worldgen::WorldGenerator;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};

pub(super) enum WorkerCommand {
    LoadRegion { request: u64, pos: RegionPos },
    SaveBatch(Arc<SaveBatch>),
    Shutdown,
}

pub(super) enum WorkerCompletion {
    RegionReady(RegionReady),
    SaveComplete,
    SaveFailed(StoreError),
    Fatal(StoreError),
}

pub(super) struct PersistenceWorker {
    pub commands: Sender<WorkerCommand>,
    pub completions: Receiver<WorkerCompletion>,
    pub thread: JoinHandle<()>,
}

impl PersistenceWorker {
    pub(super) fn start(store: Option<Arc<WorldStore>>, seed: u64) -> Result<Self, StoreError> {
        let (command_tx, command_rx) = channel();
        let (completion_tx, completion_rx) = channel();
        let thread = thread::Builder::new()
            .name("region-storage".into())
            .spawn(move || worker_main(store, WorldGenerator::new(seed), command_rx, completion_tx))
            .map_err(|error| StoreError::WorkerStart(error.to_string()))?;
        Ok(Self {
            commands: command_tx,
            completions: completion_rx,
            thread,
        })
    }
}

fn worker_main(
    store: Option<Arc<WorldStore>>,
    generator: WorldGenerator,
    commands: Receiver<WorkerCommand>,
    completions: Sender<WorkerCompletion>,
) {
    while let Ok(command) = commands.recv() {
        let completion = match command {
            WorkerCommand::LoadRegion { request, pos } => {
                let result = store
                    .as_ref()
                    .map_or(Ok(None), |store| store.load_region(pos))
                    .map(|loaded| RegionLoad {
                        region: loaded.unwrap_or_else(|| generator.generate_region(pos)),
                        revision: 0,
                        persisted_revision: 0,
                    })
                    .map_err(|source| StoreError::RegionLoad {
                        pos,
                        source: Box::new(source),
                    });
                WorkerCompletion::RegionReady(RegionReady {
                    request,
                    pos,
                    result,
                })
            }
            WorkerCommand::SaveBatch(batch) => {
                let materialized = batch
                    .regions
                    .iter()
                    .map(|(pos, snapshot)| {
                        Ok((
                            *pos,
                            snapshot.materialize(store.as_deref(), &generator, *pos)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, StoreError>>();
                match materialized {
                    Ok(regions) => match store
                        .as_ref()
                        .map_or(Ok(()), |store| store.save_batch(&batch, &regions))
                    {
                        Ok(()) => WorkerCompletion::SaveComplete,
                        Err(error) => WorkerCompletion::SaveFailed(error),
                    },
                    Err(error) => WorkerCompletion::Fatal(error),
                }
            }
            WorkerCommand::Shutdown => break,
        };
        if completions.send(completion).is_err() {
            break;
        }
    }
}
