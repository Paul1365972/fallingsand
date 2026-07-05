use crate::framing::{Closed, FrameBuffer, encode_frame};
use crate::{Connection, ConnectionStatus};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use wasm_bindgen_futures::spawn_local;

pub struct WtWasmConnection {
    tx: UnboundedSender<Vec<u8>>,
    rx: Mutex<Receiver<Vec<u8>>>,
    close_tx: UnboundedSender<String>,
    closed: Arc<Closed>,
}

pub fn connect(url: &str, cert_hash: Option<Vec<u8>>) -> WtWasmConnection {
    let closed = Arc::new(Closed::default());
    let (out_tx, out_rx) = unbounded::<Vec<u8>>();
    let (close_tx, close_rx) = unbounded::<String>();
    let (in_tx, in_rx) = channel::<Vec<u8>>();

    let url = url.to_string();
    let task_closed = closed.clone();
    spawn_local(async move {
        if let Err(err) =
            run_session(url, cert_hash, out_rx, close_rx, in_tx, task_closed.clone()).await
        {
            task_closed.mark(&err);
        }
    });

    WtWasmConnection {
        tx: out_tx,
        rx: Mutex::new(in_rx),
        close_tx,
        closed,
    }
}

async fn run_session(
    url: String,
    cert_hash: Option<Vec<u8>>,
    mut out_rx: UnboundedReceiver<Vec<u8>>,
    mut close_rx: UnboundedReceiver<String>,
    in_tx: Sender<Vec<u8>>,
    closed: Arc<Closed>,
) -> Result<(), String> {
    let parsed = crate::normalize_server_url(&url).map_err(|_| "invalid url".to_string())?;
    let builder = web_transport_wasm::ClientBuilder::new();
    let client = match cert_hash {
        Some(hash) => builder.with_server_certificate_hashes(vec![hash]),
        None => builder.with_system_roots(),
    };
    let session = client
        .connect(parsed)
        .await
        .map_err(|err| err.to_string())?;
    let (mut send_stream, mut recv_stream) =
        session.open_bi().await.map_err(|err| err.to_string())?;

    let reader_closed = closed.clone();
    spawn_local(async move {
        let mut frames = FrameBuffer::default();
        loop {
            match recv_stream.read(usize::MAX).await {
                Ok(Some(chunk)) => {
                    frames.push(&chunk);
                    loop {
                        match frames.next_frame() {
                            Ok(Some(frame)) => {
                                if in_tx.send(frame).is_err() {
                                    return;
                                }
                            }
                            Ok(None) => break,
                            Err(()) => {
                                reader_closed.mark("oversized frame");
                                return;
                            }
                        }
                    }
                }
                Ok(None) => {
                    reader_closed.mark("stream closed");
                    return;
                }
                Err(err) => {
                    reader_closed.mark(&err.to_string());
                    return;
                }
            }
        }
    });

    let close_session = session.clone();
    spawn_local(async move {
        if let Some(reason) = close_rx.next().await {
            close_session.close(0, &reason);
        }
    });

    let closed_watch = closed.clone();
    spawn_local(async move {
        let err = session.closed().await;
        closed_watch.mark(&err.to_string());
    });

    while let Some(message) = out_rx.next().await {
        if send_stream.write(&encode_frame(&message)).await.is_err() {
            return Err("write failed".into());
        }
    }
    Ok(())
}

impl Connection for WtWasmConnection {
    fn send(&mut self, message: Vec<u8>) {
        let _ = self.tx.unbounded_send(message);
    }

    fn poll(&mut self) -> Option<Vec<u8>> {
        match self.rx.lock().unwrap().try_recv() {
            Ok(message) => Some(message),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }

    fn status(&self) -> ConnectionStatus {
        self.closed.status()
    }

    fn close(&mut self, reason: &str) {
        self.closed.mark(reason);
        let _ = self.close_tx.unbounded_send(reason.to_string());
    }
}
