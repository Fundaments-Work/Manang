//! `wss://` support, behind the `tls` feature: a TLS address that does not
//! speak TLS fails cleanly instead of hanging.

#![cfg(feature = "tls")]

use std::time::{Duration, Instant};

use manang_live::transport::{SyncTransport, TransportEvent};
use manang_relay::RelayClient;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;

#[test]
fn wss_to_a_server_that_does_not_speak_tls_goes_down() {
    let rt = Runtime::new().expect("runtime");
    // A TCP port that accepts and then says nothing useful.
    let listener = rt.block_on(TcpListener::bind("127.0.0.1:0")).expect("bind");
    let addr = listener.local_addr().expect("addr");
    rt.spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let _ = socket.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
            let _ = socket.shutdown().await;
        }
    });
    let mut client = RelayClient::new(
        format!("wss://{addr}/room/00000000000000000000000000000000"),
        rt.handle().clone(),
    );
    client.connect().expect("connect starts");
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        match client.poll() {
            Some(TransportEvent::Down) => break,
            Some(TransportEvent::Up) => panic!("a TLS handshake cannot succeed here"),
            _ => {}
        }
        assert!(
            Instant::now() < end,
            "the failed connection was never reported"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
