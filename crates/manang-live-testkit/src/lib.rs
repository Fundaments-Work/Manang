//! A test fixture, not a relay: no limits, no hardening, never run it as a
//! service.
//!
//! It is the smallest in-process WebSocket fan-out that lets the live-editing
//! tests run two clients against each other. It accepts connections on
//! `/room/<32 hex chars>` and forwards every binary frame to the other
//! sockets of the same room, never back to the sender. It has no frame size
//! cap, no rate limit, no timeouts and no room or peer limits, and it does not
//! check what it forwards.
//!
//! Tests can observe the forwarded frames with [`spawn_with_tap`] and take the
//! fixture down with [`Handle::stop`] (every connection drops), then bring it
//! back on the same port with [`spawn`] to exercise reconnecting.

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::{JoinHandle, JoinSet};
use tokio_tungstenite::tungstenite::Bytes;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{
    Callback, ErrorResponse, Request, Response,
};
use tokio_tungstenite::tungstenite::http::StatusCode;

/// Called with every frame the fixture forwards.
pub type Tap = Arc<dyn Fn(&[u8]) + Send + Sync>;

type Peers = Vec<(u64, mpsc::UnboundedSender<Bytes>)>;

#[derive(Default)]
struct State {
    rooms: Mutex<HashMap<String, Peers>>,
    next_id: Mutex<u64>,
    tap: Option<Tap>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A running fixture.
pub struct Handle {
    addr: SocketAddr,
    task: JoinHandle<()>,
}

impl Handle {
    /// The address it listens on (useful after binding port 0).
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stop listening and drop every connection. When this returns the port
    /// is free again.
    pub async fn stop(self) {
        self.task.abort();
        let _ = self.task.await;
    }
}

/// Start the fixture on `addr`.
///
/// # Errors
/// Returns the bind error.
pub async fn spawn(addr: SocketAddr) -> io::Result<Handle> {
    start(addr, None).await
}

/// Like [`spawn`], calling `tap` with every forwarded frame.
///
/// # Errors
/// Returns the bind error.
pub async fn spawn_with_tap(addr: SocketAddr, tap: Tap) -> io::Result<Handle> {
    start(addr, Some(tap)).await
}

async fn start(addr: SocketAddr, tap: Option<Tap>) -> io::Result<Handle> {
    let listener = TcpListener::bind(addr).await?;
    let addr = listener.local_addr()?;
    let state = Arc::new(State {
        tap,
        ..State::default()
    });
    let task = tokio::spawn(async move {
        // Dropping the set (when this task is aborted) aborts every
        // connection and so closes its socket.
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    if let Ok((stream, _)) = accepted {
                        connections.spawn(serve(stream, Arc::clone(&state)));
                    }
                }
                Some(_) = connections.join_next() => {}
            }
        }
    });
    Ok(Handle { addr, task })
}

/// Handshake callback: accept only `/room/<32 hex>` and remember the room.
struct RoomPath(Arc<Mutex<Option<String>>>);

impl Callback for RoomPath {
    fn on_request(self, req: &Request, resp: Response) -> Result<Response, ErrorResponse> {
        let room = req
            .uri()
            .path()
            .strip_prefix("/room/")
            .filter(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()));
        match room {
            Some(id) => {
                *lock(&self.0) = Some(id.to_owned());
                Ok(resp)
            }
            None => {
                let mut err = ErrorResponse::new(None);
                *err.status_mut() = StatusCode::NOT_FOUND;
                Err(err)
            }
        }
    }
}

async fn serve(stream: TcpStream, state: Arc<State>) {
    let room_slot = Arc::new(Mutex::new(None));
    let Ok(ws) =
        tokio_tungstenite::accept_hdr_async(stream, RoomPath(Arc::clone(&room_slot))).await
    else {
        return;
    };
    let Some(room) = lock(&room_slot).take() else {
        return;
    };
    let (tx, mut rx) = mpsc::unbounded_channel::<Bytes>();
    let id = {
        let mut next = lock(&state.next_id);
        *next += 1;
        *next
    };
    lock(&state.rooms)
        .entry(room.clone())
        .or_default()
        .push((id, tx));

    let (mut sink, mut stream) = ws.split();
    loop {
        tokio::select! {
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Binary(frame))) => {
                    if let Some(tap) = &state.tap {
                        tap(&frame);
                    }
                    if let Some(peers) = lock(&state.rooms).get(&room) {
                        for (peer, out) in peers {
                            if *peer != id {
                                let _ = out.send(frame.clone());
                            }
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            Some(frame) = rx.recv() => {
                if sink.send(Message::Binary(frame)).await.is_err() {
                    break;
                }
            }
        }
    }
    let mut rooms = lock(&state.rooms);
    if let Some(peers) = rooms.get_mut(&room) {
        peers.retain(|(peer, _)| *peer != id);
        if peers.is_empty() {
            rooms.remove(&room);
        }
    }
}
