use futures_util::{SinkExt, StreamExt};
use rustc_hash::FxHashMap;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::runtime::{Builder, Runtime};
use tokio::sync::mpsc::error::{TryRecvError, TrySendError};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

pub struct Client {
    pub send_buffer: Vec<u8>,
    pub receive_buffer: Vec<u8>,
    pub should_close: bool,
}

const TIMEOUT_DURATION: Duration = Duration::from_secs(10);
const CHANNEL_BUFFER_SIZE: usize = 1024;

struct Connection {
    to_socket_tx: Sender<Vec<u8>>,
    from_socket_rx: Receiver<Vec<u8>>,
    last_active: Instant,
}

pub struct NetworkClient {
    runtime: Runtime,
    connection: Option<Connection>,
}

impl NetworkClient {
    pub fn new() -> Self {
        let runtime = Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime");

        NetworkClient {
            runtime,
            connection: None,
        }
    }

    pub fn connect(&mut self, url: &str) {
        assert!(self.connection.is_none());
        let (to_socket_tx, to_socket_rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);
        let (from_socket_tx, from_socket_rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);

        let url = url.to_owned();
        self.runtime.spawn(async move {
            match connect_async(&url).await {
                Ok((ws_stream, _)) => {
                    handle_connection(ws_stream, to_socket_rx, from_socket_tx).await;
                }
                Err(e) => eprintln!("Failed to connect to WebSocket server: {}", e),
            }
        });

        self.connection = Some(Connection {
            to_socket_tx,
            from_socket_rx,
            last_active: Instant::now(),
        });
    }

    pub fn disconnect(&mut self) {
        drop(self.connection.take());
    }

    pub fn tick(&mut self, client: &mut Option<Client>) {
        let now = Instant::now();

        // Handle initial connection if we don't have one
        if self.connection.is_some() && client.is_none() {
            *client = Some(Client {
                send_buffer: Vec::new(),
                receive_buffer: Vec::new(),
                should_close: false,
            });
        }

        // Process existing connection
        if let Some(connection) = &mut self.connection {
            if tick_connection(connection, client.as_mut().unwrap(), now).is_err() {
                // Connection lost
                self.connection = None;
                *client = None;
            }
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }

    pub fn close(mut self) {
        self.disconnect();
        self.runtime.shutdown_timeout(Duration::from_secs_f32(1.0));
    }
}

fn tick_connection(
    connection: &mut Connection,
    client: &mut Client,
    now: Instant,
) -> Result<(), ()> {
    if now.duration_since(connection.last_active) > TIMEOUT_DURATION {
        return Err(());
    }

    if !client.send_buffer.is_empty() {
        let data = client.send_buffer.clone();
        client.send_buffer.clear();
        match connection.to_socket_tx.try_send(data) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) | Err(TrySendError::Closed(_)) => {
                return Err(());
            }
        }
    }

    loop {
        match connection.from_socket_rx.try_recv() {
            Ok(data) => {
                client.receive_buffer.extend_from_slice(&data);
                connection.last_active = now;
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                return Err(());
            }
        }
    }
    Ok(())
}

async fn handle_connection(
    ws_stream: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
    mut to_socket_rx: mpsc::Receiver<Vec<u8>>,
    from_socket_tx: mpsc::Sender<Vec<u8>>,
) {
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    tokio::spawn(async move {
        while let Some(data) = to_socket_rx.recv().await {
            if ws_sender.send(Message::Binary(data)).await.is_err() {
                break;
            }
        }
    });

    tokio::spawn(async move {
        while let Some(Ok(message)) = ws_receiver.next().await {
            match message {
                Message::Binary(data) => {
                    if from_socket_tx.send(data).await.is_err() {
                        break;
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });
}
