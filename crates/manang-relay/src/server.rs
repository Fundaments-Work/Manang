//! The relay server: rooms, limits, forwarding.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Instant, interval_at, sleep_until, timeout};
use tokio_tungstenite::tungstenite::Bytes;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{
    Callback, ErrorResponse, Request, Response,
};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tracing::{debug, info, warn};

use manang_live::envelope::{HEADER_LEN, Kind, TAG_LEN, VERSION};

use crate::config::RelayConfig;

type RoomId = String;

/// Handshake callback: accept only `/room/<32 hex>` and remember the room.
struct RoomPath(Arc<Mutex<Option<RoomId>>>);

impl Callback for RoomPath {
    fn on_request(self, req: &Request, resp: Response) -> Result<Response, ErrorResponse> {
        match parse_room(req.uri().path()) {
            Some(room) => {
                *self.0.lock() = Some(room);
                Ok(resp)
            }
            None => {
                let mut err = ErrorResponse::new(Some("expected /room/<32 hex>".to_owned()));
                *err.status_mut() = StatusCode::NOT_FOUND;
                Err(err)
            }
        }
    }
}

struct Room {
    peers: HashMap<u64, mpsc::Sender<Bytes>>,
}

struct State {
    config: RelayConfig,
    rooms: Mutex<HashMap<RoomId, Room>>,
    connections: Arc<Semaphore>,
}

/// A running relay.
pub struct RunningRelay {
    /// The address it listens on (useful when bound to port 0).
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl RunningRelay {
    /// Stop accepting, drop all connections and wait for the accept loop.
    pub async fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let _ = self.task.await;
    }
}

/// Bind `addr` and serve in the background.
pub async fn spawn(addr: SocketAddr, config: RelayConfig) -> std::io::Result<RunningRelay> {
    let listener = TcpListener::bind(addr).await?;
    let addr = listener.local_addr()?;
    let (tx, rx) = oneshot::channel();
    let state = Arc::new(State {
        connections: Arc::new(Semaphore::new(config.max_connections)),
        config,
        rooms: Mutex::new(HashMap::new()),
    });
    info!(%addr, "relay listening");
    let task = tokio::spawn(accept_loop(listener, state, rx));
    Ok(RunningRelay {
        addr,
        shutdown: Some(tx),
        task,
    })
}

async fn accept_loop(
    listener: TcpListener,
    state: Arc<State>,
    mut shutdown: oneshot::Receiver<()>,
) {
    let (stop_tx, _) = tokio::sync::broadcast::channel::<()>(1);
    let mut next_id: u64 = 0;
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => {
                let Ok((stream, peer_addr)) = accepted else { continue };
                let Ok(permit) = Arc::clone(&state.connections).try_acquire_owned() else {
                    warn!(%peer_addr, "connection limit reached; refusing");
                    continue;
                };
                next_id += 1;
                let id = next_id;
                let state = Arc::clone(&state);
                let mut stop = stop_tx.subscribe();
                tokio::spawn(async move {
                    tokio::select! {
                        _ = stop.recv() => {}
                        () = handle_connection(stream, id, state) => {}
                    }
                    drop(permit);
                });
            }
        }
    }
    let _ = stop_tx.send(());
    info!("relay stopped");
}

fn parse_room(path: &str) -> Option<RoomId> {
    let id = path.strip_prefix("/room/")?;
    let ok = id.len() == 32 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    ok.then(|| id.to_owned())
}

/// Short, non-reversible-enough tag for logs: the first 8 hex chars of the id.
fn log_tag(room: &str) -> &str {
    &room[..8.min(room.len())]
}

async fn handle_connection(mut stream: TcpStream, id: u64, state: Arc<State>) {
    // Plain HTTP health check: answer without a WebSocket handshake. The
    // request is read to its end first so closing does not reset the socket.
    let mut head = [0u8; 64];
    if let Ok(n) = stream.peek(&mut head).await
        && head[..n].starts_with(b"GET /health ")
    {
        let mut buf = vec![0u8; 4096];
        let mut filled = 0;
        let _ = timeout(Duration::from_secs(2), async {
            while filled < buf.len() {
                match stream.read(&mut buf[filled..]).await {
                    Ok(0) | Err(_) => break,
                    Ok(k) => {
                        filled += k;
                        if buf[..filled].windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                }
            }
        })
        .await;
        let _ = stream
            .write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: 3\r\nconnection: close\r\n\r\nok\n",
            )
            .await;
        let _ = stream.shutdown().await;
        return;
    }

    let room_slot: Arc<Mutex<Option<RoomId>>> = Arc::default();
    let callback = RoomPath(Arc::clone(&room_slot));
    let ws_config = WebSocketConfig::default()
        .max_message_size(Some(state.config.max_frame_bytes))
        .max_frame_size(Some(state.config.max_frame_bytes));
    let ws = match timeout(
        state.config.handshake_timeout,
        tokio_tungstenite::accept_hdr_async_with_config(stream, callback, Some(ws_config)),
    )
    .await
    {
        Ok(Ok(ws)) => ws,
        Ok(Err(e)) => {
            debug!(conn = id, error = %e, "handshake failed");
            return;
        }
        Err(_) => {
            debug!(conn = id, "handshake timed out");
            return;
        }
    };
    let Some(room) = room_slot.lock().take() else {
        return;
    };
    run_peer(ws, id, room, state).await;
}

enum Join {
    Joined(mpsc::Receiver<Bytes>),
    RoomsFull,
    RoomFull,
}

fn join(state: &State, room: &RoomId, id: u64) -> Join {
    let (tx, rx) = mpsc::channel(state.config.outbound_queue);
    let mut rooms = state.rooms.lock();
    if !rooms.contains_key(room) && rooms.len() >= state.config.max_rooms {
        return Join::RoomsFull;
    }
    let entry = rooms.entry(room.clone()).or_insert_with(|| Room {
        peers: HashMap::new(),
    });
    if entry.peers.len() >= state.config.max_peers_per_room {
        return Join::RoomFull;
    }
    entry.peers.insert(id, tx);
    Join::Joined(rx)
}

fn leave(state: &State, room: &RoomId, id: u64) -> usize {
    let mut rooms = state.rooms.lock();
    let Some(entry) = rooms.get_mut(room) else {
        return 0;
    };
    entry.peers.remove(&id);
    let remaining = entry.peers.len();
    if remaining == 0 {
        rooms.remove(room);
    }
    remaining
}

/// Forward to everyone else. A receiver whose queue is full is too slow to
/// keep in sync; it is dropped from the room (its connection then closes and
/// the client reconnects and resyncs) rather than letting memory grow.
fn broadcast(state: &State, room: &RoomId, from: u64, frame: &Bytes) {
    let mut rooms = state.rooms.lock();
    let Some(entry) = rooms.get_mut(room) else {
        return;
    };
    let mut slow = Vec::new();
    for (peer, tx) in &entry.peers {
        if *peer != from && tx.try_send(frame.clone()).is_err() {
            slow.push(*peer);
        }
    }
    for peer in slow {
        entry.peers.remove(&peer);
        warn!(conn = peer, room = log_tag(room), "dropped slow receiver");
    }
}

struct Bucket {
    capacity: f64,
    tokens: f64,
    per_sec: f64,
    last: Instant,
}

impl Bucket {
    fn new(per_sec: u64) -> Self {
        let per_sec = per_sec as f64;
        let capacity = per_sec * 2.0;
        Self {
            capacity,
            tokens: capacity,
            per_sec,
            last: Instant::now(),
        }
    }

    fn take(&mut self, amount: f64) -> bool {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + dt * self.per_sec).min(self.capacity);
        if self.tokens >= amount {
            self.tokens -= amount;
            true
        } else {
            false
        }
    }
}

/// What the relay can check without the key.
fn plausible_envelope(frame: &[u8]) -> Option<Kind> {
    if frame.len() < HEADER_LEN + TAG_LEN || frame[0] != VERSION {
        return None;
    }
    match frame[1] {
        1 => Some(Kind::Data),
        2 => Some(Kind::Presence),
        _ => None,
    }
}

async fn run_peer(
    ws: tokio_tungstenite::WebSocketStream<TcpStream>,
    id: u64,
    room: RoomId,
    state: Arc<State>,
) {
    let (mut sink, mut stream) = ws.split();
    let mut rx = match join(&state, &room, id) {
        Join::Joined(rx) => rx,
        refused => {
            let reason = match refused {
                Join::RoomsFull => "relay is full",
                _ => "room is full",
            };
            let _ = sink
                .send(Message::Close(Some(CloseFrame {
                    code: CloseCode::Again,
                    reason: reason.into(),
                })))
                .await;
            return;
        }
    };
    info!(conn = id, room = log_tag(&room), "joined");

    let cfg = &state.config;
    let mut bytes_bucket = Bucket::new(cfg.rate_bytes_per_sec);
    let mut msgs_bucket = Bucket::new(cfg.rate_msgs_per_sec);
    let mut presence_bucket = Bucket::new(cfg.presence_msgs_per_sec);
    let mut ping = interval_at(Instant::now() + cfg.ping_interval, cfg.ping_interval);
    let mut deadline = Instant::now() + cfg.idle_timeout;
    let mut close: Option<(CloseCode, &'static str)> = None;

    loop {
        tokio::select! {
            incoming = stream.next() => {
                deadline = Instant::now() + cfg.idle_timeout;
                match incoming {
                    Some(Ok(Message::Binary(frame))) => {
                        let Some(kind) = plausible_envelope(&frame) else {
                            close = Some((CloseCode::Protocol, "not an envelope"));
                            break;
                        };
                        let within = bytes_bucket.take(frame.len() as f64)
                            && msgs_bucket.take(1.0)
                            && (kind != Kind::Presence || presence_bucket.take(1.0));
                        if !within {
                            close = Some((CloseCode::Policy, "rate limit exceeded"));
                            break;
                        }
                        if let Some(tap) = &cfg.tap {
                            tap(&frame);
                        }
                        broadcast(&state, &room, id, &frame);
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                    Some(Ok(Message::Text(_))) => {
                        close = Some((CloseCode::Unsupported, "binary frames only"));
                        break;
                    }
                    Some(Ok(Message::Close(_) | Message::Frame(_))) | Some(Err(_)) | None => break,
                }
            }
            outgoing = rx.recv() => {
                match outgoing {
                    Some(frame) => {
                        if sink.send(Message::Binary(frame)).await.is_err() {
                            break;
                        }
                    }
                    // Removed from the room (too slow): hang up.
                    None => {
                        close = Some((CloseCode::Again, "too slow"));
                        break;
                    }
                }
            }
            _ = ping.tick() => {
                if sink.send(Message::Ping(Bytes::new())).await.is_err() {
                    break;
                }
            }
            () = sleep_until(deadline) => {
                close = Some((CloseCode::Away, "idle"));
                break;
            }
        }
    }

    if let Some((code, reason)) = close {
        let _ = timeout(
            Duration::from_secs(1),
            sink.send(Message::Close(Some(CloseFrame {
                code,
                reason: reason.into(),
            }))),
        )
        .await;
        info!(conn = id, room = log_tag(&room), reason, "closed by relay");
    }
    let remaining = leave(&state, &room, id);
    info!(conn = id, room = log_tag(&room), remaining, "left");
}
