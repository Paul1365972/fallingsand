use crate::framing::Closed;
use crate::{Connection, ConnectionStatus, Listener};
use bytes::Bytes;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};

pub(crate) struct MemoryConnection {
    tx: Sender<Bytes>,
    rx: Mutex<Receiver<Bytes>>,
    closed: Arc<Closed>,
}

pub(crate) fn memory_pair() -> (MemoryConnection, MemoryConnection) {
    let (ab_tx, ab_rx) = channel();
    let (ba_tx, ba_rx) = channel();
    let closed = Arc::new(Closed::default());
    (
        MemoryConnection {
            tx: ab_tx,
            rx: Mutex::new(ba_rx),
            closed: closed.clone(),
        },
        MemoryConnection {
            tx: ba_tx,
            rx: Mutex::new(ab_rx),
            closed,
        },
    )
}

impl Connection for MemoryConnection {
    fn send(&mut self, message: Vec<u8>) {
        let _ = self.tx.send(Bytes::from(message));
    }

    fn poll(&mut self) -> Option<Bytes> {
        match self.rx.lock().unwrap().try_recv() {
            Ok(message) => Some(message),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.closed.hint("peer dropped");
                None
            }
        }
    }

    fn status(&self) -> ConnectionStatus {
        self.closed.status()
    }

    fn close(&mut self, reason: &str) {
        self.closed.mark(reason);
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
