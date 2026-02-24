use fallingsand_sim::network::{Client, ClientId, ClientMap};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::mpsc::error::{TryRecvError, TrySendError};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_async, WebSocketStream};

const TIMEOUT_DURATION: Duration = Duration::from_secs(10);
const CHANNEL_BUFFER_SIZE: usize = 1024;

struct Connection {
    to_socket: Sender<Vec<u8>>,
    from_socket: Receiver<Vec<u8>>,
    last_active: Instant,
}

pub struct NetworkManager {
    runtime: Runtime,
    connections: HashMap<ClientId, Connection>,
    new_connection_rx: mpsc::Receiver<(Sender<Vec<u8>>, Receiver<Vec<u8>>)>,
    next_id: ClientId,
}

impl NetworkManager {
    pub fn new(addr: &str) -> Self {
        // Create runtime with 2 threads
        let runtime = Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime");

        // Channel for receiving new connections
        let (new_connection_tx, new_connection_rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);

        // Start the websocket listener
        let addr = addr.to_owned();
        runtime.spawn(async move {
            let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
            println!("WebSocket server listening on: {}", addr);

            loop {
                if let Ok((stream, _)) = listener.accept().await {
                    // Spawn a new task for each connection
                    let new_conn_tx = new_connection_tx.clone();
                    tokio::spawn(async move {
                        if let Ok(ws_stream) = accept_async(stream).await {
                            handle_connection(ws_stream, new_conn_tx).await;
                        }
                    });
                }
            }
        });

        NetworkManager {
            runtime,
            connections: HashMap::new(),
            new_connection_rx,
            next_id: 1,
        }
    }

    pub fn tick(&mut self, clients: &mut ClientMap) {
        let now = Instant::now();
        // Process any new connections
        while let Ok((to_socket, from_socket)) = self.new_connection_rx.try_recv() {
            let id = self.next_id;
            self.next_id += 1;

            let connection = Connection {
                to_socket,
                from_socket,
                last_active: now,
            };

            self.connections.insert(id, connection);
            clients.insert(
                id,
                Client {
                    send_buffer: Vec::new(),
                    receive_buffer: Vec::new(),
                },
            );
        }

        let mut to_remove = Vec::new();
        for (id, connection) in self.connections.iter_mut() {
            let client = clients.get_mut(id).unwrap();

            if tick_connection(connection, client, now).is_err() {
                to_remove.push(*id);
            }
        }

        for id in to_remove {
            self.connections.remove(&id);
            clients.remove(&id);
        }
    }

    pub fn close(self) {
        drop(self.connections);
        drop(self.new_connection_rx);
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
        match connection.to_socket.try_send(data) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) | Err(TrySendError::Closed(_)) => {
                return Err(());
            }
        }
    }

    loop {
        match connection.from_socket.try_recv() {
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
    ws_stream: WebSocketStream<TcpStream>,
    new_connection_tx: Sender<(Sender<Vec<u8>>, Receiver<Vec<u8>>)>,
) {
    let (ws_sender, ws_receiver) = ws_stream.split();

    // Create channels for this connection
    let (to_socket_tx, mut to_socket_rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);
    let (from_socket_tx, from_socket_rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);

    // Send the channels to NetworkManager
    if new_connection_tx
        .send((to_socket_tx, from_socket_rx))
        .await
        .is_err()
    {
        return; // NetworkManager is gone
    }

    let mut ws_sender = ws_sender;
    tokio::spawn(async move {
        while let Some(data) = to_socket_rx.recv().await {
            if ws_sender.send(Message::Binary(data)).await.is_err() {
                break;
            }
        }
    });

    let mut ws_receiver = ws_receiver;
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
