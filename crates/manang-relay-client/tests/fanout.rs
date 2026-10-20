//! The client against the in-process fixture: frames reach the other peer of
//! a room and never echo back, and an outage is reported as `Down`.

use std::time::{Duration, Instant};

use manang_live::transport::{SyncTransport, TransportEvent};
use manang_relay_client::RelayClient;
use tokio::runtime::Runtime;

const ROOM: &str = "0123456789abcdef0123456789abcdef";

fn next(client: &mut RelayClient, limit: Duration) -> Option<TransportEvent> {
    let end = Instant::now() + limit;
    while Instant::now() < end {
        if let Some(event) = client.poll() {
            return Some(event);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    None
}

fn wait_up(client: &mut RelayClient) {
    assert!(matches!(
        next(client, Duration::from_secs(5)),
        Some(TransportEvent::Up)
    ));
}

#[test]
fn frames_reach_the_other_peer_only_and_an_outage_goes_down() {
    let rt = Runtime::new().expect("runtime");
    let relay = rt
        .block_on(manang_live_testkit::spawn(
            "127.0.0.1:0".parse().expect("addr"),
        ))
        .expect("fixture binds");
    let url = format!("ws://{}/room/{ROOM}", relay.local_addr());
    let mut a = RelayClient::new(url.clone(), rt.handle().clone());
    let mut b = RelayClient::new(url, rt.handle().clone());
    a.connect().expect("connect");
    b.connect().expect("connect");
    wait_up(&mut a);
    wait_up(&mut b);

    a.send(b"hello".to_vec()).expect("send");
    match next(&mut b, Duration::from_secs(5)) {
        Some(TransportEvent::Frame(f)) => assert_eq!(f, b"hello"),
        other => panic!("expected the frame, got {other:?}"),
    }
    // The sender never hears its own frame.
    assert!(next(&mut a, Duration::from_millis(300)).is_none());

    rt.block_on(relay.stop());
    assert!(matches!(
        next(&mut a, Duration::from_secs(5)),
        Some(TransportEvent::Down)
    ));
}
