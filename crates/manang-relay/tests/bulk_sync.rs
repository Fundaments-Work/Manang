//! A project's first sync as the relay sees it: the paced bulk traffic that
//! `manang_live::pacer::Pacer` produces (about 2 MiB/s, 100 messages/s, frames
//! up to 1 MiB) must pass a relay with its default limits (200 messages/s,
//! 4 MiB/s, 16 MiB frames) without any peer being dropped.
//!
//! The workload is a host answering a 100-file join: one manifest page, 70 text
//! files (two small frames each), 25 small blobs (one chunk) and 2 blobs of
//! 1.5 MiB (a 1 MiB and a 0.5 MiB chunk each). Runs in about 3 seconds of real
//! time (the relay's token buckets use the wall clock).
//!
//! Against the Rust relay, always. Against another relay (the Cloudflare
//! Worker under `wrangler dev`), only when `MANANG_RELAY_URL` names its `ws://`
//! origin (this test has no TLS client),
//! for example `MANANG_RELAY_URL=ws://127.0.0.1:8787 cargo test -p
//! manang-relay --test bulk_sync`; `relay/worker/scripts/conformance.mjs` sets
//! that variable and runs the whole suite.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use manang_live::pacer::{Class, PaceConfig, Pacer};
use manang_relay::{RelayConfig, spawn};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::{Bytes, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

/// A frame the relay's plausibility check accepts, tagged with `id` so the
/// receiver can check order.
fn frame(len: usize, id: u32) -> Vec<u8> {
    let mut v = vec![0u8; len.max(48)];
    v[0] = 1;
    v[1] = 1;
    v[2..6].copy_from_slice(&id.to_be_bytes());
    v
}

fn room(salt: u8) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{:030x}{salt:02x}", nanos & ((1u128 << 120) - 1))
}

/// The host's side of a 100-file join, in the order the session queues it.
fn workload() -> Vec<(Class, Vec<u8>)> {
    let mut out = Vec::new();
    let mut id = 0u32;
    let mut push = |class, len| {
        out.push((class, frame(len, id)));
        id += 1;
    };
    push(Class::Priority, 12 * 1024); // manifest page
    for _ in 0..70 {
        push(Class::Normal, 3 * 1024); // DocSync reply
        push(Class::Normal, 3 * 1024); // DocSync final
    }
    for _ in 0..25 {
        push(Class::Bulk, 60 * 1024); // blob, one chunk
    }
    for _ in 0..2 {
        push(Class::Bulk, 1024 * 1024); // 1.5 MiB blob: two chunks
        push(Class::Bulk, 512 * 1024);
    }
    out
}

struct Sent {
    at: Duration,
    len: usize,
}

/// Highest count and byte total inside any one-second window.
fn peak_per_second(sent: &[Sent]) -> (usize, usize) {
    let (mut peak_msgs, mut peak_bytes) = (0, 0);
    let mut lo = 0;
    let mut bytes = 0;
    for hi in 0..sent.len() {
        bytes += sent[hi].len;
        while sent[hi].at - sent[lo].at >= Duration::from_secs(1) {
            bytes -= sent[lo].len;
            lo += 1;
        }
        peak_msgs = peak_msgs.max(hi - lo + 1);
        peak_bytes = peak_bytes.max(bytes);
    }
    (peak_msgs, peak_bytes)
}

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Read every expected frame, checking size and order; a close, an error or
/// a 15 s silence is a dropped peer.
async fn drain(mut ws: Ws, mut want: VecDeque<Vec<u8>>) -> (Ws, usize) {
    let mut got = 0usize;
    while !want.is_empty() {
        match tokio::time::timeout(Duration::from_secs(15), ws.next()).await {
            Ok(Some(Ok(Message::Binary(b)))) => {
                let next = want.pop_front().expect("checked non-empty");
                assert_eq!(b.len(), next.len(), "frame {got} changed size");
                assert_eq!(b[2..6], next[2..6], "frame {got} out of order");
                got += 1;
            }
            Ok(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => {}
            other => panic!("receiver dropped after {got} frames: {other:?}"),
        }
    }
    (ws, got)
}

async fn replay(base: &str) {
    let url = format!("{base}/room/{}", room(1));
    let (mut host, _) = connect_async(&url).await.expect("host");
    let (guest, _) = connect_async(&url).await.expect("guest");
    let (watcher, _) = connect_async(&url).await.expect("watcher");
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut expected: VecDeque<Vec<u8>> = VecDeque::new();
    let mut pacer = Pacer::new(PaceConfig::default());
    for (class, f) in workload() {
        expected.push_back(f.clone());
        pacer.push(class, f);
    }
    let total = expected.len();
    let total_bytes: usize = expected.iter().map(Vec::len).sum();

    // Two receivers read everything as it arrives, in order.
    let guest_task = tokio::spawn(drain(guest, expected.clone()));
    let watcher_task = tokio::spawn(drain(watcher, expected.clone()));

    // The sender: tick every 10 ms, send what the pacer releases.
    let start = Instant::now();
    let mut sent: Vec<Sent> = Vec::new();
    let mut out = Vec::new();
    while sent.len() < total {
        out.clear();
        pacer.drain(start.elapsed().as_millis() as u64, &mut out);
        for f in out.drain(..) {
            sent.push(Sent {
                at: start.elapsed(),
                len: f.len(),
            });
            host.send(Message::Binary(Bytes::from(f)))
                .await
                .expect("the host was dropped while sending");
        }
        assert!(start.elapsed() < Duration::from_secs(30), "pacer stalled");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let (mut guest, got) = guest_task.await.expect("guest task");
    assert_eq!(got, total);
    let (_watcher, got) = watcher_task.await.expect("watcher task");
    assert_eq!(got, total);

    // Nobody was closed: the host is still connected (a close or error would
    // be the next thing on its stream), and the room still carries frames.
    assert!(
        tokio::time::timeout(Duration::from_millis(300), host.next())
            .await
            .is_err(),
        "the host was closed by the relay"
    );
    guest
        .send(Message::Binary(Bytes::from(frame(64, 9999))))
        .await
        .expect("the guest was dropped");
    let echo = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match host.next().await {
                Some(Ok(Message::Binary(b))) => break b,
                Some(Ok(_)) => {}
                other => panic!("host dropped: {other:?}"),
            }
        }
    })
    .await
    .expect("the room stopped forwarding");
    assert_eq!(echo.len(), 64);

    // The sender stayed inside the relay's limits, and about at the pacer's.
    let (msgs, bytes) = peak_per_second(&sent);
    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "{total} frames, {:.2} MiB in {elapsed:.2} s; peak {msgs} msg/s, {:.2} MiB/s in one second",
        total_bytes as f64 / 1_048_576.0,
        bytes as f64 / 1_048_576.0
    );
    assert!(msgs <= 200, "{msgs} messages in one second");
    assert!(bytes <= 4 * 1024 * 1024, "{bytes} bytes in one second");
    assert!(sent.iter().all(|s| s.len <= 1024 * 1024 + 4096));
    // Paced means paced: not faster than the pacer's sustained rates plus its
    // burst.
    assert!(
        total_bytes as f64 <= 2.0 * 1_048_576.0 * elapsed + 512.0 * 1024.0 + 1_048_576.0,
        "bytes left faster than the pacer allows"
    );
}

#[tokio::test]
async fn paced_bulk_sync_passes_the_relay_without_drops() {
    let relay = spawn("127.0.0.1:0".parse().expect("addr"), RelayConfig::default())
        .await
        .expect("relay");
    replay(&format!("ws://{}", relay.addr)).await;
    relay.stop().await;
}

/// The same replay against the Worker (or any relay) named by the
/// environment; skipped when it is not set.
#[tokio::test]
async fn paced_bulk_sync_passes_a_relay_named_by_the_environment() {
    let Ok(base) = std::env::var("MANANG_RELAY_URL") else {
        return;
    };
    replay(base.trim_end_matches('/')).await;
}

/// A connection's burst is two seconds of its byte rate, so the largest frame
/// it can ever send is 8 MiB (half the 16 MiB frame limit): the frame limit
/// is the outer bound, the byte bucket the effective one. The protocol never
/// goes near it (chunks are 1 MiB).
#[tokio::test]
async fn frame_limits_as_enforced() {
    let relay = spawn("127.0.0.1:0".parse().expect("addr"), RelayConfig::default())
        .await
        .expect("relay");
    let url = format!("ws://{}/room/{}", relay.addr, room(2));
    let (mut a, _) = connect_async(&url).await.expect("a");
    let (mut b, _) = connect_async(&url).await.expect("b");
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 1 MiB (the protocol's biggest frame) passes.
    a.send(Message::Binary(Bytes::from(frame(1024 * 1024, 1))))
        .await
        .expect("send");
    let got = tokio::time::timeout(Duration::from_secs(5), b.next())
        .await
        .expect("timeout")
        .expect("some")
        .expect("ok");
    assert!(matches!(got, Message::Binary(ref x) if x.len() == 1024 * 1024));

    // Over the byte bucket (8 MiB) but under the frame limit: refused.
    let (mut c, _) = connect_async(&url).await.expect("c");
    c.send(Message::Binary(Bytes::from(frame(8 * 1024 * 1024 + 1, 2))))
        .await
        .expect("send");
    let closed = loop {
        match tokio::time::timeout(Duration::from_secs(10), c.next()).await {
            Ok(Some(Ok(Message::Close(_)) | Err(_)) | None) => break true,
            Ok(Some(Ok(_))) => {}
            Err(_) => break false,
        }
    };
    assert!(closed, "an 8 MiB + 1 frame was not refused");
    relay.stop().await;
}
