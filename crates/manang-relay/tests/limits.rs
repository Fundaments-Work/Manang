//! Relay hardening: health, routing, limits, hostile frames.

use std::net::SocketAddr;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use manang_relay::{RelayConfig, RunningRelay, spawn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::{Bytes, Message};

const ROOM: &str = "00112233445566778899aabbccddeeff";

async fn start(config: RelayConfig) -> RunningRelay {
    spawn("127.0.0.1:0".parse().expect("addr"), config)
        .await
        .expect("relay")
}

fn url(addr: SocketAddr, room: &str) -> String {
    format!("ws://{addr}/room/{room}")
}

/// A frame that passes the relay's plausibility check.
fn envelope(len: usize, kind: u8) -> Bytes {
    let mut v = vec![0u8; len.max(42)];
    v[0] = 1;
    v[1] = kind;
    Bytes::from(v)
}

async fn next_close_or_end(
    ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
) -> bool {
    loop {
        match tokio::time::timeout(Duration::from_secs(3), ws.next()).await {
            Ok(Some(Ok(Message::Close(_)))) | Ok(Some(Err(_))) | Ok(None) => return true,
            Ok(Some(Ok(_))) => {}
            Err(_) => return false,
        }
    }
}

#[tokio::test]
async fn health_endpoint_answers_ok() {
    let relay = start(RelayConfig::default()).await;
    let mut s = TcpStream::connect(relay.addr).await.expect("connect");
    s.write_all(b"GET /health HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .expect("write");
    let mut body = String::new();
    s.read_to_string(&mut body).await.expect("read");
    assert!(body.starts_with("HTTP/1.1 200"));
    assert!(body.ends_with("ok\n"));
    relay.stop().await;
}

#[tokio::test]
async fn bad_paths_are_refused() {
    let relay = start(RelayConfig::default()).await;
    for bad in [
        "/",
        "/room/",
        "/room/ZZ112233445566778899aabbccddeeff",
        "/room/00112233445566778899aabbccddeef",
        "/room/00112233445566778899AABBCCDDEEFF",
        "/other/00112233445566778899aabbccddeeff",
    ] {
        let r = connect_async(format!("ws://{}{bad}", relay.addr)).await;
        assert!(r.is_err(), "{bad} was accepted");
    }
    relay.stop().await;
}

#[tokio::test]
async fn frames_reach_other_peers_in_the_same_room_only() {
    let relay = start(RelayConfig::default()).await;
    let (mut a, _) = connect_async(url(relay.addr, ROOM)).await.expect("a");
    let (mut b, _) = connect_async(url(relay.addr, ROOM)).await.expect("b");
    let (mut other, _) = connect_async(url(relay.addr, "ffeeddccbbaa99887766554433221100"))
        .await
        .expect("other");
    tokio::time::sleep(Duration::from_millis(100)).await;
    let frame = envelope(100, 1);
    a.send(Message::Binary(frame.clone())).await.expect("send");
    let got = tokio::time::timeout(Duration::from_secs(2), b.next())
        .await
        .expect("timeout")
        .expect("some")
        .expect("ok");
    assert_eq!(got, Message::Binary(frame));
    // Not echoed to the sender, not leaked to the other room.
    assert!(
        tokio::time::timeout(Duration::from_millis(300), a.next())
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(300), other.next())
            .await
            .is_err()
    );
    relay.stop().await;
}

#[tokio::test]
async fn hostile_frames_close_the_connection() {
    let relay = start(RelayConfig {
        max_frame_bytes: 4096,
        ..RelayConfig::default()
    })
    .await;
    // Text frame.
    let (mut ws, _) = connect_async(url(relay.addr, ROOM)).await.expect("ws");
    ws.send(Message::text("hello")).await.expect("send");
    assert!(next_close_or_end(&mut ws).await, "text not rejected");
    // Not an envelope (wrong version byte).
    let (mut ws, _) = connect_async(url(relay.addr, ROOM)).await.expect("ws");
    ws.send(Message::Binary(Bytes::from(vec![9u8; 100])))
        .await
        .expect("send");
    assert!(next_close_or_end(&mut ws).await, "garbage not rejected");
    // Too short.
    let (mut ws, _) = connect_async(url(relay.addr, ROOM)).await.expect("ws");
    ws.send(Message::Binary(Bytes::from(vec![1u8, 1, 0])))
        .await
        .expect("send");
    assert!(next_close_or_end(&mut ws).await, "short frame not rejected");
    // Oversized.
    let (mut ws, _) = connect_async(url(relay.addr, ROOM)).await.expect("ws");
    ws.send(Message::Binary(envelope(10_000, 1)))
        .await
        .expect("send");
    assert!(next_close_or_end(&mut ws).await, "oversize not rejected");
    relay.stop().await;
}

#[tokio::test]
async fn rate_limits_close_flooders_but_not_polite_peers() {
    let relay = start(RelayConfig {
        rate_msgs_per_sec: 10,
        ..RelayConfig::default()
    })
    .await;
    let (mut polite, _) = connect_async(url(relay.addr, ROOM)).await.expect("polite");
    for _ in 0..5 {
        polite
            .send(Message::Binary(envelope(60, 1)))
            .await
            .expect("send");
    }
    let (mut flood, _) = connect_async(url(relay.addr, ROOM)).await.expect("flood");
    for _ in 0..200 {
        if flood.send(Message::Binary(envelope(60, 1))).await.is_err() {
            break;
        }
    }
    assert!(next_close_or_end(&mut flood).await, "flooder not closed");
    // The polite peer is still connected and can still send.
    polite
        .send(Message::Binary(envelope(60, 2)))
        .await
        .expect("still open");
    relay.stop().await;
}

#[tokio::test]
async fn room_and_peer_limits() {
    let relay = start(RelayConfig {
        max_peers_per_room: 2,
        max_rooms: 1,
        ..RelayConfig::default()
    })
    .await;
    let (_a, _) = connect_async(url(relay.addr, ROOM)).await.expect("a");
    let (_b, _) = connect_async(url(relay.addr, ROOM)).await.expect("b");
    let (mut c, _) = connect_async(url(relay.addr, ROOM)).await.expect("c");
    assert!(next_close_or_end(&mut c).await, "third peer not refused");
    let (mut d, _) = connect_async(url(relay.addr, "ffeeddccbbaa99887766554433221100"))
        .await
        .expect("d");
    assert!(next_close_or_end(&mut d).await, "second room not refused");
    relay.stop().await;
}

#[tokio::test]
async fn idle_connections_are_closed_and_rooms_forgotten() {
    let relay = start(RelayConfig {
        idle_timeout: Duration::from_millis(300),
        ping_interval: Duration::from_secs(60),
        max_rooms: 1,
        ..RelayConfig::default()
    })
    .await;
    let (mut a, _) = connect_async(url(relay.addr, ROOM)).await.expect("a");
    assert!(next_close_or_end(&mut a).await, "idle peer not closed");
    // The room is gone, so a different room fits under max_rooms = 1.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let (mut b, _) = connect_async(url(relay.addr, "ffeeddccbbaa99887766554433221100"))
        .await
        .expect("b");
    b.send(Message::Binary(envelope(60, 1)))
        .await
        .expect("send");
    relay.stop().await;
}
