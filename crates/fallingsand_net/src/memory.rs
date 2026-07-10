use crate::{Connection, ConnectionStatus, Listener};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Shared {
    closed: Mutex<Option<String>>,
}

pub(crate) struct MemoryConnection {
    tx: Sender<Vec<u8>>,
    rx: Mutex<Receiver<Vec<u8>>>,
    shared: Arc<Shared>,
}

pub(crate) fn memory_pair() -> (MemoryConnection, MemoryConnection) {
    let (ab_tx, ab_rx) = channel();
    let (ba_tx, ba_rx) = channel();
    let shared = Arc::new(Shared::default());
    (
        MemoryConnection {
            tx: ab_tx,
            rx: Mutex::new(ba_rx),
            shared: shared.clone(),
        },
        MemoryConnection {
            tx: ba_tx,
            rx: Mutex::new(ab_rx),
            shared,
        },
    )
}

impl Connection for MemoryConnection {
    fn send(&mut self, message: Vec<u8>) {
        let _ = self.tx.send(message);
    }

    fn poll(&mut self) -> Option<Vec<u8>> {
        match self.rx.lock().unwrap().try_recv() {
            Ok(message) => Some(message),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                let mut closed = self.shared.closed.lock().unwrap();
                if closed.is_none() {
                    *closed = Some("peer dropped".into());
                }
                None
            }
        }
    }

    fn status(&self) -> ConnectionStatus {
        match self.shared.closed.lock().unwrap().clone() {
            Some(reason) => ConnectionStatus::Closed { reason },
            None => ConnectionStatus::Connected,
        }
    }

    fn close(&mut self, reason: &str) {
        let mut closed = self.shared.closed.lock().unwrap();
        if closed.is_none() {
            *closed = Some(reason.to_string());
        }
    }
}

pub struct MemoryListener {
    incoming: Mutex<Receiver<MemoryConnection>>,
}

#[derive(Clone)]
pub struct MemoryDialer {
    listener: Sender<MemoryConnection>,
}

pub fn memory_listener() -> (MemoryListener, MemoryDialer) {
    let (tx, rx) = channel();
    (
        MemoryListener {
            incoming: Mutex::new(rx),
        },
        MemoryDialer { listener: tx },
    )
}

impl MemoryDialer {
    pub fn connect(&self) -> Option<Box<dyn Connection>> {
        let (client, server) = memory_pair();
        self.listener.send(server).ok()?;
        Some(Box::new(client))
    }
}

impl Listener for MemoryListener {
    fn poll_accept(&mut self) -> Option<Box<dyn Connection>> {
        Some(Box::new(self.incoming.lock().unwrap().try_recv().ok()?))
    }
}
