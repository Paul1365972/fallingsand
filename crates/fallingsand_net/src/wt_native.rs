use crate::framing::{Closed, FrameBuffer, encode_frame};
use crate::{Connection, ConnectionStatus, Listener};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

pub struct WtConnection {
    tx: UnboundedSender<Vec<u8>>,
    rx: Mutex<Receiver<Vec<u8>>>,
    closed: Arc<Closed>,
    session: web_transport_quinn::Session,
}

impl WtConnection {
    pub fn new(
        runtime: &tokio::runtime::Handle,
        session: web_transport_quinn::Session,
        send_stream: web_transport_quinn::SendStream,
        recv_stream: web_transport_quinn::RecvStream,
    ) -> Self {
        let closed = Arc::new(Closed::default());
        let (out_tx, out_rx) = unbounded_channel::<Vec<u8>>();
        let (in_tx, in_rx) = channel::<Vec<u8>>();

        runtime.spawn(writer(send_stream, out_rx));
        runtime.spawn(reader(recv_stream, in_tx, closed.clone()));
        runtime.spawn(watch_closed(session.clone(), closed.clone()));

        Self {
            tx: out_tx,
            rx: Mutex::new(in_rx),
            closed,
            session,
        }
    }
}

async fn writer(
    mut stream: web_transport_quinn::SendStream,
    mut messages: UnboundedReceiver<Vec<u8>>,
) {
    while let Some(message) = messages.recv().await {
        if stream.write_all(&encode_frame(&message)).await.is_err() {
            return;
        }
    }
    let _ = stream.finish();
}

async fn reader(
    mut stream: web_transport_quinn::RecvStream,
    messages: Sender<Vec<u8>>,
    closed: Arc<Closed>,
) {
    let mut frames = FrameBuffer::default();
    loop {
        match stream.read_chunk(64 * 1024, true).await {
            Ok(Some(chunk)) => {
                frames.push(&chunk.bytes);
                loop {
                    match frames.next_frame() {
                        Ok(Some(frame)) => {
                            if messages.send(frame).is_err() {
                                return;
                            }
                        }
                        Ok(None) => break,
                        Err(()) => {
                            closed.mark("oversized frame");
                            return;
                        }
                    }
                }
            }
            Ok(None) => {
                closed.mark("stream closed");
                return;
            }
            Err(err) => {
                closed.mark(&err.to_string());
                return;
            }
        }
    }
}

async fn watch_closed(session: web_transport_quinn::Session, closed: Arc<Closed>) {
    let err = session.closed().await;
    closed.mark(&err.to_string());
}

impl Connection for WtConnection {
    fn send(&mut self, message: Vec<u8>) {
        let _ = self.tx.send(message);
    }

    fn poll(&mut self) -> Option<Vec<u8>> {
        match self.rx.lock().unwrap().try_recv() {
            Ok(message) => Some(message),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => None,
        }
    }

    fn status(&self) -> ConnectionStatus {
        self.closed.status()
    }

    fn close(&mut self, reason: &str) {
        self.closed.mark(reason);
        self.session.close(0, reason.as_bytes());
    }
}

pub struct WtListener {
    incoming: Mutex<Receiver<WtConnection>>,
}

impl WtListener {
    pub fn bind(
        runtime: tokio::runtime::Handle,
        addr: std::net::SocketAddr,
        cert_chain: Vec<rustls_pki_types::CertificateDer<'static>>,
        key: rustls_pki_types::PrivateKeyDer<'static>,
    ) -> anyhow::Result<Self> {
        let (tx, rx) = channel::<WtConnection>();
        let server = {
            let _guard = runtime.enter();
            web_transport_quinn::ServerBuilder::new()
                .with_addr(addr)
                .with_certificate(cert_chain, key)?
        };
        let accept_runtime = runtime.clone();
        runtime.spawn(async move {
            let mut server = server;
            while let Some(request) = server.accept().await {
                let Ok(session) = request.ok().await else {
                    continue;
                };
                let Ok((send_stream, recv_stream)) = session.accept_bi().await else {
                    continue;
                };
                let conn = WtConnection::new(&accept_runtime, session, send_stream, recv_stream);
                if tx.send(conn).is_err() {
                    return;
                }
            }
        });
        Ok(Self {
            incoming: Mutex::new(rx),
        })
    }
}

impl Listener for WtListener {
    fn poll_accept(&mut self) -> Option<Box<dyn Connection>> {
        Some(Box::new(self.incoming.lock().unwrap().try_recv().ok()?))
    }
}

pub fn connect(
    runtime: tokio::runtime::Handle,
    url: &str,
    cert_hash: Option<Vec<u8>>,
) -> anyhow::Result<WtConnection> {
    let url: url::Url = url.parse()?;
    let connect_runtime = runtime.clone();
    let session = runtime.block_on(async move {
        let builder = web_transport_quinn::ClientBuilder::new();
        let client = match cert_hash {
            Some(hash) => builder.with_server_certificate_hashes(vec![hash])?,
            None => builder.with_system_roots()?,
        };
        let session = client.connect(url).await?;
        let (send_stream, recv_stream) = session.open_bi().await?;
        anyhow::Ok((session, send_stream, recv_stream))
    })?;
    let (session, send_stream, recv_stream) = session;
    Ok(WtConnection::new(
        &connect_runtime,
        session,
        send_stream,
        recv_stream,
    ))
}
