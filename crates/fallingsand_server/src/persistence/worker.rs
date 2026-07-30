use super::store::WorldStore;
use super::{RegionLoad, RegionReady, SaveBatch, StoreError};
use fallingsand_core::RegionPos;
use fallingsand_worldgen::WorldGenerator;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread::{self, JoinHandle};

enum WorkerCommand {
    LoadRegion { request: u64, pos: RegionPos },
    SaveBatch(Arc<SaveBatch>),
    Shutdown,
}

pub(super) enum WorkerCompletion {
    RegionReady(RegionReady),
    SaveComplete,
    SaveFailed(StoreError),
}

pub(super) struct PersistenceWorker {
    commands: Sender<WorkerCommand>,
    completions: Receiver<WorkerCompletion>,
    thread: Option<JoinHandle<()>>,
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
            thread: Some(thread),
        })
    }

    pub(super) fn request_region(
        &mut self,
        request: u64,
        pos: RegionPos,
    ) -> Result<(), StoreError> {
        self.dispatch(WorkerCommand::LoadRegion { request, pos })
    }

    pub(super) fn save(&mut self, batch: Arc<SaveBatch>) -> Result<(), StoreError> {
        self.dispatch(WorkerCommand::SaveBatch(batch))
    }

    fn dispatch(&mut self, command: WorkerCommand) -> Result<(), StoreError> {
        match self.commands.send(command) {
            Ok(()) => Ok(()),
            Err(_) => Err(self.died()),
        }
    }

    pub(super) fn drain_completions(&mut self) -> Result<Vec<WorkerCompletion>, StoreError> {
        let mut messages = Vec::new();
        loop {
            match self.completions.try_recv() {
                Ok(message) => messages.push(message),
                Err(TryRecvError::Empty) => return Ok(messages),
                Err(TryRecvError::Disconnected) => return Err(self.died()),
            }
        }
    }

    pub(super) fn recv_completion(&mut self) -> Result<WorkerCompletion, StoreError> {
        match self.completions.recv() {
            Ok(message) => Ok(message),
            Err(_) => Err(self.died()),
        }
    }

    pub(super) fn shutdown(self) -> Result<(), StoreError> {
        let Self {
            commands,
            completions,
            thread,
        } = self;
        drop(completions);
        let _ = commands.send(WorkerCommand::Shutdown);
        drop(commands);
        match join(thread) {
            Some(message) => Err(StoreError::WorkerPanicked(message)),
            None => Ok(()),
        }
    }

    fn died(&mut self) -> StoreError {
        match join(self.thread.take()) {
            Some(message) => StoreError::WorkerPanicked(message),
            None => StoreError::WorkerDisconnected,
        }
    }
}

fn join(thread: Option<JoinHandle<()>>) -> Option<String> {
    thread?
        .join()
        .err()
        .map(fallingsand_core::diagnostics::panic_message)
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
                match store
                    .as_ref()
                    .map_or(Ok(()), |store| store.save_batch(&batch))
                {
                    Ok(()) => WorkerCompletion::SaveComplete,
                    Err(error) => WorkerCompletion::SaveFailed(error),
                }
            }
            WorkerCommand::Shutdown => break,
        };
        if completions.send(completion).is_err() {
            break;
        }
    }
}
