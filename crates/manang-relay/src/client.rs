//! A [`SyncTransport`] over a WebSocket to the relay.
//!
//! The core crate is synchronous and poll-based; this adapter owns the async
//! I/O. `connect` spawns one task on the supplied Tokio runtime handle that
//! pumps frames in both directions and reports `Up`, `Frame` and `Down` events
//! through a queue the caller drains with `poll`. Only I/O happens on the
//! runtime: the CRDT work stays on the caller's thread.
//!
//! `wss://` needs a TLS connector, which the desktop app supplies (rustls with
//! the ring provider, as its other network code does); this crate builds with
//! no TLS and no C dependencies so it stays easy to audit.

use std::sync::mpsc as std_mpsc;

use futures_util::{SinkExt, StreamExt};
use manang_live::LiveError;
use manang_live::transport::{SyncTransport, TransportEvent};
use tokio::runtime::Handle;
use tokio::sync::mpsc as tokio_mpsc;
use tokio_tungstenite::tungstenite::{Bytes, Message};

/// WebSocket transport to a relay room.
pub struct RelayClient {
    url: String,
    handle: Handle,
    events_tx: std_mpsc::Sender<TransportEvent>,
    events_rx: std_mpsc::Receiver<TransportEvent>,
    out_tx: Option<tokio_mpsc::UnboundedSender<Vec<u8>>>,
}

impl RelayClient {
    /// A client for `url` (for example `ws://127.0.0.1:8787/room/<hex>`),
    /// doing its I/O on `handle`'s runtime. Nothing connects until
    /// [`SyncTransport::connect`].
    #[must_use]
    pub fn new(url: impl Into<String>, handle: Handle) -> Self {
        let (events_tx, events_rx) = std_mpsc::channel();
        Self {
            url: url.into(),
            handle,
            events_tx,
            events_rx,
            out_tx: None,
        }
    }
}

async fn run(
    url: String,
    events: std_mpsc::Sender<TransportEvent>,
    mut outbound: tokio_mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let Ok((ws, _)) = tokio_tungstenite::connect_async(url).await else {
        let _ = events.send(TransportEvent::Down);
        return;
    };
    let _ = events.send(TransportEvent::Up);
    let (mut sink, mut stream) = ws.split();
    loop {
        tokio::select! {
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Binary(b))) => {
                    if events.send(TransportEvent::Frame(b.to_vec())).is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            out = outbound.recv() => match out {
                Some(frame) => {
                    if sink.send(Message::Binary(Bytes::from(frame))).await.is_err() {
                        break;
                    }
                }
                // The transport was closed or reconnected: leave quietly.
                None => {
                    let _ = sink.send(Message::Close(None)).await;
                    return;
                }
            },
        }
    }
    let _ = events.send(TransportEvent::Down);
}

impl SyncTransport for RelayClient {
    fn connect(&mut self) -> Result<(), LiveError> {
        let (tx, rx) = tokio_mpsc::unbounded_channel();
        // Replacing the sender ends any previous connection task.
        self.out_tx = Some(tx);
        self.handle
            .spawn(run(self.url.clone(), self.events_tx.clone(), rx));
        Ok(())
    }

    fn send(&mut self, frame: Vec<u8>) -> Result<(), LiveError> {
        if let Some(tx) = &self.out_tx {
            // A closed channel means the connection task ended; the frame is
            // dropped and the resync after reconnecting recovers it.
            let _ = tx.send(frame);
        }
        Ok(())
    }

    fn poll(&mut self) -> Option<TransportEvent> {
        self.events_rx.try_recv().ok()
    }

    fn close(&mut self) {
        self.out_tx = None;
    }
}
