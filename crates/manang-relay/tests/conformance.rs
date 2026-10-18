//! Conformance of a relay running elsewhere, such as the Cloudflare Worker in
//! `relay/worker` under `wrangler dev`, to the contract the Rust `RelayClient`
//! expects. Skipped unless `MANANG_RELAY_URL` names a relay origin, for example
//! `MANANG_RELAY_URL=ws://127.0.0.1:8787`. `relay/worker/scripts/conformance.mjs`
//! starts the Worker and runs this test.

use std::time::{Duration, Instant};

use manang_live::transport::{SyncTransport, TransportEvent};
use manang_relay::RelayClient;
use tokio::runtime::Runtime;

/// A valid-looking envelope: version 1, the kind, then filler.
fn envelope(len: usize, kind: u8, fill: u8) -> Vec<u8> {
    let mut f = vec![fill; len];
    f[0] = 1;
    f[1] = kind;
    f
}

fn room_hex(salt: u8) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{:030x}{salt:02x}", nanos & ((1u128 << 120) - 1))
}

/// Poll until `want` matches an event or five seconds pass; other events are
/// discarded.
fn wait_for(
    t: &mut RelayClient,
    mut want: impl FnMut(&TransportEvent) -> bool,
) -> Option<TransportEvent> {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        match t.poll() {
            Some(ev) if want(&ev) => return Some(ev),
            Some(_) => {}
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    None
}

fn connected(rt: &Runtime, base: &str, room: &str) -> Option<RelayClient> {
    let mut c = RelayClient::new(format!("{base}/room/{room}"), rt.handle().clone());
    c.connect().ok()?;
    wait_for(&mut c, |e| matches!(e, TransportEvent::Up))?;
    Some(c)
}

#[test]
fn relay_at_url_follows_the_contract() {
    let Ok(base) = std::env::var("MANANG_RELAY_URL") else {
        eprintln!("MANANG_RELAY_URL not set; skipping");
        return;
    };
    let base = base.trim_end_matches('/').to_owned();
    let Ok(rt) = Runtime::new() else {
        panic!("tokio runtime");
    };
    let room = room_hex(1);

    // Fan-out: a frame reaches the others byte for byte and is never echoed.
    let (Some(mut a), Some(mut b), Some(mut c)) = (
        connected(&rt, &base, &room),
        connected(&rt, &base, &room),
        connected(&rt, &base, &room),
    ) else {
        panic!("three clients should connect");
    };
    let data = envelope(100, 1, 0xab);
    a.send(data.clone()).expect("send");
    let got = |t: &mut RelayClient, want: &[u8]| {
        wait_for(t, |e| matches!(e, TransportEvent::Frame(f) if f == want)).is_some()
    };
    assert!(got(&mut b, &data), "b receives a's frame");
    assert!(got(&mut c, &data), "c receives a's frame");
    let presence = envelope(60, 2, 0x11);
    b.send(presence.clone()).expect("send");
    assert!(got(&mut a, &presence), "a receives b's presence frame");
    std::thread::sleep(Duration::from_millis(300));
    while let Some(ev) = a.poll() {
        assert!(
            !matches!(ev, TransportEvent::Frame(ref f) if *f == data),
            "a must not get its own frame back"
        );
    }

    // Another room is separate.
    let Some(mut other) = connected(&rt, &base, &room_hex(2)) else {
        panic!("other room connects");
    };
    a.send(envelope(80, 1, 0x22)).expect("send");
    std::thread::sleep(Duration::from_millis(300));
    while let Some(ev) = other.poll() {
        assert!(
            !matches!(ev, TransportEvent::Frame(_)),
            "rooms must be isolated"
        );
    }

    // Not an envelope: the relay hangs up on the sender only.
    let mut bad = connected(&rt, &base, &room).expect("bad connects");
    bad.send(vec![0u8; 60]).expect("send");
    assert!(
        wait_for(&mut bad, |e| matches!(e, TransportEvent::Down)).is_some(),
        "a non-envelope closes the sender"
    );
    a.send(envelope(60, 1, 0x33)).expect("send");
    assert!(got(&mut b, &envelope(60, 1, 0x33)), "room still works");

    // A wrong path never comes up.
    let mut wrong = RelayClient::new(format!("{base}/rooms/{room}"), rt.handle().clone());
    wrong.connect().expect("connect");
    assert!(
        wait_for(&mut wrong, |e| matches!(e, TransportEvent::Down)).is_some(),
        "wrong path is refused"
    );

    // Room limit (16 peers): the 17th is accepted, then closed with 1013.
    let full = room_hex(3);
    let mut peers = Vec::new();
    for _ in 0..16 {
        peers.push(connected(&rt, &base, &full).expect("peer connects"));
    }
    let mut extra = RelayClient::new(format!("{base}/room/{full}"), rt.handle().clone());
    extra.connect().expect("connect");
    assert!(
        wait_for(&mut extra, |e| matches!(e, TransportEvent::Down)).is_some(),
        "the 17th peer is turned away"
    );
}
